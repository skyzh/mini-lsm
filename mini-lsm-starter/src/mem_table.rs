#![allow(dead_code)] // REMOVE THIS LINE after fully implementing this functionality

use crossbeam_skiplist::map::SkipMap;
use std::ops::Bound;
use std::path::Path;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use anyhow::Result;
use bytes::Bytes;
use crossbeam_skiplist::map::Entry;
use ouroboros::self_referencing;

use crate::iterators::StorageIterator;
use crate::key::{KeyBytes, KeySlice, TS_DEFAULT, TS_RANGE_BEGIN, TS_RANGE_END};
use crate::table::SsTableBuilder;
use crate::wal::Wal;

/// A basic mem-table based on crossbeam-skiplist.
///
/// An initial implementation of memtable is part of week 1, day 1. It will be incrementally implemented in other
/// chapters of week 1 and week 2.
pub struct MemTable {
    map: Arc<SkipMap<KeyBytes, Bytes>>,
    wal: Option<Wal>,
    id: usize,
    approximate_size: Arc<AtomicUsize>,
}

pub(crate) fn key_slice_to_key_bytes(key_slice: KeySlice) -> KeyBytes {
    KeyBytes::from_bytes_with_ts(
        Bytes::from_static(unsafe { std::mem::transmute::<&[u8], &[u8]>(key_slice.key_ref()) }),
        key_slice.ts(),
    )
}

pub(crate) fn slice_to_bytes(key: &[u8]) -> Bytes {
    Bytes::from_static(unsafe { std::mem::transmute::<&[u8], &[u8]>(key) })
}

/// Create a bound of `Bytes` from a bound of `&[u8]`.
pub(crate) fn map_key_slice_bound(bound: Bound<KeySlice>) -> Bound<KeyBytes> {
    match bound {
        Bound::Included(x) => Bound::Included(key_slice_to_key_bytes(x)),
        Bound::Excluded(x) => Bound::Excluded(key_slice_to_key_bytes(x)),
        Bound::Unbounded => Bound::Unbounded,
    }
}

pub(crate) fn map_bound_bytes(bound: Bound<&[u8]>) -> Bound<Bytes> {
    match bound {
        Bound::Included(x) => Bound::Included(slice_to_bytes(x)),
        Bound::Excluded(x) => Bound::Excluded(slice_to_bytes(x)),
        Bound::Unbounded => Bound::Unbounded,
    }
}

pub(crate) fn map_bound(bound: Bound<&[u8]>, ts: u64) -> Bound<KeySlice> {
    match bound {
        Bound::Included(x) => Bound::Included(KeySlice::from_slice(x, ts)),
        Bound::Excluded(x) => Bound::Excluded(KeySlice::from_slice(x, ts)),
        Bound::Unbounded => Bound::Unbounded,
    }
}

pub(crate) fn ts_bound_mapper(bound: Bound<&[u8]>, is_lower: bool) -> u64 {
    match bound {
        Bound::Included(_) => {
            if is_lower {
                TS_RANGE_BEGIN
            } else {
                TS_RANGE_END
            }
        }
        Bound::Excluded(_) => {
            if is_lower {
                TS_RANGE_END
            } else {
                TS_RANGE_BEGIN
            }
        }
        Bound::Unbounded => TS_DEFAULT,
    }
}
impl MemTable {
    /// Create a new mem-table.
    pub fn create(id: usize) -> Self {
        Self {
            id,
            map: Arc::new(SkipMap::new()),
            wal: None,
            approximate_size: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Create a new mem-table with WAL
    pub fn create_with_wal(id: usize, path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            id,
            map: Arc::new(SkipMap::new()),
            wal: Some(Wal::create(path)?),
            approximate_size: Arc::new(AtomicUsize::new(0)),
        })
    }

    /// Create a memtable from WAL
    pub fn recover_from_wal(id: usize, path: impl AsRef<Path>) -> Result<Self> {
        let skiplist = SkipMap::<KeyBytes, Bytes>::new();
        // println!("path {:?}", path.as_ref());
        let wal = Wal::recover(path, &skiplist)?;

        for entry in skiplist.iter() {
            println!(
                "Key: {}, Value: {}",
                String::from_utf8_lossy(entry.key().key_ref()),
                String::from_utf8_lossy(entry.value()),
            );
        }
        Ok(Self {
            id,
            map: Arc::new(skiplist),
            wal: Some(wal),
            approximate_size: Arc::new(AtomicUsize::new(0)),
        })
    }

    pub fn for_testing_put_slice(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.put(KeySlice::from_slice(key, TS_DEFAULT), value)
    }

    pub fn for_testing_get_slice(&self, key: &[u8]) -> Option<Bytes> {
        self.get(KeySlice::from_slice(key, TS_DEFAULT))
    }

