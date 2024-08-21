#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use std::path::Path;
use std::sync::Arc;

use anyhow::{Ok, Result};
use bytes::BufMut;

use super::{BlockMeta, FileObject, SsTable};
use crate::{
    block::BlockBuilder,
    key::{self, KeyBytes, KeySlice},
    lsm_storage::BlockCache,
};

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
        let block_is_full = !self.builder.add(key, value);

        if block_is_full {
            // our block was full! We need to create a new block.
            self.freeze_block_and_create_new();
            // if the block was full, we never actually stored the data
            // in the above add call. So we add it now
            if !self.builder.add(key, value) {
                panic!("This is not an expected state. We froze and created a new block above. So this call
                        should succeed!")
            }
        }

        // was this the first key? If the block was full before, this will
        // always be true since it's a new block.
        if self.first_key.is_empty() {
            self.first_key.clear();
            self.first_key.extend(key.into_inner());
        }

        // the last key always needs an update
        self.last_key.clear();
        self.last_key.extend(key.into_inner());
    }

    fn freeze_block_and_create_new(&mut self) {
        // The SST contract asks us to persist the first key, last key, and the offset
        // of the block that we are freezing. We need to persist this block info to the
        // SST's Metadata (meta in the struct). mem::take is efficient here as we do not
        // want to unnecessary clone the data and this block is going to be frozen anyway.
        let old_first_key_to_persist = std::mem::take(&mut self.first_key);
        let old_last_key_to_persist = std::mem::take(&mut self.last_key);

        self.meta.push(BlockMeta {
            first_key: KeyBytes::from_bytes(old_first_key_to_persist.into()),
            last_key: KeyBytes::from_bytes(old_last_key_to_persist.into()),
            // this data is the SsT data blocks. If this is the first block being froezen,
            // then the offset will be 0.
            offset: self.data.len(),
        });

        // replace API provides nice atomic movement of data semantics and returns the
        // old value  at the moved location. We still need our current builder to finally
        // build it and store the data in our SST.
        let old_builder = std::mem::replace(&mut self.builder, BlockBuilder::new(self.block_size));
        // now extend our data with the block by building it and encoding it.
        self.data.extend(old_builder.build().encode());
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
        mut self,
        id: usize,
        block_cache: Option<Arc<BlockCache>>,
        path: impl AsRef<Path>,
    ) -> Result<SsTable> {
        self.freeze_block_and_create_new();

        let mut buf = self.data;
        let block_meta_offset = buf.len();
        BlockMeta::encode_block_meta(&self.meta, &mut buf);
        buf.put_u32(block_meta_offset as u32);
        let file = FileObject::create(path.as_ref(), buf)?;

        Ok(SsTable {
            file,
            block_meta_offset,
            id,
            block_cache,
            first_key: self.meta.first().unwrap().first_key.clone(),
            last_key: self.meta.last().unwrap().last_key.clone(),
            block_meta: self.meta,
            bloom: None,
            max_ts: 0,
        })
    }

    #[cfg(test)]
    pub(crate) fn build_for_test(self, path: impl AsRef<Path>) -> Result<SsTable> {
        self.build(0, None, path)
    }
}
