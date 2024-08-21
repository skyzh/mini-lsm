#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

pub(crate) mod bloom;
mod builder;
mod iterator;

use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Ok, Result};
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

const U32_SIZE: usize = std::mem::size_of::<u32>();
const U16_SIZE: usize = std::mem::size_of::<u16>();

impl BlockMeta {
    /// Encode block meta to a buffer.
    /// You may add extra fields to the buffer,
    /// in order to help keep track of `first_key` when decoding from the same buffer in the future.
    pub fn encode_block_meta(
        block_meta: &[BlockMeta],
        #[allow(clippy::ptr_arg)] // remove this allow after you finish
        buf: &mut Vec<u8>,
    ) {
        let mut block_metadata_size = 0;
        for meta in block_meta {
            block_metadata_size += one_block_meta_estimated_size(meta);
        }
        buf.reserve(block_metadata_size); // pre-allocate the buffer for better perf
        for meta in block_meta {
            // store the offset
            buf.put_u32(meta.offset as u32);
            // store the length of the first key
            buf.put_u16(meta.first_key.len() as u16);
            // store the first key
            buf.put_slice(meta.first_key.raw_ref());
            // store the length of the second key
            buf.put_u16(meta.first_key.len() as u16);
            // store the second key
            buf.put_slice(meta.last_key.raw_ref());
        }
    }

    /// Decode block meta from a buffer.
    pub fn decode_block_meta(mut buf: impl Buf) -> Vec<BlockMeta> {
        let mut block_meta = Vec::new();

        while buf.remaining() > 0 {
            let offset = buf.get_u32() as usize;
            let first_key_len = buf.get_u16() as usize;
            let first_key = buf.copy_to_bytes(first_key_len);
            let last_key_len = buf.get_u16() as usize;
            let last_key = buf.copy_to_bytes(last_key_len);
            block_meta.push(BlockMeta {
                offset,
                first_key: KeyBytes::from_bytes(first_key),
                last_key: KeyBytes::from_bytes(last_key),
            });
        }

        block_meta
    }
}

pub fn one_block_meta_estimated_size(one_block_meta: &BlockMeta) -> usize {
    let mut one_block_metadata_size = 0;
    // size for the stored offset of the block
    one_block_metadata_size += U32_SIZE;
    // 2 bytes for the length of the first key
    one_block_metadata_size += U16_SIZE;
    // actual size of the first key
    one_block_metadata_size += one_block_meta.first_key.len();
    // 2 bytes for the length of the second key
    one_block_metadata_size += U32_SIZE;
    // actual size of the second key
    one_block_metadata_size += one_block_meta.last_key.len();

    one_block_metadata_size
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
        // total lenght of the encoded SSTable file
        let length = file.size();

        // the last 4 bytes contain the offset of the block metadata, i.e., where does
        // the list of block metadata start
        let block_meta_offset_bytes = file.read(length - 4, 4)?;

        // get the actual block meta offset
        let block_meta_offset = (&block_meta_offset_bytes[..]).get_u32() as u64;

        // get the bytes form of the block metadata |b1,b2..bn|bm1,bm2...bmn|b_meta_offset
        let block_meta_bytes = file.read(block_meta_offset, length - 4 - block_meta_offset)?;

        let block_meta_decoded = BlockMeta::decode_block_meta(&block_meta_bytes[..]);
        Ok(Self {
            id,
            file,
            first_key: block_meta_decoded.first().unwrap().first_key.clone(),
            last_key: block_meta_decoded.last().unwrap().last_key.clone(),
            block_meta: block_meta_decoded,
            block_meta_offset: block_meta_offset as usize,
            bloom: None,
            block_cache,
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
        let offset = self.block_meta[block_idx].offset;
        let offset_end = self
            .block_meta
            .get(block_idx + 1)
            .map_or(self.block_meta_offset, |x| x.offset);
        let block_data = self
            .file
            .read(offset as u64, (offset_end - offset) as u64)?;
        Ok(Arc::new(Block::decode(&block_data[..])))
    }

    /// Read a block from disk, with block cache. (Day 4)
    pub fn read_block_cached(&self, block_idx: usize) -> Result<Arc<Block>> {
        if let Some(cache) = &self.block_cache {
            // get block based on block index for the current SSTable ID
            let block = cache.get(&(self.id, block_idx));
            if let Some(block) = block {
                Ok(block)
            } else {
                self.read_block(block_idx)
            }
        } else {
            self.read_block(block_idx)
        }
    }

    /// Find the block that may contain `key`.
    /// Note: You may want to make use of the `first_key` stored in `BlockMeta`.
    /// You may also assume the key-value pairs stored in each consecutive block are sorted.
    pub fn find_block_idx(&self, key: KeySlice) -> usize {
        self.block_meta
            .partition_point(|meta| meta.first_key.as_key_slice() <= key)
            .saturating_sub(1)

        // let mut start = 0;
        // let mut end = self.block_meta.len();

        // // binary search to find the index of the block where the key may reside
        // while start < end {
        //     let mid = start + (end - start) / 2;
        //     if self.block_meta[mid].first_key.as_key_slice() <= key {
        //         start = mid + 1;
        //     } else {
        //         end = mid;
        //     }
        // }

        // if start == 0 {
        //     return start;
        // }
        // start - 1
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
