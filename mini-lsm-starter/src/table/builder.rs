use std::path::Path;
use std::sync::Arc;

use super::{BlockMeta, FileObject, SsTable};
use crate::block::Block;
use crate::key::KeyBytes;
use crate::{block::BlockBuilder, key::KeySlice, lsm_storage::BlockCache};
use anyhow::Result;

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
            first_key: vec![],
            last_key: vec![],
            data: vec![],
            meta: vec![],
            block_size,
        }
    }

    /// flush block in self.builder
    fn flush_block(&mut self) {
        let block =
            std::mem::replace(&mut self.builder, BlockBuilder::new(self.block_size)).build();

        let first_key_len = Block::read_u16(block.data.as_slice(), 0) as usize;
        self.meta.push(BlockMeta {
            offset: self.data.len(),
            first_key: KeyBytes::from_bytes(block.data[2..first_key_len + 2].to_vec().into()),
            last_key: KeyBytes::from_bytes(self.last_key.clone().into()),
        });

        self.data
            .extend_from_slice(block.encode().to_vec().as_slice());
    }

    /// Adds a key-value pair to SSTable.
    ///
    /// Note: You should split a new block when the current block is full.(`std::mem::replace` may
    /// be helpful here)
    pub fn add(&mut self, key: KeySlice, value: &[u8]) {
        if self.first_key.len() == 0 {
            self.first_key.extend_from_slice(key.raw_ref());
        }
        // when block is full, flush block into self.data
        if !self.builder.add(key, value) {
            self.flush_block();
            self.add(key, value);
        } else {
            let _ = std::mem::replace(&mut self.last_key, Vec::from(key.raw_ref()));
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
        let mut sst_builder = self;
        // save last block in cache if exists
        if !sst_builder.builder.is_empty() {
            sst_builder.flush_block();
        }
        let mut buf = vec![];
        buf.extend_from_slice(sst_builder.data.as_slice());
        let block_meta_offset = buf.len();
        BlockMeta::encode_block_meta(sst_builder.meta.as_slice(), &mut buf);

        Ok(SsTable {
            id,
            block_cache,
            file: FileObject::create(path.as_ref(), buf)?,
            block_meta: sst_builder.meta,
            block_meta_offset,
            first_key: KeyBytes::from_bytes(sst_builder.first_key.into()),
            last_key: KeyBytes::from_bytes(sst_builder.last_key.into()),
            bloom: None,
            max_ts: 0,
        })
    }

    #[cfg(test)]
    pub(crate) fn build_for_test(self, path: impl AsRef<Path>) -> Result<SsTable> {
        self.build(0, None, path)
    }
}
