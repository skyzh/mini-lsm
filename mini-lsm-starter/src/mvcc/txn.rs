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

use std::{
    collections::HashSet,
    ops::Bound,
    sync::{Arc, atomic::AtomicBool},
};

use anyhow::Result;
use bytes::Bytes;
use crossbeam_skiplist::SkipMap;
use ouroboros::self_referencing;
use parking_lot::Mutex;

use crate::{
    iterators::{StorageIterator, two_merge_iterator::TwoMergeIterator},
    lsm_iterator::{FusedIterator, LsmIterator},
    lsm_storage::{LsmStorageInner, WriteBatchRecord},
};

pub struct Transaction {
    pub(crate) read_ts: u64,
    pub(crate) inner: Arc<LsmStorageInner>,
    pub(crate) local_storage: Arc<SkipMap<Bytes, Bytes>>,
    pub(crate) committed: Arc<AtomicBool>,
    /// Write set and read set
    pub(crate) key_hashes: Option<Mutex<(HashSet<u32>, HashSet<u32>)>>,
}

impl Transaction {
    pub fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        anyhow::ensure!(
            !self.committed.load(std::sync::atomic::Ordering::SeqCst),
            "transaction committed"
        );
        if let Some(hashes) = &self.key_hashes {
            hashes.lock().1.insert(farmhash::fingerprint32(key));
        }
        if let Some(value) = self.local_storage.get(key) {
            return Ok((!value.value().is_empty()).then_some(value.value().clone()));
        }
        self.inner.get_with_ts(key, self.read_ts)
    }

    pub fn scan(self: &Arc<Self>, lower: Bound<&[u8]>, upper: Bound<&[u8]>) -> Result<TxnIterator> {
        anyhow::ensure!(
            !self.committed.load(std::sync::atomic::Ordering::SeqCst),
            "transaction committed"
        );
        let lsm_iter = self.inner.scan_with_ts(lower, upper, self.read_ts)?;
        let lower = lower.map(Bytes::copy_from_slice);
        let upper = upper.map(Bytes::copy_from_slice);
        let mut local_iter = TxnLocalIteratorBuilder {
            map: self.local_storage.clone(),
            iter_builder: move |map| map.range((lower, upper)),
            item: (Bytes::new(), Bytes::new()),
        }
        .build();
        local_iter.next()?;
        TxnIterator::create(
            self.clone(),
            TwoMergeIterator::create(local_iter, lsm_iter)?,
        )
    }

    pub fn put(&self, key: &[u8], value: &[u8]) {
        assert!(
            !self.committed.load(std::sync::atomic::Ordering::SeqCst),
            "transaction committed"
        );
        self.local_storage
            .insert(Bytes::copy_from_slice(key), Bytes::copy_from_slice(value));
        if let Some(hashes) = &self.key_hashes {
            hashes.lock().0.insert(farmhash::fingerprint32(key));
        }
    }

    pub fn delete(&self, key: &[u8]) {
        assert!(
            !self.committed.load(std::sync::atomic::Ordering::SeqCst),
            "transaction committed"
        );
        self.local_storage
            .insert(Bytes::copy_from_slice(key), Bytes::new());
        if let Some(hashes) = &self.key_hashes {
            hashes.lock().0.insert(farmhash::fingerprint32(key));
        }
    }

    pub fn commit(&self) -> Result<()> {
        self.committed
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
            )
            .map_err(|_| anyhow::anyhow!("transaction committed"))?;
        let batch = self
            .local_storage
            .iter()
            .map(|entry| {
                if entry.value().is_empty() {
                    WriteBatchRecord::Del(entry.key().clone())
                } else {
                    WriteBatchRecord::Put(entry.key().clone(), entry.value().clone())
                }
            })
            .collect::<Vec<_>>();
        if batch.is_empty() {
            return Ok(());
        }
        let _commit_guard = self.inner.mvcc().commit_lock.lock();
        let (write_set, read_set) = self
            .key_hashes
            .as_ref()
            .map(|hashes| hashes.lock().clone())
            .unwrap_or_default();
        if self
            .inner
            .mvcc()
            .committed_txns
            .lock()
            .range((
                std::ops::Bound::Excluded(self.read_ts),
                std::ops::Bound::Unbounded,
            ))
            .any(|(_, txn)| !txn.key_hashes.is_disjoint(&read_set))
        {
            anyhow::bail!("serializable validation failed");
        }
        self.inner.write_batch(&batch)?;
        let commit_ts = self.inner.mvcc().latest_commit_ts();
        self.inner.mvcc().committed_txns.lock().insert(
            commit_ts,
            crate::mvcc::CommittedTxnData {
                key_hashes: write_set,
                read_ts: self.read_ts,
                commit_ts,
            },
        );
        Ok(())
    }
}

