#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

mod builder;
mod iterator;

use std::f32::consts::E;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
pub use builder::SsTableBuilder;
use bytes::{Buf, Bytes, BufMut};
pub use iterator::SsTableIterator;

use crate::block::Block;
use crate::lsm_storage::BlockCache;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockMeta {
    /// Offset of this data block.
    pub offset: usize,
    /// The first key of the data block, mainly used for index purpose.
    pub first_key: Bytes,
}

impl BlockMeta {
    /// Encode block meta to a buffer.
    /// You may add extra fields to the buffer,
    /// in order to help keep track of `first_key` when decoding from the same buffer in the future.
    pub fn encode_block_meta(
        block_meta: &[BlockMeta],
        buf: &mut Vec<u8>,
    ) {
        let mut  estimated_size = 0 ;
        for block_m in  block_meta {
            // the length of  size
            estimated_size += std::mem::size_of::<u32>();
            // the length of the key lenght
            estimated_size += std::mem::size_of::<u16>();
            // the size of the acutal key 
            estimated_size += block_m.first_key.len();

        }
         // Reserve the space to improve performance, especially when the size of incoming data is large
        buf.reserve(estimated_size);

        for meta in block_meta{
            buf.put_u32(meta.offset as u32);
            buf.put_u16(meta.first_key.len() as u16);
            //每次key都是和lenth 绑定在一起的
            buf.put_slice(&meta.first_key);

        }
       
    }

    /// Decode block meta from a buffer.
    pub fn decode_block_meta(mut buf: impl Buf) -> Vec<BlockMeta> {
        let mut block_meta = Vec::new();
        while buf.has_remaining() {
            let offset = buf.get_u32() as usize;
            let first_key_len = buf.get_u16() as usize;
            let first_key = buf.copy_to_bytes(first_key_len);
            block_meta.push(BlockMeta { offset, first_key });
        }
        block_meta
    }
}

/// A file object.
pub struct FileObject(Bytes);

impl FileObject {
    pub fn read(&self, offset: u64, len: u64) -> Result<Vec<u8>> {
        Ok(self.0[offset as usize..(offset + len) as usize].to_vec())
    }

    pub fn size(&self) -> u64 {
        self.0.len() as u64
    }

    /// Create a new file object (day 2) and write the file to the disk (day 4).
    pub fn create(path: &Path, data: Vec<u8>) -> Result<Self> {
        Ok(FileObject(data.into()))
    }

    pub fn open(path: &Path) -> Result<Self> {
        unimplemented!()
    }
}

/// -------------------------------------------------------------------------------------------------------
/// |              Data Block             |             Meta Block              |          Extra          |
/// -------------------------------------------------------------------------------------------------------
/// | Data Block #1 | ... | Data Block #N | Meta Block #1 | ... | Meta Block #N | Meta Block Offset (u32) |
/// -------------------------------------------------------------------------------------------------------
pub struct SsTable {
    /// The actual storage unit of SsTable, the format is as above.
    file: FileObject,
    /// The meta blocks that hold info for data blocks.
    block_metas: Vec<BlockMeta>,
    /// The offset that indicates the start point of meta blocks in `file`.
    block_meta_offset: usize,
}

impl SsTable {
    #[cfg(test)]
    pub(crate) fn open_for_test(file: FileObject) -> Result<Self> {
        Self::open(0, None, file)
    }

    /// Open SSTable from a file.
    pub fn open(id: usize, block_cache: Option<Arc<BlockCache>>, file: FileObject) -> Result<Self> {
        unimplemented!()
    }

    /// Read a block from the disk.
    pub fn read_block(&self, block_idx: usize) -> Result<Arc<Block>> {
        unimplemented!()
    }

    /// Read a block from disk, with block cache. (Day 4)
    pub fn read_block_cached(&self, block_idx: usize) -> Result<Arc<Block>> {
        unimplemented!()
    }

    /// Find the block that may contain `key`.
    /// Note: You may want to make use of the `first_key` stored in `BlockMeta`.
    /// You may also assume the key-value pairs stored in each consecutive block are sorted.
    pub fn find_block_idx(&self, key: &[u8]) -> usize {
        unimplemented!()
    }

    /// Get number of data blocks.
    pub fn num_of_blocks(&self) -> usize {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests;
