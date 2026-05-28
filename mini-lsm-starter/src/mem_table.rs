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

use std::ops::Bound;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use anyhow::{Ok, Result};
use bytes::Bytes;
use crossbeam_skiplist::SkipMap;
use ouroboros::self_referencing;

use crate::iterators::StorageIterator;
use crate::key::{Key, KeySlice};
use crate::table::SsTableBuilder;
use crate::vlog::{KvKind, ValueLog};
use crate::wal::Wal;

/// A basic mem-table based on crossbeam-skiplist.
///
/// An initial implementation of memtable is part of week 1, day 1. It will be incrementally implemented in other
/// chapters of week 1 and week 2.
pub struct MemTable {
    map: Arc<SkipMap<Bytes, Bytes>>,
    wal: Option<Wal>,
    id: usize,
    approximate_size: Arc<AtomicUsize>,
    /// When true, values in the map are kind-prefixed: `[KvKind:1][payload]`.
    vlog_enabled: bool,
}

/// Create a bound of `Bytes` from a bound of `&[u8]`.
pub(crate) fn map_bound(bound: Bound<&[u8]>) -> Bound<Bytes> {
    match bound {
        Bound::Included(x) => Bound::Included(Bytes::copy_from_slice(x)),
        Bound::Excluded(x) => Bound::Excluded(Bytes::copy_from_slice(x)),
        Bound::Unbounded => Bound::Unbounded,
    }
}

impl MemTable {
    /// Create a new mem-table. id is the sst_id when flush this memtable to sst
    pub fn create(id: usize) -> Self {
        Self {
            map: Arc::new(SkipMap::new()),
            wal: None,
            id,
            approximate_size: Arc::new(AtomicUsize::new(0)),
            vlog_enabled: false,
        }
    }

    /// Create a new mem-table with vLog (kind-prefixed values).
    pub fn create_vlog(id: usize) -> Self {
        Self {
            map: Arc::new(SkipMap::new()),
            wal: None,
            id,
            approximate_size: Arc::new(AtomicUsize::new(0)),
            vlog_enabled: true,
        }
    }

    /// Create a new mem-table with WAL
    pub fn create_with_wal(id: usize, path: impl AsRef<Path>) -> Result<Self> {
        let mut ret = Self::create(id);
        let wal = Wal::create(path)?;
        ret.wal = Some(wal);

        Ok(ret)
    }

    /// Create a new mem-table with WAL and vLog (kind-prefixed values).
    pub fn create_with_wal_vlog(id: usize, path: impl AsRef<Path>) -> Result<Self> {
        let mut ret = Self::create_vlog(id);
        let wal = Wal::create(path)?;
        ret.wal = Some(wal);

        Ok(ret)
    }

    /// Create a memtable from WAL
    pub fn recover_from_wal(id: usize, path: impl AsRef<Path>) -> Result<Self> {
        let mut ret = Self::create(id);
        let wal = Wal::recover(path, &ret.map)?;
        ret.wal = Some(wal);

        Ok(ret)
    }

    /// Create a memtable from WAL with vLog (kind-prefixed values).
    pub fn recover_from_wal_vlog(id: usize, path: impl AsRef<Path>) -> Result<Self> {
        let mut ret = Self::create_vlog(id);
        let wal = Wal::recover(path, &ret.map)?;
        ret.wal = Some(wal);

        Ok(ret)
    }

    pub fn for_testing_put_slice(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.put(key, value)
    }

    pub fn for_testing_get_slice(&self, key: &[u8]) -> Option<Bytes> {
        self.get(key)
    }

    pub fn for_testing_scan_slice(
        &self,
        lower: Bound<&[u8]>,
        upper: Bound<&[u8]>,
    ) -> MemTableIterator {
        self.scan(lower, upper)
    }

    /// Whether this memtable uses kind-prefixed values.
    pub fn vlog_enabled(&self) -> bool {
        self.vlog_enabled
    }

    /// Get a value by key.
    /// When vlog_enabled, strips the 1-byte KvKind prefix from the stored value.
    pub fn get(&self, key: &[u8]) -> Option<Bytes> {
        self.map.get(key).map(|x| {
            let val = x.value();
            if self.vlog_enabled && !val.is_empty() {
                // Strip the KvKind prefix byte
                val.slice(1..)
            } else {
                val.clone()
            }
        })
    }

