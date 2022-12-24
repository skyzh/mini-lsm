#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use std::sync::Arc;

use anyhow::Result;

use super::SsTable;

/// An iterator over the contents of an SSTable.
pub struct SsTableIterator {}

impl SsTableIterator {
    /// Create a new iterator and seek to the first key-value pair.
    pub fn create_and_seek_to_first(table: Arc<SsTable>) -> Result<Self> {
        unimplemented!()
    }

    /// Seek to the first key-value pair.
    pub fn seek_to_first(&mut self) -> Result<()> {
        unimplemented!()
    }

    /// Create a new iterator and seek to the first key-value pair which >= `key`.
    pub fn create_and_seek_to_key(table: Arc<SsTable>, key: &[u8]) -> Result<Self> {
        unimplemented!()
    }

    /// Seek to the first key-value pair which >= `key`.
    pub fn seek_to_key(&mut self, key: &[u8]) -> Result<()> {
        unimplemented!()
    }

    /// Get the current key.
    pub fn key(&self) -> &[u8] {
        unimplemented!()
    }

    /// Get the current value.
    pub fn value(&self) -> &[u8] {
        unimplemented!()
    }

    /// Check if the iterator is valid.
    pub fn is_valid(&self) -> bool {
        unimplemented!()
    }

    /// Move to the next key-value pair.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<()> {
        unimplemented!()
    }
}
