use std::{path::Path, sync::Arc};

use anyhow::{bail, Result};
use bytes::Bytes;

use crate::{
    iterators::StorageIterator,
    lsm_storage::{BlockCache, LsmStorageInner},
    table::{SsTable, SsTableBuilder},
};

#[derive(Clone)]
pub struct MockIterator {
    pub data: Vec<(Bytes, Bytes)>,
    pub error_when: Option<usize>,
    pub index: usize,
}

impl MockIterator {
    pub fn new(data: Vec<(Bytes, Bytes)>) -> Self {
        Self {
            data,
            index: 0,
            error_when: None,
        }
    }

    pub fn new_with_error(data: Vec<(Bytes, Bytes)>, error_when: usize) -> Self {
        Self {
            data,
            index: 0,
            error_when: Some(error_when),
        }
    }
}

impl StorageIterator for MockIterator {
    fn next(&mut self) -> Result<()> {
        if self.index < self.data.len() {
            self.index += 1;
        }
        if let Some(error_when) = self.error_when {
            if self.index == error_when {
                bail!("fake error!");
            }
        }
        Ok(())
    }

    fn key(&self) -> &[u8] {
        if let Some(error_when) = self.error_when {
            if self.index >= error_when {
                panic!("invalid access after next returns an error!");
            }
        }
        self.data[self.index].0.as_ref()
    }

    fn value(&self) -> &[u8] {
        if let Some(error_when) = self.error_when {
            if self.index >= error_when {
                panic!("invalid access after next returns an error!");
            }
        }
        self.data[self.index].1.as_ref()
    }

    fn is_valid(&self) -> bool {
        if let Some(error_when) = self.error_when {
            if self.index >= error_when {
                panic!("invalid access after next returns an error!");
            }
        }
        self.index < self.data.len()
    }
}

pub fn as_bytes(x: &[u8]) -> Bytes {
    Bytes::copy_from_slice(x)
}

pub fn check_iter_result(iter: &mut impl StorageIterator, expected: Vec<(Bytes, Bytes)>) {
    for (k, v) in expected {
        assert!(iter.is_valid());
        assert_eq!(
            k,
            iter.key(),
            "expected key: {:?}, actual key: {:?}",
            k,
            as_bytes(iter.key()),
        );
        assert_eq!(
            v,
            iter.value(),
            "expected value: {:?}, actual value: {:?}",
            v,
            as_bytes(iter.value()),
        );
        iter.next().unwrap();
    }
    assert!(!iter.is_valid());
}

pub fn expect_iter_error(mut iter: impl StorageIterator) {
    loop {
        match iter.next() {
            Ok(_) if iter.is_valid() => continue,
            Ok(_) => panic!("expect an error"),
            Err(_) => break,
        }
    }
}

pub fn generate_sst(
    id: usize,
    path: impl AsRef<Path>,
    data: Vec<(Bytes, Bytes)>,
    block_cache: Option<Arc<BlockCache>>,
) -> SsTable {
    let mut builder = SsTableBuilder::new(128);
    for (key, value) in data {
        builder.add(&key[..], &value[..]);
    }
    builder.build(id, block_cache, path.as_ref()).unwrap()
}

pub fn sync(storage: &LsmStorageInner) {
    storage
        .force_freeze_memtable(&storage.state_lock.lock())
        .unwrap();
    storage.force_flush_next_imm_memtable().unwrap();
}
