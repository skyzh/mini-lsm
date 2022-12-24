use tempfile::tempdir;

use super::MemTable;
use crate::iterators::StorageIterator;
use crate::table::{SsTableBuilder, SsTableIterator};

#[test]
fn test_memtable_get() {
    let memtable = MemTable::create();
    memtable.put(b"key1", b"value1");
    memtable.put(b"key2", b"value2");
    memtable.put(b"key3", b"value3");
    assert_eq!(&memtable.get(b"key1").unwrap()[..], b"value1");
    assert_eq!(&memtable.get(b"key2").unwrap()[..], b"value2");
    assert_eq!(&memtable.get(b"key3").unwrap()[..], b"value3");
}

#[test]
fn test_memtable_overwrite() {
    let memtable = MemTable::create();
    memtable.put(b"key1", b"value1");
    memtable.put(b"key2", b"value2");
    memtable.put(b"key3", b"value3");
    memtable.put(b"key1", b"value11");
    memtable.put(b"key2", b"value22");
    memtable.put(b"key3", b"value33");
    assert_eq!(&memtable.get(b"key1").unwrap()[..], b"value11");
    assert_eq!(&memtable.get(b"key2").unwrap()[..], b"value22");
    assert_eq!(&memtable.get(b"key3").unwrap()[..], b"value33");
}

#[test]
fn test_memtable_flush() {
    let memtable = MemTable::create();
    memtable.put(b"key1", b"value1");
    memtable.put(b"key2", b"value2");
    memtable.put(b"key3", b"value3");
    let mut builder = SsTableBuilder::new(128);
    memtable.flush(&mut builder).unwrap();
    let dir = tempdir().unwrap();
    let sst = builder.build_for_test(dir.path().join("1.sst")).unwrap();
    let mut iter = SsTableIterator::create_and_seek_to_first(sst.into()).unwrap();
    assert_eq!(iter.key(), b"key1");
    assert_eq!(iter.value(), b"value1");
    iter.next().unwrap();
    assert_eq!(iter.key(), b"key2");
    assert_eq!(iter.value(), b"value2");
    iter.next().unwrap();
    assert_eq!(iter.key(), b"key3");
    assert_eq!(iter.value(), b"value3");
    iter.next().unwrap();
    assert!(!iter.is_valid());
}

#[test]
fn test_memtable_iter() {
    use std::ops::Bound;
    let memtable = MemTable::create();
    memtable.put(b"key1", b"value1");
    memtable.put(b"key2", b"value2");
    memtable.put(b"key3", b"value3");

    {
        let mut iter = memtable.scan(Bound::Unbounded, Bound::Unbounded);
        assert_eq!(iter.key(), b"key1");
        assert_eq!(iter.value(), b"value1");
        iter.next().unwrap();
        assert_eq!(iter.key(), b"key2");
        assert_eq!(iter.value(), b"value2");
        iter.next().unwrap();
        assert_eq!(iter.key(), b"key3");
        assert_eq!(iter.value(), b"value3");
        iter.next().unwrap();
        assert!(!iter.is_valid());
    }

    {
        let mut iter = memtable.scan(Bound::Included(b"key1"), Bound::Included(b"key2"));
        assert_eq!(iter.key(), b"key1");
        assert_eq!(iter.value(), b"value1");
        iter.next().unwrap();
        assert_eq!(iter.key(), b"key2");
        assert_eq!(iter.value(), b"value2");
        iter.next().unwrap();
        assert!(!iter.is_valid());
    }

    {
        let mut iter = memtable.scan(Bound::Excluded(b"key1"), Bound::Excluded(b"key3"));
        assert_eq!(iter.key(), b"key2");
        assert_eq!(iter.value(), b"value2");
        iter.next().unwrap();
        assert!(!iter.is_valid());
    }
}
