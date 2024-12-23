#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use std::path::Path;
use std::sync::Arc;

use super::{BlockMeta, FileObject, SsTable};
use crate::key::{KeyBytes, KeyVec};
use crate::{block::BlockBuilder, key::KeySlice, lsm_storage::BlockCache};
use anyhow::Result;
use bytes::BufMut;

/// Builds an SSTable from key-value pairs.
pub struct SsTableBuilder {
    builder: BlockBuilder,
    first_key: Vec<u8>,
    last_key: Vec<u8>,
    data: Vec<u8>,
    pub(crate) meta: Vec<BlockMeta>,
    block_size: usize,
}

impl SsTableBuilder {
    /// Create a builder based on target block size.
    pub fn new(block_size: usize) -> Self {
        Self {
            builder: BlockBuilder::new(block_size),
            first_key: Vec::new(),
            last_key: Vec::new(),
            data: Vec::new(),
            meta: Vec::new(),
            block_size,
        }
    }

    /// Adds a key-value pair to SSTable.
    ///
    /// Note: You should split a new block when the current block is full.(`std::mem::replace` may
    /// be helpful here)
    pub fn add(&mut self, key: KeySlice, value: &[u8]) {
        if self.first_key.is_empty() {
            self.first_key = key.to_key_vec().into_inner();
        }
        if !self.builder.add(key.clone(), value) {
            self.push_block_to_buf();
            self.add(key, value);
            return;
        }
        self.last_key = key.to_key_vec().into_inner();
    }

    fn push_block_to_buf(&mut self) {
        let new_builder = BlockBuilder::new(self.block_size);
        let old_builder = std::mem::replace(&mut self.builder, new_builder);
        if !old_builder.is_empty() {
            self.meta.push(BlockMeta {
                offset: self.data.len(),
                first_key: old_builder.first_key().into_key_bytes(),
                last_key: KeyVec::from_vec(self.last_key.clone()).into_key_bytes(),
            });
            let block = old_builder.build();
            self.data.put(block.encode());
        }
    }

    /// Get the estimated size of the SSTable.
    ///
    /// Since the data blocks contain much more data than meta blocks, just return the size of data
    /// blocks here.
    pub fn estimated_size(&self) -> usize {
        self.data.len()
    }

    /// Builds the SSTable and writes it to the given path. Use the `FileObject` structure to manipulate the disk objects.
    pub fn build(
        self,
        id: usize,
        block_cache: Option<Arc<BlockCache>>,
        path: impl AsRef<Path>,
    ) -> Result<SsTable> {
        let mut s = self;
        s.push_block_to_buf();
        let mut data = s.data;
        let mut meta = s.meta;
        let meta_offset = data.len();
        BlockMeta::encode_block_meta(&meta[..], &mut data);
        data.put_u32(meta_offset as u32);

        SsTable::open(id, block_cache, FileObject::create(path.as_ref(), data)?)
    }

    #[cfg(test)]
    pub(crate) fn build_for_test(self, path: impl AsRef<Path>) -> Result<SsTable> {
        self.build(0, None, path)
    }
}