impl Drop for Transaction {
    fn drop(&mut self) {
        self.inner.mvcc().ts.lock().1.remove_reader(self.read_ts);
    }
}

type SkipMapRangeIter<'a> =
    crossbeam_skiplist::map::Range<'a, Bytes, (Bound<Bytes>, Bound<Bytes>), Bytes, Bytes>;

#[self_referencing]
pub struct TxnLocalIterator {
    /// Stores a reference to the skipmap.
    map: Arc<SkipMap<Bytes, Bytes>>,
    /// Stores a skipmap iterator that refers to the lifetime of `TxnLocalIterator` itself.
    #[borrows(map)]
    #[not_covariant]
    iter: SkipMapRangeIter<'this>,
    /// Stores the current key-value pair.
    item: (Bytes, Bytes),
}

impl StorageIterator for TxnLocalIterator {
    type KeyType<'a> = &'a [u8];

    fn value(&self) -> &[u8] {
        &self.borrow_item().1
    }

    fn key(&self) -> &[u8] {
        &self.borrow_item().0
    }

    fn is_valid(&self) -> bool {
        !self.borrow_item().0.is_empty()
    }

    fn next(&mut self) -> Result<()> {
        let item = self.with_iter_mut(|iter| {
            iter.next()
                .map(|entry| (entry.key().clone(), entry.value().clone()))
                .unwrap_or_default()
        });
        self.with_item_mut(|current| *current = item);
        Ok(())
    }
}

pub struct TxnIterator {
    _txn: Arc<Transaction>,
    iter: TwoMergeIterator<TxnLocalIterator, FusedIterator<LsmIterator>>,
}

impl TxnIterator {
    pub fn create(
        txn: Arc<Transaction>,
        iter: TwoMergeIterator<TxnLocalIterator, FusedIterator<LsmIterator>>,
    ) -> Result<Self> {
        let mut this = Self { _txn: txn, iter };
        this.skip_tombstones()?;
        this.record_current_key();
        Ok(this)
    }

    fn record_current_key(&self) {
        if self.iter.is_valid()
            && let Some(hashes) = &self._txn.key_hashes
        {
            hashes
                .lock()
                .1
                .insert(farmhash::fingerprint32(self.iter.key()));
        }
    }

    fn skip_tombstones(&mut self) -> Result<()> {
        while self.iter.is_valid() && self.iter.value().is_empty() {
            self.iter.next()?;
        }
        Ok(())
    }
}

impl StorageIterator for TxnIterator {
    type KeyType<'a>
        = &'a [u8]
    where
        Self: 'a;

    fn value(&self) -> &[u8] {
        self.iter.value()
    }

    fn key(&self) -> Self::KeyType<'_> {
        self.iter.key()
    }

    fn is_valid(&self) -> bool {
        self.iter.is_valid()
    }

    fn next(&mut self) -> Result<()> {
        self.iter.next()?;
        self.skip_tombstones()?;
        self.record_current_key();
        Ok(())
    }

    fn num_active_iterators(&self) -> usize {
        self.iter.num_active_iterators()
    }
}