    /// Get the raw value (with kind prefix if vlog_enabled) by key.
    pub fn get_raw(&self, key: &[u8]) -> Option<Bytes> {
        self.map.get(key).map(|x| x.value().clone())
    }

    /// Put a key-value pair into the mem-table.
    ///
    /// When vlog_enabled, wraps the value with a KvKind::Inline prefix.
    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        if self.vlog_enabled {
            // Prepend KvKind::Inline prefix
            let mut prefixed = Vec::with_capacity(1 + value.len());
            prefixed.push(crate::vlog::KvKind::Inline as u8);
            prefixed.extend_from_slice(value);
            self.put_raw(key, &prefixed)
        } else {
            self.put_raw(key, value)
        }
    }

    /// Put a raw key-value pair into the mem-table without kind prefixing.
    /// Used by compare_and_set_with_kind to write pre-prefixed values.
    pub fn put_raw(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.put_raw_batch(&[(KeySlice::from_slice(key), value)])
    }

    /// Put a batch of raw (pre-prefixed) key-value pairs.
    pub fn put_raw_batch(&self, data: &[(KeySlice, &[u8])]) -> Result<()> {
        if let Some(wal) = &self.wal {
            for (key, value) in data {
                wal.put(key.raw_ref(), value)?;
            }
            wal.sync()?;
        }

        for (key, value) in data {
            self.map.insert(
                Bytes::copy_from_slice(key.raw_ref()),
                Bytes::copy_from_slice(value),
            );

            self.approximate_size.fetch_add(
                std::mem::size_of_val(key) + std::mem::size_of_val(*value),
                std::sync::atomic::Ordering::SeqCst,
            );
        }

        Ok(())
    }

    /// Implement this in week 3, day 5.
    pub fn put_batch(&self, data: &[(KeySlice, &[u8])]) -> Result<()> {
        if self.vlog_enabled {
            // Prefix each value with KvKind::Inline
            let prefixed: Vec<(KeySlice, Vec<u8>)> = data
                .iter()
                .map(|(k, v)| {
                    let mut p = Vec::with_capacity(1 + v.len());
                    p.push(crate::vlog::KvKind::Inline as u8);
                    p.extend_from_slice(v);
                    (*k, p)
                })
                .collect();
            let refs: Vec<(KeySlice, &[u8])> =
                prefixed.iter().map(|(k, v)| (*k, v.as_slice())).collect();
            self.put_raw_batch(&refs)
        } else {
            self.put_raw_batch(data)
        }
    }

    pub fn sync_wal(&self) -> Result<()> {
        if let Some(ref wal) = self.wal {
            wal.sync()?;
        }
        Ok(())
    }

    /// Get an iterator over a range of keys.
    pub fn scan(&self, lower: Bound<&[u8]>, upper: Bound<&[u8]>) -> MemTableIterator {
        self.scan_with_vlog(lower, upper, None)
    }

    /// Get an iterator over a range of keys, with optional vLog for ValuePointer dereferencing.
    pub fn scan_with_vlog(
        &self,
        lower: Bound<&[u8]>,
        upper: Bound<&[u8]>,
        vlog: Option<Arc<ValueLog>>,
    ) -> MemTableIterator {
        let vlog_enabled = self.vlog_enabled;
        let mut iter = MemTableIteratorBuilder {
            map: self.map.clone(),
            iter_builder: |map| map.range((map_bound(lower), map_bound(upper))),
            item: (Bytes::new(), Bytes::new()),
            vlog_enabled,
            vlog,
            resolved: Bytes::new(),
        }
        .build();
        iter.next().unwrap();
        iter
    }

    /// Flush the mem-table to SSTable. Implement in week 1 day 6.
    /// When vlog_enabled, checks the KvKind prefix: ValuePointer entries are
    /// passed through via `add_raw()` to preserve the pointer; Inline entries
    /// have their prefix stripped and go through `add()` for value separation.
    pub fn flush(&self, builder: &mut SsTableBuilder) -> Result<()> {
        for e in self.map.iter() {
            let key_bytes = Key::from_bytes(e.key().clone());
            let key = key_bytes.as_key_slice();
            if self.vlog_enabled {
                let val = e.value();
                if !val.is_empty() && val[0] == crate::vlog::KvKind::ValuePointer as u8 {
                    // ValuePointer entry (from GC CAS) — pass through as-is
                    builder.add_raw(key, val)?;
                } else {
                    // Inline entry — strip prefix, let add() handle value separation
                    let raw = if !val.is_empty() {
                        &val[1..]
                    } else {
                        val.as_ref()
                    };
                    builder.add(key, raw)?;
                }
            } else {
                builder.add(key, e.value())?;
            }
        }

        Ok(())
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn approximate_size(&self) -> usize {
        self.approximate_size
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Only use this function when closing the database
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn range_overlap(&self, lower: Bound<&[u8]>, upper: Bound<&[u8]>) -> bool {
        if self.map.is_empty() {
            return false;
        }

        let e = self.map.front().unwrap();
        let lo = e.key();
        let e = self.map.back().unwrap();
        let hi = e.key();
        match (lower, upper) {
            (Bound::Included(x), Bound::Included(y)) => x >= lo && x <= hi || y >= lo && y <= hi,
            (Bound::Excluded(x), Bound::Excluded(y)) => x > lo && x < hi || y > lo && y < hi,
            _ => true,
        }
    }
}

type SkipMapRangeIter<'a> =
    crossbeam_skiplist::map::Range<'a, Bytes, (Bound<Bytes>, Bound<Bytes>), Bytes, Bytes>;

/// An iterator over a range of `SkipMap`. This is a self-referential structure and please refer to week 1, day 2
/// chapter for more information.
///
/// This is part of week 1, day 2.
#[self_referencing]
pub struct MemTableIterator {
    /// Stores a reference to the skipmap.
    map: Arc<SkipMap<Bytes, Bytes>>,
    /// Stores a skipmap iterator that refers to the lifetime of `MemTableIterator` itself.
    #[borrows(map)]
    #[not_covariant]
    iter: SkipMapRangeIter<'this>,
    /// Stores the current key-value pair.
    item: (Bytes, Bytes),
    /// Whether values are kind-prefixed (need to strip prefix in value()).
    vlog_enabled: bool,
    /// Optional vLog for dereferencing ValuePointer entries during scans.
    vlog: Option<Arc<ValueLog>>,
    /// Cached resolved value (stripped of kind prefix, ValuePointers dereferenced).
    resolved: Bytes,
}

impl StorageIterator for MemTableIterator {
    type KeyType<'a> = KeySlice<'a>;

    fn value(&self) -> &[u8] {
        if *self.borrow_vlog_enabled() {
            self.borrow_resolved().as_ref()
        } else {
            self.borrow_item().1.as_ref()
        }
    }

    fn key(&self) -> KeySlice<'_> {
        Key::from_slice(self.borrow_item().0.as_ref())
    }

    fn is_valid(&self) -> bool {
        !self.borrow_item().0.is_empty()
    }

    fn next(&mut self) -> Result<()> {
        let n = self.with_iter_mut(|iter| {
            iter.next()
                .map(|e| (e.key().clone(), e.value().clone()))
                .unwrap_or_else(|| (Bytes::from_static(&[]), Bytes::from_static(&[])))
        });

        self.with_mut(|m| {
            *m.item = n;
            *m.resolved = Self::resolve_item_value(m.vlog, m.vlog_enabled, m.item);
        });

        Ok(())
    }
}

impl MemTableIterator {
    /// Resolve the value for the current item: dereference ValuePointers via vLog,
    /// strip kind prefix for Inline entries.
    fn resolve_item_value(
        vlog: &Option<Arc<ValueLog>>,
        vlog_enabled: &bool,
        item: &(Bytes, Bytes),
    ) -> Bytes {
        let val = &item.1;
        if !*vlog_enabled || val.is_empty() {
            return val.clone();
        }

        // Not a ValuePointer — strip kind prefix (Inline or unknown)
        if val[0] != KvKind::ValuePointer as u8 {
            return val.slice(1..);
        }

        // ValuePointer — dereference through vLog
        let Some(vlog) = vlog else {
            return val.slice(1..);
        };
        let Some(ptr) = crate::vlog::ValuePointer::try_decode(&val[1..]) else {
            return Bytes::new();
        };
        match vlog.read(&ptr, &item.0) {
            std::result::Result::Ok(bytes) => bytes,
            std::result::Result::Err(_) => Bytes::new(),
        }
    }
}
