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
    pub fn create_and_seek_to_first(sstables: Vec<Arc<SsTable>>) -> Result<Self> {
        let current = sstables
            .first()
            .map(|sst| SsTableIterator::create_and_seek_to_first(sst.clone()))
            .transpose()?;
        let next_sst_idx = usize::from(current.is_some());
        Ok(Self {
            current,
            next_sst_idx,
            sstables,
        })
    }

    pub fn create_and_seek_to_key(sstables: Vec<Arc<SsTable>>, key: KeySlice) -> Result<Self> {
        let current_sst_idx = sstables.partition_point(|sst| sst.last_key().as_key_slice() < key);
        let current = sstables
            .get(current_sst_idx)
            .map(|sst| SsTableIterator::create_and_seek_to_key(sst.clone(), key))
            .transpose()?;
        let next_sst_idx = current_sst_idx + usize::from(current.is_some());
        Ok(Self {
            current,
            next_sst_idx,
            sstables,
        })
    }
}

impl StorageIterator for SstConcatIterator {
    type KeyType<'a> = KeySlice<'a>;

    fn key(&self) -> KeySlice<'_> {
        self.current.as_ref().unwrap().key()
    }

    fn value(&self) -> &[u8] {
        self.current.as_ref().unwrap().value()
    }

    fn is_valid(&self) -> bool {
        self.current.as_ref().is_some_and(StorageIterator::is_valid)
    }

    fn next(&mut self) -> Result<()> {
        let Some(current) = &mut self.current else {
            return Ok(());
        };
        current.next()?;
        if current.is_valid() {
            return Ok(());
        }

        self.current = self
            .sstables
            .get(self.next_sst_idx)
            .map(|sst| SsTableIterator::create_and_seek_to_first(sst.clone()))
            .transpose()?;
        self.next_sst_idx += usize::from(self.current.is_some());
        Ok(())
    }

    fn num_active_iterators(&self) -> usize {
        1
    }
}
