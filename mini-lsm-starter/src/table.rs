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

#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

pub(crate) mod bloom;
mod builder;
mod iterator;

use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Result, ensure};
pub use builder::SsTableBuilder;
use bytes::{Buf, BufMut};
pub use iterator::SsTableIterator;

use crate::block::Block;
use crate::key::{KeyBytes, KeySlice};
use crate::lsm_storage::BlockCache;

use self::bloom::Bloom;

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
    /// You may add extra fields to the buffer,
    /// in order to help keep track of `first_key` when decoding from the same buffer in the future.
    pub fn encode_block_meta(block_meta: &[BlockMeta], buf: &mut Vec<u8>) {
        assert!(block_meta.len() <= u32::MAX as usize);
        let metadata_start = buf.len();
        buf.put_u32(block_meta.len() as u32);
        for meta in block_meta {
            assert!(meta.offset <= u32::MAX as usize);
            assert!(meta.first_key.len() <= u16::MAX as usize);
            assert!(meta.last_key.len() <= u16::MAX as usize);
            buf.put_u32(meta.offset as u32);
            buf.put_u16(meta.first_key.len() as u16);
            buf.extend_from_slice(meta.first_key.raw_ref());
            buf.put_u16(meta.last_key.len() as u16);
            buf.extend_from_slice(meta.last_key.raw_ref());
        }
        let checksum = crc32fast::hash(&buf[metadata_start..]);
        buf.put_u32(checksum);
    }

    /// Decode block meta from a buffer.
    pub fn decode_block_meta(buf: impl Buf) -> Vec<BlockMeta> {
        Self::decode_block_meta_checked(buf).expect("invalid block metadata")
    }

    fn decode_block_meta_checked(mut buf: impl Buf) -> Result<Vec<BlockMeta>> {
        ensure!(buf.remaining() >= 4, "block metadata checksum is missing");
        let mut encoded = vec![0; buf.remaining()];
        buf.copy_to_slice(&mut encoded);
        let payload_end = encoded.len() - 4;
        let stored_checksum = u32::from_be_bytes(encoded[payload_end..].try_into().unwrap());
        ensure!(
            crc32fast::hash(&encoded[..payload_end]) == stored_checksum,
            "block metadata checksum mismatch"
        );

        let mut payload = &encoded[..payload_end];
        ensure!(payload.remaining() >= 4, "block metadata count is missing");
        let count = payload.get_u32() as usize;
        ensure!(
            count <= payload.remaining() / 8,
            "block metadata count exceeds the available bytes"
        );
        let mut block_meta = Vec::with_capacity(count);
        for _ in 0..count {
            ensure!(payload.remaining() >= 6, "truncated block metadata record");
            let offset = payload.get_u32() as usize;
            let first_key_len = payload.get_u16() as usize;
            ensure!(
                payload.remaining() >= first_key_len + 2,
                "truncated first key in block metadata"
            );
            let first_key = KeyBytes::from_bytes(payload.copy_to_bytes(first_key_len));
            let last_key_len = payload.get_u16() as usize;
            ensure!(
                payload.remaining() >= last_key_len,
                "truncated last key in block metadata"
            );
            let last_key = KeyBytes::from_bytes(payload.copy_to_bytes(last_key_len));
            block_meta.push(BlockMeta {
                offset,
                first_key,
                last_key,
            });
        }
        ensure!(payload.remaining() == 0, "trailing block metadata bytes");
        Ok(block_meta)
    }
}

/// A file object.
pub struct FileObject(Option<File>, u64);

impl FileObject {
    pub fn read(&self, offset: u64, len: u64) -> Result<Vec<u8>> {
        use std::os::unix::fs::FileExt;
        let mut data = vec![0; len as usize];
        self.0
            .as_ref()
            .unwrap()
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
            data.len() as u64,
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
    /// The maximum timestamp stored in this SST, implemented in week 3.
    max_ts: u64,
}

impl SsTable {
    #[cfg(test)]
    pub(crate) fn open_for_test(file: FileObject) -> Result<Self> {
        Self::open(0, None, file)
    }

