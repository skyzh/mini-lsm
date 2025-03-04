// Copyright (c) 2022-2025 Alex Chi Z
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

use bytes::Buf;
use std::fs::File;
use std::mem;
use std::path::Path;
use std::sync::Arc;

pub(crate) const USIZE_SIZE: usize = mem::size_of::<usize>();

use anyhow::Result;
pub use builder::SsTableBuilder;
use bytes::{BufMut, Bytes};
pub use iterator::SsTableIterator;

use crate::block::Block;
use crate::key::{KeyBytes, KeySlice, KeyVec};
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
        // Add the size so that it can be decoded later
        let mut estimated_size = mem::size_of::<u16>();

        for meta_data in block_meta.iter() {
            // Offset size estimate
            estimated_size += mem::size_of::<u32>();
            estimated_size += mem::size_of::<u16>();
            estimated_size += meta_data.first_key.raw_len();
            estimated_size += mem::size_of::<u16>();
            estimated_size += meta_data.last_key.raw_len();
        }
        estimated_size += 4; // for the checksum

        let original_len = buf.len();
        buf.reserve(estimated_size);

        buf.put_u16(block_meta.len() as u16);

        for meta_data in block_meta.iter() {
            buf.put_u32(meta_data.offset as u32);
            buf.put_u16(meta_data.first_key.raw_len() as u16);
            buf.extend(meta_data.first_key.key_ref());
            buf.put_u64(meta_data.first_key.ts());
            buf.put_u16(meta_data.last_key.raw_len() as u16);
            buf.extend(meta_data.last_key.key_ref());
            buf.put_u64(meta_data.last_key.ts());
        }

        buf.put_u32(crc32fast::hash(
            &buf[original_len..original_len + estimated_size - 4],
        ));
        assert_eq!(estimated_size, buf.len() - original_len);
    }

    /// Decode block meta from a buffer.
    pub fn decode_block_meta(mut buf: impl Buf) -> Vec<BlockMeta> {
        let bytes = Bytes::copy_from_slice(buf.chunk());
        // Validate the checkSum
        let checksum = (&bytes[bytes.len() - 4..]).get_u32();
        if crc32fast::hash(&bytes[..bytes.len() - 4]) != checksum {
            panic!("The Block Meta checksum failed");
        }

        let data_points = buf.get_u16() as usize; // First read the number of BlockMeta objects
        let mut ans = Vec::with_capacity(data_points);
        for _ in 0..data_points {
            let offset = buf.get_u32() as usize;
            let first_key_len = buf.get_u16() as usize;
            let first_key_key = buf.copy_to_bytes(first_key_len - 8);
            let first_key_ts = buf.get_u64();

            let last_key_len = buf.get_u16() as usize;
            let last_key_key = buf.copy_to_bytes(last_key_len - 8);
            let last_key_ts = buf.get_u64();

            let mut first_key = KeyVec::new();
            let mut last_key = KeyVec::new();

            first_key.append(&first_key_key);
            first_key.set_ts(first_key_ts);
            last_key.append(&last_key_key);
            last_key.set_ts(last_key_ts);

            ans.push(BlockMeta {
                offset,
                first_key: first_key.into_key_bytes(),
                last_key: last_key.into_key_bytes(),
            });
        }
        ans
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
        let mut total_len = file.1;
        let max_ts_encoded = file.read(total_len - 8, 8)?;
        let max_ts = (&max_ts_encoded[..]).get_u64();
        total_len -= 8;
        let bloom_offset = (&file.read(total_len - 4, 4).unwrap()[..]).get_u32() as u64;
        let bloom_encoded = file.read(bloom_offset, (total_len - bloom_offset) - 4)?;
        let bloom = Some(Bloom::decode(&bloom_encoded[..])?);
        total_len = bloom_offset;
        let block_meta_offset = (&file.read(total_len - 4, 4).unwrap()[..]).get_u32() as u64;
        let block_meta_encoded =
            file.read(block_meta_offset, (total_len - block_meta_offset) - 4)?;
        let block_meta = BlockMeta::decode_block_meta(&block_meta_encoded[..]);
        Ok(Self {
            file,
            block_meta_offset: block_meta_offset as usize,
            id,
            block_cache,
            first_key: block_meta.first().unwrap().first_key.clone(),
            last_key: block_meta.last().unwrap().last_key.clone(),
            block_meta,
            bloom,
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
        let block_offset = self.block_meta[block_idx].offset;

        let block_size = if block_idx == self.block_meta.len() - 1 {
            self.block_meta_offset - block_offset
        } else {
            self.block_meta[block_idx + 1].offset - self.block_meta[block_idx].offset
        };

        let block_data_with_checksum = self
            .file
            .read(block_offset as u64, block_size as u64)
            .unwrap();

        let block_data = &block_data_with_checksum[..block_data_with_checksum.len() - 4];
        let block_checksum = u32::from_be_bytes(
            block_data_with_checksum[block_data_with_checksum.len() - 4..]
                .try_into()
                .unwrap(),
        );

        // Verify the checksum
        if block_checksum != crc32fast::hash(block_data) {
            panic!("The block checksum does not match")
        }
        let block = Arc::new(Block::decode(block_data));
        Ok(block)
    }

    /// Read a block from disk, with block cache. (Day 4)
    pub fn read_block_cached(&self, block_idx: usize) -> Result<Arc<Block>> {
        if let Some(cache) = &self.block_cache {
            // Use the `?` operator for error handling and ensure the error type is `anyhow::Error`
            let block = cache
                .try_get_with((self.sst_id(), block_idx), || self.read_block(block_idx))
                .map_err(|e| anyhow::Error::msg(e.to_string()))?; // Convert the error to `anyhow::Error`
            Ok(block)
        } else {
            self.read_block(block_idx)
        }
    }

    /// Find the block that may contain `key`.
    /// Note: You may want to make use of the `first_key` stored in `BlockMeta`.
    /// You may also assume the key-value pairs stored in each consecutive block are sorted.
    pub fn find_block_idx(&self, key: KeySlice) -> usize {
        let mut lo = 0_i32;
        let mut high = (self.block_meta.len() - 1) as i32;
        while lo <= high {
            let mid = (lo + high) / 2;
            if key < self.block_meta[mid as usize].first_key.as_key_slice() {
                high = mid - 1;
            } else {
                lo = mid + 1;
            }
        }

        // Validate the block at `lo - 1` if it exists
        if lo > 0 && key <= self.block_meta[(lo - 1) as usize].last_key.as_key_slice() {
            (lo - 1) as usize
        } else {
            lo as usize
        }
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