    pub fn for_testing_scan_slice(
        &self,
        lower: Bound<&[u8]>,
        upper: Bound<&[u8]>,
    ) -> MemTableIterator {
        self.scan(
            map_bound(lower, ts_bound_mapper(lower, true)),
            map_bound(upper, ts_bound_mapper(upper, false)),
        )
    }

    /// Get a value by key.
    pub fn get(&self, _key: KeySlice) -> Option<Bytes> {
        self.map
            .get(&KeyBytes::from_bytes_with_ts(
                Bytes::from_static(unsafe { std::mem::transmute::<&[u8], &[u8]>(_key.key_ref()) }),
                _key.ts(),
            ))
            .map(|e| e.value().clone())
    }

    /// Put a key-value pair into the mem-table.
    ///
    /// In week 1, day 1, simply put the key-value pair into the skipmap.
    /// In week 2, day 6, also flush the data to WAL.
    /// In week 3, day 5, modify the function to use the batch API.
    pub fn put(&self, _key: KeySlice, _value: &[u8]) -> Result<()> {
        self.map.insert(
            KeyBytes::from_bytes_with_ts(Bytes::copy_from_slice(_key.key_ref()), _key.ts()),
            Bytes::copy_from_slice(_value),
        );
        if let Some(wal) = &self.wal {
            wal.put(_key, _value)?;
        }
        let size = _key.raw_len() + _value.len();
        self.approximate_size
            .fetch_add(size, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// Implement this in week 3, day 5.
    pub fn put_batch(&self, _data: &[(KeySlice, &[u8])]) -> Result<()> {
        unimplemented!()
    }

    pub fn sync_wal(&self) -> Result<()> {
        if let Some(ref wal) = self.wal {
            println!("Syncing Wal {:?}", self.id());
            wal.sync()?;
        }
        Ok(())
    }

    /// Get an iterator over a range of keys.
    pub fn scan(&self, _lower: Bound<KeySlice>, _upper: Bound<KeySlice>) -> MemTableIterator {
        let (lower, upper) = (map_key_slice_bound(_lower), map_key_slice_bound(_upper));
        let mut iter = MemTableIteratorBuilder {
            map: self.map.clone(),
            iter_builder: |map| map.range((lower, upper)),
            item: (KeyBytes::new(), Bytes::new()),
        }
        .build();

        let entry = iter.with_iter_mut(|iter| MemTableIterator::entry_to_item(iter.next()));
        iter.with_mut(|x| *x.item = entry);
        iter
    }

    /// Flush the mem-table to SSTable. Implement in week 1 day 6.
    pub fn flush(&self, builder: &mut SsTableBuilder) -> Result<()> {
        for entry in self.map.iter() {
            let key = entry.key(); // Access the key from the Entry
            let value = entry.value();
            // println!("Flush Key: {:?}", Bytes::copy_from_slice(value));
            builder.add(key.as_key_slice(), value);
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
}

type SkipMapRangeIter<'a> = crossbeam_skiplist::map::Range<
    'a,
    KeyBytes,
    (Bound<KeyBytes>, Bound<KeyBytes>),
    KeyBytes,
    Bytes,
>;

/// An iterator over a range of `SkipMap`. This is a self-referential structure and please refer to week 1, day 2
/// chapter for more information.
///
/// This is part of week 1, day 2.
#[self_referencing]
pub struct MemTableIterator {
    /// Stores a reference to the skipmap.
    map: Arc<SkipMap<KeyBytes, Bytes>>,
    /// Stores a skipmap iterator that refers to the lifetime of `MemTableIterator` itself.
    #[borrows(map)]
    #[not_covariant]
    iter: SkipMapRangeIter<'this>,
    /// Stores the current key-value pair.
    item: (KeyBytes, Bytes),
}

impl MemTableIterator {
    fn entry_to_item(entry_data: Option<Entry<'_, KeyBytes, Bytes>>) -> (KeyBytes, Bytes) {
        entry_data
            .map(|x| (x.key().clone(), x.value().clone()))
            .unwrap_or_else(|| (KeyBytes::new(), Bytes::new()))
    }
}
impl StorageIterator for MemTableIterator {
    type KeyType<'a> = KeySlice<'a>;

    fn value(&self) -> &[u8] {
        &self.borrow_item().1[..]
    }

    fn key(&self) -> KeySlice {
        KeySlice::from_slice(self.borrow_item().0.key_ref(), self.borrow_item().0.ts())
    }

    fn is_valid(&self) -> bool {
        !self.borrow_item().0.is_empty()
    }

    fn next(&mut self) -> Result<()> {
        let entry = self.with_iter_mut(|iter| MemTableIterator::entry_to_item(iter.next()));
        self.with_mut(|x| *x.item = entry);
        Ok(())
    }
}