    /// Open SSTable from a file.
    pub fn open(id: usize, block_cache: Option<Arc<BlockCache>>, file: FileObject) -> Result<Self> {
        let file_size = usize::try_from(file.size())?;
        ensure!(file_size >= 8, "SST is too short for its trailer");

        let bloom_offset_bytes = file.read((file_size - 4) as u64, 4)?;
        let bloom_offset = u32::from_be_bytes(bloom_offset_bytes.try_into().unwrap()) as usize;
        ensure!(
            bloom_offset >= 4 && bloom_offset < file_size - 4,
            "invalid Bloom section offset"
        );

        let meta_offset_bytes = file.read((bloom_offset - 4) as u64, 4)?;
        let block_meta_offset = u32::from_be_bytes(meta_offset_bytes.try_into().unwrap()) as usize;
        ensure!(
            block_meta_offset < bloom_offset - 4,
            "invalid metadata section ordering"
        );

        let meta_len = bloom_offset - 4 - block_meta_offset;
        let meta_bytes = file.read(block_meta_offset as u64, meta_len as u64)?;
        let block_meta = BlockMeta::decode_block_meta_checked(meta_bytes.as_slice())?;
        ensure!(!block_meta.is_empty(), "SST contains no block metadata");
        ensure!(
            block_meta[0].offset == 0
                && block_meta
                    .windows(2)
                    .all(|pair| pair[0].offset < pair[1].offset)
                && block_meta
                    .iter()
                    .all(|meta| meta.offset < block_meta_offset),
            "invalid data block offsets"
        );

        let bloom_bytes = file.read(bloom_offset as u64, (file_size - 4 - bloom_offset) as u64)?;
        ensure!(!bloom_bytes.is_empty(), "Bloom section is empty");
        let bloom = Bloom::decode(&bloom_bytes)?;
        let first_key = block_meta.first().unwrap().first_key.clone();
        let last_key = block_meta.last().unwrap().last_key.clone();

        Ok(Self {
            file,
            block_meta,
            block_meta_offset,
            id,
            block_cache,
            first_key,
            last_key,
            bloom: Some(bloom),
            max_ts: 0,
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
        ensure!(
            block_idx < self.block_meta.len(),
            "block index out of bounds"
        );
        let offset = self.block_meta[block_idx].offset;
        let end = if block_idx + 1 < self.block_meta.len() {
            self.block_meta[block_idx + 1].offset
        } else {
            self.block_meta_offset
        };
        ensure!(offset < end, "invalid block byte range");
        let encoded = self.file.read(offset as u64, (end - offset) as u64)?;
        ensure!(encoded.len() >= 4, "data block checksum is missing");
        let content_end = encoded.len() - 4;
        let stored_checksum = u32::from_be_bytes(encoded[content_end..].try_into().unwrap());
        ensure!(
            crc32fast::hash(&encoded[..content_end]) == stored_checksum,
            "data block checksum mismatch"
        );
        Ok(Arc::new(Block::decode(&encoded[..content_end])))
    }

    /// Read a block from disk, with block cache. (Day 4)
    pub fn read_block_cached(&self, block_idx: usize) -> Result<Arc<Block>> {
        let Some(cache) = &self.block_cache else {
            return self.read_block(block_idx);
        };
        cache
            .try_get_with((self.id, block_idx), || self.read_block(block_idx))
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }

    /// Find the block that may contain `key`.
    /// Note: You may want to make use of the `first_key` stored in `BlockMeta`.
    /// You may also assume the key-value pairs stored in each consecutive block are sorted.
    pub fn find_block_idx(&self, key: KeySlice) -> usize {
        self.block_meta
            .partition_point(|meta| meta.last_key.as_key_slice() < key)
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
