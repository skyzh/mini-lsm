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

use crate::iterators::concat_iterator::SstConcatIterator;
use crate::{
    iterators::{
        merge_iterator::MergeIterator, two_merge_iterator::TwoMergeIterator, StorageIterator,
    },
    mem_table::MemTableIterator,
    table::SsTableIterator,
};
use anyhow::{bail, Result};
use bytes::Bytes;
use std::ops::Bound;

/// Represents the internal type for an LSM iterator. This type will be changed across the tutorial for multiple times.
type LsmIteratorInner = TwoMergeIterator<
    TwoMergeIterator<MergeIterator<MemTableIterator>, MergeIterator<SsTableIterator>>,
    MergeIterator<SstConcatIterator>,
>;


pub struct LsmIterator {
    inner: LsmIteratorInner,
    end_bound: Bound<Bytes>,
    is_valid: bool,
    prev_key: Vec<u8>,
    // prev_ts: u64,
    read_ts: u64,
}

impl LsmIterator {
    pub(crate) fn new(
        iter: LsmIteratorInner,
        end_bound: Bound<Bytes>,
        read_ts: u64,
    ) -> Result<Self> {
        let mut iter = Self {
            inner: iter,
            end_bound,
            is_valid: true,
            prev_key: Vec::<u8>::new(),
            // prev_ts: TS_DEFAULT,
            read_ts,
        };
        iter.move_to_the_correct_key()?;
        Ok(iter)
    }
}

impl LsmIterator {
    fn next_inner(&mut self) -> Result<()> {
        self.prev_key = self.key().to_vec();
        self.inner.next()?;
        if !self.inner.is_valid() {
            self.is_valid = false;
            return Ok(());
        }

        match self.end_bound.as_ref() {
            Bound::Unbounded => {}
            Bound::Included(key) => {
                self.is_valid = self.key() <= key;
            }
            Bound::Excluded(key) => {
                self.is_valid = self.key() < key;
            }
        }

        Ok(())
    }
    fn move_to_the_correct_key(&mut self) -> Result<()> {
        let mut current_key = self.inner.key().key_ref().to_vec();
        // While the value is non empty keep moving if the current key is same as prev until the read_ts is same
        while self.is_valid()
            && (self.inner.value().is_empty()
                || (current_key == self.prev_key && self.inner.key().ts() <= self.read_ts))
        {
            self.prev_key = current_key.clone();
            // self.prev_ts = self.inner.key().ts();
            self.next_inner()?;
            if self.is_valid() {
                current_key = self.inner.key().key_ref().to_vec();
            }
        }

        Ok(())
    }
}

impl StorageIterator for LsmIterator {
    type KeyType<'a> = &'a [u8];

    fn is_valid(&self) -> bool {
        self.is_valid
    }

    fn key(&self) -> &[u8] {
        if self.inner.key().ts() > self.read_ts {
            return &self.prev_key[..];
        }
        self.inner.key().key_ref()
    }

    fn value(&self) -> &[u8] {
        self.inner.value()
    }

    fn next(&mut self) -> Result<()> {
        self.next_inner()?;
        self.move_to_the_correct_key()?;
        Ok(())
    }

    fn num_active_iterators(&self) -> usize {
        self.inner.num_active_iterators()
    }
}

/// A wrapper around existing iterator, will prevent users from calling `next` when the iterator is
/// invalid. If an iterator is already invalid, `next` does not do anything. If `next` returns an error,
/// `is_valid` should return false, and `next` should always return an error.
pub struct FusedIterator<I: StorageIterator> {
    iter: I,
    has_errored: bool,
}

impl<I: StorageIterator> FusedIterator<I> {
    pub fn new(iter: I) -> Self {
        Self {
            iter,
            has_errored: false,
        }
    }
}

impl<I: StorageIterator> StorageIterator for FusedIterator<I> {
    type KeyType<'a>
        = I::KeyType<'a>
    where
        Self: 'a;

    fn is_valid(&self) -> bool {
        if self.has_errored {
            return false;
        }
        self.iter.is_valid()
    }

    fn key(&self) -> Self::KeyType<'_> {
        if !self.is_valid() {
            panic!("Invalid Access to the key, iterator is expired");
        }
        return self.iter.key();
    }

    fn value(&self) -> &[u8] {
        if !self.is_valid() {
            panic!("Invalid Access to the key, iterator is expired");
        }
        return self.iter.value();
    }

    fn next(&mut self) -> Result<()> {
        if self.has_errored {
            bail!("Underlying iterator has errored");
        } else if self.is_valid() {
            if let Err(e) = self.iter.next() {
                self.has_errored = true;
                return Err(e);
            }
        }
        Ok(())
    }

    fn num_active_iterators(&self) -> usize {
        self.iter.num_active_iterators()
    }
}
