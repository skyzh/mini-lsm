#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use anyhow::Result;
use std::path::Path;

use super::{BlockMeta, SsTable};

/// Builds an SSTable from key-value pairs.
pub struct SsTableBuilder {
    pub(super) meta: Vec<BlockMeta>,
}

impl SsTableBuilder {
    /// Create a builder based on target SST size and target block size.
    pub fn new(target_size: usize, block_size: usize) -> Self {
        unimplemented!()
    }

    /// Adds a key-value pair to SSTable, return false when SST full.
    #[must_use]
    pub fn add(&mut self, key: &[u8], value: &[u8]) -> bool {
        unimplemented!()
    }

    /// Builds the SSTable and writes it to the given path. No need to actually write to disk until chapter 4 block cache.
    pub fn build(self, path: impl AsRef<Path>) -> Result<SsTable> {
        unimplemented!()
    }
}
