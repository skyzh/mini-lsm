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
        let block = table.read_block(0)?;
        let mut iterator = BlockIterator::new(block);
        iterator.seek_to_first();
        Ok(Self {
            table,
            blk_iter: iterator,
            blk_idx: 0,
        })
    }

    /// Seek to the first key-value pair in the first data block.
    pub fn seek_to_first(&mut self) -> Result<()> {
        self.blk_idx = 0;
        self.blk_iter = self.get_block_iter(self.blk_idx)?;
        self.blk_iter.seek_to_first();
        Ok(())
    }

    /// Create a new iterator and seek to the first key-value pair which >= `key`.
    pub fn create_and_seek_to_key(table: Arc<SsTable>, key: KeySlice) -> Result<Self> {
        let block_idx = table.find_block_idx(key);
        let block = table.read_block(block_idx)?;
        let mut iterator = BlockIterator::new(block);
        iterator.seek_to_key(key);
        Ok(Self {
            table,
            blk_iter: iterator,
            blk_idx: 0,
        })
    }

    /// Seek to the first key-value pair which >= `key`.
    /// Note: You probably want to review the handout for detailed explanation when implementing
    /// this function.
    pub fn seek_to_key(&mut self, key: KeySlice) -> Result<()> {
        let block_idx = self.table.find_block_idx(key);
        let block = self.table.read_block(block_idx)?;
        let mut iterator = BlockIterator::new(block);
        iterator.seek_to_key(key);
        self.blk_iter = iterator;
        Ok(())
    }

    fn get_block_iter(&self, block_idx: usize) -> Result<BlockIterator> {
        let block = self.table.read_block(block_idx)?;
        Ok(BlockIterator::new(block))
    }
}

impl StorageIterator for SsTableIterator {
    type KeyType<'a> = KeySlice<'a>;

    /// Return the `key` that's held by the underlying block iterator.
    fn key(&self) -> KeySlice {
        self.blk_iter.key()
    }

    /// Return the `value` that's held by the underlying block iterator.
    fn value(&self) -> &[u8] {
        self.blk_iter.value()
    }

    /// Return whether the current block iterator is valid or not.
    fn is_valid(&self) -> bool {
        self.blk_iter.is_valid()
    }

    /// Move to the next `key` in the block.
    /// Note: You may want to check if the current block iterator is valid after the move.
    fn next(&mut self) -> Result<()> {
        self.blk_iter.next();
        if !self.is_valid() && self.blk_idx < self.table.block_meta.len() - 1 {
            self.blk_idx += 1;
            let mut iterator = self.get_block_iter(self.blk_idx)?;
            iterator.seek_to_first();
            self.blk_iter = iterator;
        }
        Ok(())
    }
}
