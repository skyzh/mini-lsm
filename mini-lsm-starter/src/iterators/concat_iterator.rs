#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use std::sync::Arc;

use anyhow::Result;

use super::StorageIterator;
use crate::{
    key::KeySlice,
    table::{SsTable, SsTableIterator},
};

/// Concat multiple iterators ordered in key order and their key ranges do not overlap. We do not want to create the
/// iterators when initializing this iterator to reduce the overhead of seeking.
pub struct SstConcatIterator {
    current: Option<SsTableIterator>,
    next_sst_idx: usize,
    sstables: Vec<Arc<SsTable>>,
}

impl SstConcatIterator {
    fn find_sst_iterator(&self, key_pass: KeySlice) -> Result<Option<SsTableIterator>> {
        let mut idx = self
            .sstables
            .partition_point(|sst| sst.first_key().as_key_slice() <= key_pass)
            .saturating_sub(1);

        // Jump to the next table in case the current table does not contain the key
        if self.sstables[idx].last_key().as_key_slice() < key_pass {
            idx += 1;
        }

        // Return the iterator only if the index is valid
        if idx < self.sstables.len() {
            return Ok(Some(self.seek_sst_idx_key(idx, key_pass)?));
        }
        Ok(None)
    }

    fn seek_sst_idx_first(&self, idx: usize) -> Result<SsTableIterator> {
        SsTableIterator::create_and_seek_to_first(self.sstables[idx].clone())
    }

    fn seek_sst_idx_key(&self, idx: usize, key: KeySlice) -> Result<SsTableIterator> {
        SsTableIterator::create_and_seek_to_key(self.sstables[idx].clone(), key)
    }

    pub fn create_and_seek_to_first(sstables: Vec<Arc<SsTable>>) -> Result<Self> {
        if sstables.is_empty() {
            return Ok(Self {
                current: None,
                next_sst_idx: 1,
                sstables,
            });
        }
        Ok(Self {
            current: Some(SsTableIterator::create_and_seek_to_first(
                sstables[0].clone(),
            )?),
            next_sst_idx: 1,
            sstables,
        })
    }

    pub fn create_and_seek_to_key(sstables: Vec<Arc<SsTable>>, key: KeySlice) -> Result<Self> {
        if sstables.is_empty() {
            return Ok(Self {
                current: None,
                next_sst_idx: 1,
                sstables,
            });
        }
        let mut temp_self = Self {
            current: None,
            next_sst_idx: 1,
            sstables,
        };
        temp_self.current = temp_self.find_sst_iterator(key)?;
        Ok(temp_self)
    }
}

impl StorageIterator for SstConcatIterator {
    type KeyType<'a> = KeySlice<'a>;

    fn key(&self) -> KeySlice {
        self.current.as_ref().unwrap().key()
    }

    fn value(&self) -> &[u8] {
        self.current.as_ref().unwrap().value()
    }

    fn is_valid(&self) -> bool {
        if self.current.is_none() {
            return false;
        }
        if !self.current.as_ref().unwrap().is_valid() && self.next_sst_idx == self.sstables.len() {
            return false;
        }
        true
    }

    fn next(&mut self) -> Result<()> {
        if let Some(current) = self.current.as_mut() {
            if current.is_valid() {
                current.next()?;
                return Ok(());
            }
        }
        self.current = Some(self.seek_sst_idx_first(self.next_sst_idx)?);
        self.next_sst_idx += 1;
        Ok(())
    }

    fn num_active_iterators(&self) -> usize {
        1
    }
}
