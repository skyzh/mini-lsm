// Copyright (c) 2022-2026 Alex Chi Z
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use std::sync::Arc;

use anyhow::Result;

use super::SsTable;
use crate::{block::BlockIterator, iterators::StorageIterator, key::KeySlice};

/// An iterator over the contents of an SSTable.
pub struct SsTableIterator {
    table: Arc<SsTable>,
    blk_iter: Option<BlockIterator>,
    blk_idx: usize,
}

impl SsTableIterator {
    /// Create a new iterator and seek to the first key-value pair in the first data block.
    pub fn create_and_seek_to_first(table: Arc<SsTable>) -> Result<Self> {
        let mut iter = Self {
            table,
            blk_iter: None,
            blk_idx: 0,
        };
        iter.seek_to_first()?;
        Ok(iter)
    }

    /// Seek to the first key-value pair in the first data block.
    pub fn seek_to_first(&mut self) -> Result<()> {
        self.blk_idx = 0;
        if self.table.num_of_blocks() == 0 {
            self.blk_iter = None;
            return Ok(());
        }
        let block = self.table.read_block_cached(0)?;
        self.blk_iter = Some(BlockIterator::create_and_seek_to_first(block));
        Ok(())
    }

    /// Create a new iterator and seek to the first key-value pair which >= `key`.
    pub fn create_and_seek_to_key(table: Arc<SsTable>, key: KeySlice) -> Result<Self> {
        let mut iter = Self {
            table,
            blk_iter: None,
            blk_idx: 0,
        };
        iter.seek_to_key(key)?;
        Ok(iter)
    }

    /// Seek to the first key-value pair which >= `key`.
    /// Note: You probably want to review the handout for detailed explanation when implementing
    /// this function.
    pub fn seek_to_key(&mut self, key: KeySlice) -> Result<()> {
        self.blk_idx = self.table.find_block_idx(key);
        if self.blk_idx >= self.table.num_of_blocks() {
            self.blk_iter = None;
            return Ok(());
        }
        let block = self.table.read_block_cached(self.blk_idx)?;
        let block_iter = BlockIterator::create_and_seek_to_key(block, key);
        self.blk_iter = Some(block_iter);
        if !self.is_valid() {
            self.move_to_next_block()?;
        }
        Ok(())
    }
}

impl StorageIterator for SsTableIterator {
    type KeyType<'a> = KeySlice<'a>;

    /// Return the `key` that's held by the underlying block iterator.
    fn key(&self) -> KeySlice<'_> {
        self.blk_iter.as_ref().unwrap().key()
    }

    /// Return the `value` that's held by the underlying block iterator.
    fn value(&self) -> &[u8] {
        self.blk_iter.as_ref().unwrap().value()
    }

    /// Return whether the current block iterator is valid or not.
    fn is_valid(&self) -> bool {
        self.blk_iter.as_ref().is_some_and(BlockIterator::is_valid)
    }

    /// Move to the next `key` in the block.
    /// Note: You may want to check if the current block iterator is valid after the move.
    fn next(&mut self) -> Result<()> {
        let Some(block_iter) = &mut self.blk_iter else {
            return Ok(());
        };
        block_iter.next();
        if !block_iter.is_valid() {
            self.move_to_next_block()?;
        }
        Ok(())
    }
}

impl SsTableIterator {
    fn move_to_next_block(&mut self) -> Result<()> {
        self.blk_idx += 1;
        if self.blk_idx >= self.table.num_of_blocks() {
            self.blk_iter = None;
            return Ok(());
        }
        let block = self.table.read_block_cached(self.blk_idx)?;
        self.blk_iter = Some(BlockIterator::create_and_seek_to_first(block));
        Ok(())
    }
}
