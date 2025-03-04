#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use std::sync::Arc;

use anyhow::Result;

use super::SsTable;
use crate::{block::BlockIterator, iterators::StorageIterator, key::KeySlice};

/// An iterator over the contents of an SSTable.
pub struct SsTableIterator {
    table: Arc<SsTable>,
    blk_iter: BlockIterator,
    blk_idx: usize,
}

impl SsTableIterator {
    /// Create a new iterator and seek to the first key-value pair in the first data block.
    pub fn create_and_seek_to_first(table: Arc<SsTable>) -> Result<Self> {
        Ok(Self {
            table: table.clone(),
            blk_iter: SsTableIterator::seek_to_idx(table.clone(), 0),
            blk_idx: 0,
        })
    }

    /// Seek to the first key-value pair in the first data block.
    pub fn seek_to_first(&mut self) -> Result<()> {
        self.blk_iter = SsTableIterator::seek_to_idx(self.table.clone(), 0);
        self.blk_idx = 0;
        Ok(())
    }

    /// Create a new iterator and seek to the first key-value pair which >= `key`.
    pub fn create_and_seek_to_key(table: Arc<SsTable>, key: KeySlice) -> Result<Self> {
        let mut temp_self = Self {
            table: table.clone(),
            blk_iter: SsTableIterator::seek_to_idx(table.clone(), 0),
            blk_idx: 0,
        };
        temp_self.seek_to_key(key)?;
        Ok(temp_self)
    }

    /// Seek to the first key-value pair which >= `key`.
    /// Note: You probably want to review the handout for detailed explanation when implementing
    /// this function.
    pub fn seek_to_key(&mut self, key: KeySlice) -> Result<()> {
        let idx = self.table.find_block_idx(key);
        self.blk_idx = idx;
        if idx < self.table.block_meta.len() {
            self.blk_iter = SsTableIterator::seek_to_idx(self.table.clone(), idx);
            self.blk_iter.seek_to_key(key);
        }

        Ok(())
    }

    fn seek_to_idx(table: Arc<SsTable>, idx: usize) -> BlockIterator {
        BlockIterator::create_and_seek_to_first(table.clone().read_block_cached(idx).unwrap())
    }
}

impl StorageIterator for SsTableIterator {
    type KeyType<'a> = KeySlice<'a>;

    /// Return the `key` that's held by the underlying block iterator.
    fn key(&self) -> KeySlice {
        return self.blk_iter.key();
    }

    /// Return the `value` that's held by the underlying block iterator.
    fn value(&self) -> &[u8] {
        return self.blk_iter.value();
    }

    /// Return whether the current block iterator is valid or not.
    fn is_valid(&self) -> bool {
        if ((self.table.block_meta.len() - 1) == self.blk_idx) && !self.blk_iter.is_valid() {
            return false;
        }
        true
    }

    /// Move to the next `key` in the block.
    /// Note: You may want to check if the current block iterator is valid after the move.
    fn next(&mut self) -> Result<()> {
        self.blk_iter.next();

        if self.is_valid() && !self.blk_iter.is_valid() {
            self.blk_iter = SsTableIterator::seek_to_idx(self.table.clone(), self.blk_idx + 1);
            self.blk_idx += 1;
        }

        Ok(())
    }
}
