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

use std::sync::Arc;

use anyhow::{Ok, Result};

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
        if sstables.is_empty() {
            return Ok(Self {
                current: None,
                next_sst_idx: 0,
                sstables,
            });
        }

        let it = SsTableIterator::create_and_seek_to_first(sstables[0].clone())?;
        let ret = SstConcatIterator {
            current: Some(it),
            next_sst_idx: 1,
            sstables,
        };

        Ok(ret)
    }

    pub fn create_and_seek_to_key(sstables: Vec<Arc<SsTable>>, key: KeySlice) -> Result<Self> {
        if sstables.is_empty() {
            return Ok(Self {
                current: None,
                next_sst_idx: 0,
                sstables,
            });
        }
        if key > sstables[sstables.len() - 1].last_key().as_key_slice() {
            return Ok(Self {
                current: None,
                next_sst_idx: 0,
                sstables,
            });
        }

        let mut lo = 0;
        let mut hi = sstables.len() - 1;
        let mut mid = 0;
        while lo <= hi {
            mid = lo + (hi - lo) / 2;
            let s = sstables[mid].clone();
            if key >= s.first_key().as_key_slice() && key <= s.last_key().as_key_slice() {
                lo = mid;
                break;
            }
            if key < s.first_key().as_key_slice() {
                if mid == 0 {
                    lo = 0;
                    break;
                }
                hi = mid - 1;
            } else if key > s.last_key().as_key_slice() {
                if mid == sstables.len() - 1 {
                    lo = sstables.len() - 1;
                    break;
                }
                lo = mid + 1;
            }
        }

        let it = SsTableIterator::create_and_seek_to_key(sstables[lo].clone(), key)?;
        let ret = SstConcatIterator {
            current: Some(it),
            next_sst_idx: mid + 1,
            sstables,
        };

        Ok(ret)
    }
}

impl StorageIterator for SstConcatIterator {
    type KeyType<'a> = KeySlice<'a>;

    fn key(&'_ self) -> KeySlice<'_> {
        // if !self.is_valid() {
        //     return KeySlice::from_slice(&[]);
        // }

        self.current.as_ref().unwrap().key()
    }

    fn value(&self) -> &[u8] {
        // if !self.is_valid() {
        //     return &[];
        // }

        self.current.as_ref().unwrap().value()
    }

    fn is_valid(&self) -> bool {
        self.current.is_some() && self.current.as_ref().unwrap().is_valid()
    }

    fn next(&mut self) -> Result<()> {
        // if !self.is_valid() {
        //     return Ok(());
        // }

        self.current.as_mut().unwrap().next()?;

        while let Some(ref current) = self.current {
            if current.is_valid() {
                break;
            }
            if self.next_sst_idx < self.sstables.len() {
                self.current = Some(SsTableIterator::create_and_seek_to_first(
                    self.sstables[self.next_sst_idx].clone(),
                )?);
                self.next_sst_idx += 1;
            } else {
                self.current = None;
            }
        }

        Ok(())
    }

    fn num_active_iterators(&self) -> usize {
        1
    }
}
