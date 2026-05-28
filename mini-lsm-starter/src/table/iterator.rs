// Copyright (c) 2022-2025 Alex Chi Z
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

use std::cell::UnsafeCell;
use std::sync::Arc;

use anyhow::Result;

use super::SsTable;
use crate::block::BlockIterator;
use crate::iterators::StorageIterator;
use crate::key::{KeyBytes, KeySlice};
use crate::vlog::{KvKind, ValueLog, ValuePointer};

/// An iterator over the contents of an SSTable.
pub struct SsTableIterator {
    table: Arc<SsTable>,
    blk_iter: BlockIterator,
    blk_idx: usize,
    vlog: Option<Arc<ValueLog>>,
    /// Cache for dereferenced ValuePointer values. Uses UnsafeCell for interior
    /// mutability since `StorageIterator::value()` takes `&self`.
    deref_cache: UnsafeCell<Option<(KeyBytes, Vec<u8>)>>,
}

impl SsTableIterator {
    /// Create a new iterator and seek to the first key-value pair in the first data block.
    pub fn create_and_seek_to_first(table: Arc<SsTable>) -> Result<Self> {
        let b = table.read_block_cached(0)?;
        Ok(SsTableIterator {
            table,
            blk_iter: BlockIterator::create_and_seek_to_first(b),
            blk_idx: 0,
            vlog: None,
            deref_cache: UnsafeCell::new(None),
        })
    }

    /// Create a new iterator with vLog support and seek to the first key-value pair.
    pub fn create_and_seek_to_first_with_vlog(
        table: Arc<SsTable>,
        vlog: Arc<ValueLog>,
    ) -> Result<Self> {
        let mut it = Self::create_and_seek_to_first(table)?;
        it.vlog = Some(vlog);
        Ok(it)
    }

    /// Seek to the first key-value pair in the first data block.
    pub fn seek_to_first(&mut self) -> Result<()> {
        let b = self.table.read_block_cached(0)?;
        self.blk_idx = 0;
        self.blk_iter = BlockIterator::create_and_seek_to_first(b);
        *self.deref_cache.get_mut() = None;

        Ok(())
    }

    /// Create a new iterator and seek to the first key-value pair which >= `key`.
    pub fn create_and_seek_to_key(table: Arc<SsTable>, key: KeySlice) -> Result<Self> {
        let (blk_idx, blk_iter) = Self::seek_to_key_inner(&table, key)?;

        Ok(SsTableIterator {
            table,
            blk_iter,
            blk_idx,
            vlog: None,
            deref_cache: UnsafeCell::new(None),
        })
    }

    /// Create a new iterator with vLog support and seek to the first key >= `key`.
    pub fn create_and_seek_to_key_with_vlog(
        table: Arc<SsTable>,
        key: KeySlice,
        vlog: Arc<ValueLog>,
    ) -> Result<Self> {
        let mut it = Self::create_and_seek_to_key(table, key)?;
        it.vlog = Some(vlog);
        Ok(it)
    }

    /// Set the vLog for ValuePointer dereferencing.
    pub fn set_vlog(&mut self, vlog: Arc<ValueLog>) {
        self.vlog = Some(vlog);
    }

    fn seek_to_key_inner(table: &Arc<SsTable>, key: KeySlice) -> Result<(usize, BlockIterator)> {
        let mut blk_idx = table.find_block_idx(key);
        let mut blk_iter =
            BlockIterator::create_and_seek_to_key(table.read_block_cached(blk_idx)?, key);
        if !blk_iter.is_valid() {
            blk_idx += 1;
            if blk_idx < table.num_of_blocks() {
                blk_iter =
                    BlockIterator::create_and_seek_to_first(table.read_block_cached(blk_idx)?);
            }
        }

        Ok((blk_idx, blk_iter))
    }

    /// Seek to the first key-value pair which >= `key`.
    /// Note: You probably want to review the handout for detailed explanation when implementing
    /// this function.
    pub fn seek_to_key(&mut self, key: KeySlice) -> Result<()> {
        let (blk_idx, blk_iter) = Self::seek_to_key_inner(&self.table, key)?;
        self.blk_iter = blk_iter;
        self.blk_idx = blk_idx;
        *self.deref_cache.get_mut() = None;

        Ok(())
    }
}

impl StorageIterator for SsTableIterator {
    type KeyType<'a> = KeySlice<'a>;

    /// Return the `key` that's held by the underlying block iterator.
    fn key(&'_ self) -> KeySlice<'_> {
        self.blk_iter.key()
    }

    /// Return the resolved value: strips the KvKind prefix and dereferences ValuePointers.
    fn value(&self) -> &[u8] {
        let raw = self.blk_iter.value();
        if raw.is_empty() {
            return &[];
        }
        let kind = raw[0];
        let payload = &raw[1..];

        match KvKind::from_u8(kind) {
            Some(KvKind::Inline) => {
                // Inline value or tombstone (empty payload)
                payload
            }
            Some(KvKind::ValuePointer) => {
                // Check cache first (safe: only accessed from this iterator)
                let cache = unsafe { &*self.deref_cache.get() };
                if let Some((cached_key, cached_val)) = cache
                    && cached_key.as_key_slice().raw_ref() == self.blk_iter.key().raw_ref()
                {
                    return cached_val;
                }
                // Cache miss: dereference from vLog
                let vlog = self
                    .vlog
                    .as_ref()
                    .expect("SsTableIterator encountered ValuePointer but no vLog was provided");
                let ptr = ValuePointer::try_decode(payload)
                    .expect("SsTableIterator: invalid ValuePointer encoding in block");
                let bytes = vlog
                    .read(&ptr, self.blk_iter.key().raw_ref())
                    .expect("SsTableIterator: failed to read value from vLog");
                let val = bytes.to_vec();
                // Update cache (safe: single-threaded, only written here)
                let cache_mut = unsafe { &mut *self.deref_cache.get() };
                *cache_mut = Some((self.blk_iter.key().to_key_vec().into_key_bytes(), val));
                &cache_mut.as_ref().unwrap().1
            }
            None => {
                // Unknown kind byte — treat as inline value
                raw
            }
        }
    }

    /// Return the raw value bytes including the KvKind prefix.
    fn raw_value(&self) -> &[u8] {
        self.blk_iter.value()
    }

    /// Return whether the current block iterator is valid or not.
    fn is_valid(&self) -> bool {
        self.blk_iter.is_valid()
    }

    /// Move to the next `key` in the block.
    /// Note: You may want to check if the current block iterator is valid after the move.
    fn next(&mut self) -> Result<()> {
        // Clear deref cache
        *self.deref_cache.get_mut() = None;

        self.blk_iter.next();

        if !self.blk_iter.is_valid() {
            let idx = self.blk_idx + 1;
            if idx >= self.table.num_of_blocks() {
                return Ok(());
            }

            let b = self.table.read_block_cached(idx)?;
            self.blk_idx = idx;
            self.blk_iter = BlockIterator::create_and_seek_to_first(b);
        }

        Ok(())
    }
}
