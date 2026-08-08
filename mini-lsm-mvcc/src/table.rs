// Copyright (c) 2022-2026 Alex Chi Z
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

pub(crate) mod bloom;
mod builder;
mod iterator;

use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail, ensure};
pub use builder::SsTableBuilder;
use bytes::BufMut;
pub use iterator::SsTableIterator;

use crate::block::Block;
use crate::key::{KeyBytes, KeySlice};
use crate::lsm_storage::BlockCache;

use self::bloom::Bloom;

fn take_bytes<'a>(buf: &mut &'a [u8], len: usize, what: &str) -> Result<&'a [u8]> {
    ensure!(buf.len() >= len, "{what} is truncated");
    let (value, rest) = buf.split_at(len);
    *buf = rest;
    Ok(value)
}

fn take_u16(buf: &mut &[u8], what: &str) -> Result<u16> {
    let value = take_bytes(buf, std::mem::size_of::<u16>(), what)?;
    Ok(u16::from_be_bytes([value[0], value[1]]))
}

fn take_u32(buf: &mut &[u8], what: &str) -> Result<u32> {
    let value = take_bytes(buf, std::mem::size_of::<u32>(), what)?;
    Ok(u32::from_be_bytes([value[0], value[1], value[2], value[3]]))
}

fn take_u64(buf: &mut &[u8], what: &str) -> Result<u64> {
    let value = take_bytes(buf, std::mem::size_of::<u64>(), what)?;
    Ok(u64::from_be_bytes([
        value[0], value[1], value[2], value[3], value[4], value[5], value[6], value[7],
    ]))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockMeta {
    /// Offset of this data block.
    pub offset: usize,
    /// The first key of the data block.
    pub first_key: KeyBytes,
    /// The last key of the data block.
    pub last_key: KeyBytes,
}

impl BlockMeta {
    /// Encode block meta to a buffer.
    pub fn encode_block_meta(
        block_meta: &[BlockMeta],
        max_ts: u64,
        buf: &mut Vec<u8>,
    ) -> Result<()> {
        let mut estimated_size = std::mem::size_of::<u32>(); // number of blocks
        for meta in block_meta {
            // The size of offset
            estimated_size += std::mem::size_of::<u32>();
            // The size of key length
            estimated_size += std::mem::size_of::<u16>();
            // The size of actual key
            estimated_size += meta.first_key.raw_len();
            // The size of key length
            estimated_size += std::mem::size_of::<u16>();
            // The size of actual key
            estimated_size += meta.last_key.raw_len();
        }
        estimated_size += std::mem::size_of::<u64>(); // max timestamp
        estimated_size += std::mem::size_of::<u32>(); // checksum

        // Reserve the space to improve performance, especially when the size of incoming data is
        // large
        buf.reserve(estimated_size);
        let original_len = buf.len();
        buf.put_u32(u32::try_from(block_meta.len()).context("too many SST blocks")?);
        for meta in block_meta {
            buf.put_u32(u32::try_from(meta.offset).context("SST block offset is too large")?);
            buf.put_u16(
                u16::try_from(meta.first_key.key_len()).context("SST first key is too large")?,
            );
            buf.put_slice(meta.first_key.key_ref());
            buf.put_u64(meta.first_key.ts());
            buf.put_u16(
                u16::try_from(meta.last_key.key_len()).context("SST last key is too large")?,
            );
            buf.put_slice(meta.last_key.key_ref());
            buf.put_u64(meta.last_key.ts());
        }
        buf.put_u64(max_ts);
        buf.put_u32(crc32fast::hash(&buf[original_len + 4..]));
        assert_eq!(estimated_size, buf.len() - original_len);
        Ok(())
    }

    /// Decode block meta from a buffer.
    pub fn decode_block_meta(buf: &[u8]) -> Result<(Vec<BlockMeta>, u64)> {
        let trailer_size = std::mem::size_of::<u64>() + std::mem::size_of::<u32>();
        ensure!(
            buf.len() >= std::mem::size_of::<u32>() + trailer_size,
            "SST block metadata is truncated"
        );
        let checksum_offset = buf.len() - std::mem::size_of::<u32>();
        let expected_checksum = u32::from_be_bytes([
            buf[checksum_offset],
            buf[checksum_offset + 1],
            buf[checksum_offset + 2],
            buf[checksum_offset + 3],
        ]);
        let checksum = crc32fast::hash(&buf[std::mem::size_of::<u32>()..checksum_offset]);
        ensure!(expected_checksum == checksum, "meta checksum mismatched");

        let mut cursor = buf;
        let mut block_meta = Vec::new();
        let num = take_u32(&mut cursor, "SST block count")? as usize;
        let minimum_entry_size = std::mem::size_of::<u32>()
            + std::mem::size_of::<u16>() * 2
            + std::mem::size_of::<u64>() * 2;
        ensure!(
            num <= checksum_offset
                .saturating_sub(std::mem::size_of::<u32>() + std::mem::size_of::<u64>())
                / minimum_entry_size,
            "SST block count exceeds the metadata length"
        );
        for _ in 0..num {
            let offset = take_u32(&mut cursor, "SST block offset")? as usize;
            let first_key_len = take_u16(&mut cursor, "SST first-key length")? as usize;
            ensure!(first_key_len > 0, "SST first key is empty");
            let first_key = take_bytes(&mut cursor, first_key_len, "SST first key")?;
            let first_key_ts = take_u64(&mut cursor, "SST first-key timestamp")?;
            let first_key = KeyBytes::from_bytes_with_ts(first_key.to_vec().into(), first_key_ts);
            let last_key_len = take_u16(&mut cursor, "SST last-key length")? as usize;
            ensure!(last_key_len > 0, "SST last key is empty");
            let last_key = take_bytes(&mut cursor, last_key_len, "SST last key")?;
            let last_key_ts = take_u64(&mut cursor, "SST last-key timestamp")?;
            let last_key = KeyBytes::from_bytes_with_ts(last_key.to_vec().into(), last_key_ts);
            block_meta.push(BlockMeta {
                offset,
                first_key,
                last_key,
            });
        }
        ensure!(
            cursor.len() == trailer_size,
            "SST block metadata has trailing or missing bytes"
        );
        let max_ts = take_u64(&mut cursor, "SST maximum timestamp")?;
        let stored_checksum = take_u32(&mut cursor, "SST metadata checksum")?;
        ensure!(stored_checksum == checksum, "meta checksum mismatched");

        Ok((block_meta, max_ts))
    }
}

/// A file object.
pub struct FileObject(Option<File>, u64);

impl FileObject {
    pub fn read(&self, offset: u64, len: u64) -> Result<Vec<u8>> {
        use std::os::unix::fs::FileExt;
        let end = offset
            .checked_add(len)
            .context("file read range overflow")?;
        ensure!(end <= self.1, "file read range is out of bounds");
        let len = usize::try_from(len).context("file read is too large")?;
        let mut data = vec![0; len];
        self.0
            .as_ref()
            .context("cannot read a metadata-only file object")?
            .read_exact_at(&mut data[..], offset)?;
        Ok(data)
    }

    pub fn size(&self) -> u64 {
        self.1
    }

    /// Create a new file object (day 2) and write the file to the disk (day 4).
    pub fn create(path: &Path, data: Vec<u8>) -> Result<Self> {
        std::fs::write(path, &data)?;
        File::open(path)?.sync_all()?;
        Ok(FileObject(
            Some(File::options().read(true).write(false).open(path)?),
            u64::try_from(data.len()).context("SST file is too large")?,
        ))
    }

    pub fn open(path: &Path) -> Result<Self> {
        let file = File::options().read(true).write(false).open(path)?;
        let size = file.metadata()?.len();
        Ok(FileObject(Some(file), size))
    }
}

/// An SSTable.
pub struct SsTable {
    /// The actual storage unit of SsTable, the format is as above.
    pub(crate) file: FileObject,
    /// The meta blocks that hold info for data blocks.
    pub(crate) block_meta: Vec<BlockMeta>,
    /// The offset that indicates the start point of meta blocks in `file`.
    pub(crate) block_meta_offset: usize,
    id: usize,
    block_cache: Option<Arc<BlockCache>>,
    first_key: KeyBytes,
    last_key: KeyBytes,
    pub(crate) bloom: Option<Bloom>,
    max_ts: u64,
}
impl SsTable {
    #[cfg(test)]
    pub(crate) fn open_for_test(file: FileObject) -> Result<Self> {
        Self::open(0, None, file)
    }

    /// Open SSTable from a file.
    pub fn open(id: usize, block_cache: Option<Arc<BlockCache>>, file: FileObject) -> Result<Self> {
        let len = file.size();
        let bloom_trailer_offset = len
            .checked_sub(std::mem::size_of::<u32>() as u64)
            .context("SST bloom-offset trailer is truncated")?;
        let raw_bloom_offset = file.read(bloom_trailer_offset, 4)?;
        let bloom_offset = u32::from_be_bytes([
            raw_bloom_offset[0],
            raw_bloom_offset[1],
            raw_bloom_offset[2],
            raw_bloom_offset[3],
        ]) as u64;
        ensure!(
            bloom_offset <= bloom_trailer_offset,
            "SST bloom offset is out of bounds"
        );
        let meta_trailer_offset = bloom_offset
            .checked_sub(std::mem::size_of::<u32>() as u64)
            .context("SST metadata-offset trailer is truncated")?;
        let raw_bloom = file.read(bloom_offset, bloom_trailer_offset - bloom_offset)?;
        let bloom_filter = Bloom::decode(&raw_bloom)?;
        let raw_meta_offset = file.read(meta_trailer_offset, 4)?;
        let block_meta_offset = u32::from_be_bytes([
            raw_meta_offset[0],
            raw_meta_offset[1],
            raw_meta_offset[2],
            raw_meta_offset[3],
        ]) as u64;
        ensure!(
            block_meta_offset <= meta_trailer_offset,
            "SST block-metadata offset is out of bounds"
        );
        let raw_meta = file.read(block_meta_offset, meta_trailer_offset - block_meta_offset)?;
        let (block_meta, max_ts) = BlockMeta::decode_block_meta(&raw_meta[..])?;
        ensure!(!block_meta.is_empty(), "SST has no data blocks");
        ensure!(
            block_meta[0].offset == 0,
            "first SST block must start at zero"
        );
        ensure!(
            block_meta
                .windows(2)
                .all(|pair| pair[0].offset < pair[1].offset),
            "SST block offsets are not strictly increasing"
        );
        let block_meta_offset_usize =
            usize::try_from(block_meta_offset).context("SST metadata offset is too large")?;
        ensure!(
            block_meta.iter().all(|meta| {
                meta.offset
                    .checked_add(std::mem::size_of::<u32>())
                    .is_some_and(|end| end < block_meta_offset_usize)
            }),
            "SST block offset or checksum is out of bounds"
        );
        Ok(Self {
            file,
            first_key: block_meta[0].first_key.clone(),
            last_key: block_meta[block_meta.len() - 1].last_key.clone(),
            block_meta,
            block_meta_offset: block_meta_offset_usize,
            id,
            block_cache,
            bloom: Some(bloom_filter),
            max_ts,
        })
    }

    /// Create a mock SST with only first key + last key metadata
    pub fn create_meta_only(
        id: usize,
        file_size: u64,
        first_key: KeyBytes,
        last_key: KeyBytes,
    ) -> Self {
        Self {
            file: FileObject(None, file_size),
            block_meta: vec![],
            block_meta_offset: 0,
            id,
            block_cache: None,
            first_key,
            last_key,
            bloom: None,
            max_ts: 0,
        }
    }

    /// Read a block from the disk.
    pub fn read_block(&self, block_idx: usize) -> Result<Arc<Block>> {
        let offset = self
            .block_meta
            .get(block_idx)
            .context("SST block index is out of bounds")?
            .offset;
        let offset_end = self
            .block_meta
            .get(block_idx + 1)
            .map_or(self.block_meta_offset, |x| x.offset);
        let range_len = offset_end
            .checked_sub(offset)
            .context("SST block offsets are reversed")?;
        let block_len = range_len
            .checked_sub(std::mem::size_of::<u32>())
            .context("SST block checksum is truncated")?;
        let block_data_with_chksum: Vec<u8> = self.file.read(offset as u64, range_len as u64)?;
        let block_data = &block_data_with_chksum[..block_len];
        let checksum = u32::from_be_bytes([
            block_data_with_chksum[block_len],
            block_data_with_chksum[block_len + 1],
            block_data_with_chksum[block_len + 2],
            block_data_with_chksum[block_len + 3],
        ]);
        if checksum != crc32fast::hash(block_data) {
            bail!("block checksum mismatched");
        }
        Ok(Arc::new(Block::decode_checked(block_data)?))
    }

    /// Read a block from disk, with block cache.
    pub fn read_block_cached(&self, block_idx: usize) -> Result<Arc<Block>> {
        if let Some(ref block_cache) = self.block_cache {
            let blk = block_cache
                .try_get_with((self.id, block_idx), || self.read_block(block_idx))
                .map_err(|e| anyhow!("{}", e))?;
            Ok(blk)
        } else {
            self.read_block(block_idx)
        }
    }

    /// Find the block that may contain `key`.
    pub fn find_block_idx(&self, key: KeySlice) -> usize {
        self.block_meta
            .partition_point(|meta| meta.first_key.as_key_slice() <= key)
            .saturating_sub(1)
    }

    /// Get number of data blocks.
    pub fn num_of_blocks(&self) -> usize {
        self.block_meta.len()
    }

    pub fn first_key(&self) -> &KeyBytes {
        &self.first_key
    }

    pub fn last_key(&self) -> &KeyBytes {
        &self.last_key
    }

    pub fn table_size(&self) -> u64 {
        self.file.1
    }

    pub fn sst_id(&self) -> usize {
        self.id
    }

    pub fn max_ts(&self) -> u64 {
        self.max_ts
    }
}
