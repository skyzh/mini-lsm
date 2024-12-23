#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

pub(crate) mod bloom;
mod builder;
mod iterator;

use std::fs::File;
use std::io::Read;
use std::ops::AddAssign;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
pub use builder::SsTableBuilder;
use bytes::{Buf, BufMut, Bytes};
pub use iterator::SsTableIterator;

use crate::block::{Block, SIZEOF_U16};
use crate::key::{KeyBytes, KeySlice};
use crate::lsm_storage::BlockCache;

use self::bloom::Bloom;

pub(crate) const SIZEOF_U32: usize = std::mem::size_of::<u32>();

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
    pub fn encode_block_meta(block_metas: &[BlockMeta], buf: &mut Vec<u8>) {
        buf.put_u16(block_metas.len() as u16);
        for block_meta in block_metas {
            buf.put_u32(block_meta.offset as u32);
            buf.put_u32(block_meta.first_key.len() as u32);
            buf.put(block_meta.first_key.raw_ref());
            buf.put_u32(block_meta.last_key.len() as u32);
            buf.put(block_meta.last_key.raw_ref());
        }
    }

    /// Decode block meta from a buffer.
    pub fn decode_block_meta(buf: impl Buf) -> Vec<BlockMeta> {
        let mut bytes = buf.chunk();
        let blocks = bytes.get_u16() as usize;
        let mut result = Vec::with_capacity(blocks);
        for i in 0..blocks {
            let offset = bytes.get_u32() as usize;
            let first_key_len = bytes.get_u32() as usize;
            let first_key_offset = bytes.len() - bytes.remaining();
            let first_key = KeyBytes::from_bytes(Bytes::copy_from_slice(
                &bytes[first_key_offset..first_key_offset + first_key_len],
            ));
            bytes.advance(first_key_len);
            let last_key_len = bytes.get_u32() as usize;
            let last_key_offset = bytes.len() - bytes.remaining();
            let last_key = KeyBytes::from_bytes(Bytes::copy_from_slice(
                &bytes[last_key_offset..last_key_offset + last_key_len],
            ));
            bytes.advance(last_key_len);
            result.push(BlockMeta {
                offset,
                first_key,
                last_key,
            });
        }
        result
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
        let block_meta_offset =
            (&file.read(file.1 - SIZEOF_U32 as u64, SIZEOF_U32 as u64)?[..]).get_u32() as usize;
        let block_meta_len = file.1 - block_meta_offset as u64 - SIZEOF_U32 as u64;
        let block_meta =
            BlockMeta::decode_block_meta(&file.read(block_meta_offset as u64, block_meta_len)?[..]);
        let first_key = block_meta.first().unwrap().first_key.clone();
        let last_key = block_meta.last().unwrap().last_key.clone();
        Ok(SsTable {
            file,
            block_meta,
            block_meta_offset,
            id,
            block_cache,
            first_key,
            last_key,
            bloom: None,
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
        unimplemented!()
    }

    /// Read a block from disk, with block cache. (Day 4)
    pub fn read_block_cached(&self, block_idx: usize) -> Result<Arc<Block>> {
        unimplemented!()
    }

    /// Find the block that may contain `key`.
    /// Note: You may want to make use of the `first_key` stored in `BlockMeta`.
    /// You may also assume the key-value pairs stored in each consecutive block are sorted.
    pub fn find_block_idx(&self, key: KeySlice) -> usize {
        unimplemented!()
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
