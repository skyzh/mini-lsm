use std::ops::Bound;

use bytes::Bytes;
use tempfile::tempdir;

use crate::iterators::StorageIterator;

fn as_bytes(x: &[u8]) -> Bytes {
    Bytes::copy_from_slice(x)
}

fn check_iter_result(iter: impl StorageIterator, expected: Vec<(Bytes, Bytes)>) {
    let mut iter = iter;
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

#[test]
fn test_storage_get() {
    use crate::lsm_storage::LsmStorage;
    let dir = tempdir().unwrap();
    let storage = LsmStorage::open(&dir).unwrap();
    storage.put(b"1", b"233").unwrap();
    storage.put(b"2", b"2333").unwrap();
    storage.put(b"3", b"23333").unwrap();
    assert_eq!(&storage.get(b"1").unwrap().unwrap()[..], b"233");
    assert_eq!(&storage.get(b"2").unwrap().unwrap()[..], b"2333");
    assert_eq!(&storage.get(b"3").unwrap().unwrap()[..], b"23333");
    storage.delete(b"2").unwrap();
    assert!(storage.get(b"2").unwrap().is_none());
}

#[test]
fn test_storage_scan_memtable_1() {
    use crate::lsm_storage::LsmStorage;
    let dir = tempdir().unwrap();
    let storage = LsmStorage::open(&dir).unwrap();
    storage.put(b"1", b"233").unwrap();
    storage.put(b"2", b"2333").unwrap();
    storage.put(b"3", b"23333").unwrap();
    storage.delete(b"2").unwrap();
    check_iter_result(
        storage.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("1"), Bytes::from("233")),
            (Bytes::from("3"), Bytes::from("23333")),
        ],
    );
    check_iter_result(
        storage
            .scan(Bound::Included(b"1"), Bound::Included(b"2"))
            .unwrap(),
        vec![(Bytes::from("1"), Bytes::from("233"))],
    );
    check_iter_result(
        storage
            .scan(Bound::Excluded(b"1"), Bound::Excluded(b"3"))
            .unwrap(),
        vec![],
    );
}

#[test]
fn test_storage_scan_memtable_2() {
    use crate::lsm_storage::LsmStorage;
    let dir = tempdir().unwrap();
    let storage = LsmStorage::open(&dir).unwrap();
    storage.put(b"1", b"233").unwrap();
    storage.put(b"2", b"2333").unwrap();
    storage.put(b"3", b"23333").unwrap();
    storage.delete(b"1").unwrap();
    check_iter_result(
        storage.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("2"), Bytes::from("2333")),
            (Bytes::from("3"), Bytes::from("23333")),
        ],
    );
    check_iter_result(
        storage
            .scan(Bound::Included(b"1"), Bound::Included(b"2"))
            .unwrap(),
        vec![(Bytes::from("2"), Bytes::from("2333"))],
    );
    check_iter_result(
        storage
            .scan(Bound::Excluded(b"1"), Bound::Excluded(b"3"))
            .unwrap(),
        vec![(Bytes::from("2"), Bytes::from("2333"))],
    );
}

#[test]
fn test_storage_get_after_sync() {
    use crate::lsm_storage::LsmStorage;
    let dir = tempdir().unwrap();
    let storage = LsmStorage::open(&dir).unwrap();
    storage.put(b"1", b"233").unwrap();
    storage.put(b"2", b"2333").unwrap();
    storage.sync().unwrap();
    storage.put(b"3", b"23333").unwrap();
    assert_eq!(&storage.get(b"1").unwrap().unwrap()[..], b"233");
    assert_eq!(&storage.get(b"2").unwrap().unwrap()[..], b"2333");
    assert_eq!(&storage.get(b"3").unwrap().unwrap()[..], b"23333");
    storage.delete(b"2").unwrap();
    assert!(storage.get(b"2").unwrap().is_none());
}

#[test]
fn test_storage_scan_memtable_1_after_sync() {
    use crate::lsm_storage::LsmStorage;
    let dir = tempdir().unwrap();
    let storage = LsmStorage::open(&dir).unwrap();
    storage.put(b"1", b"233").unwrap();
    storage.put(b"2", b"2333").unwrap();
    storage.sync().unwrap();
    storage.put(b"3", b"23333").unwrap();
    storage.delete(b"2").unwrap();
    check_iter_result(
        storage.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("1"), Bytes::from("233")),
            (Bytes::from("3"), Bytes::from("23333")),
        ],
    );
    check_iter_result(
        storage
            .scan(Bound::Included(b"1"), Bound::Included(b"2"))
            .unwrap(),
        vec![(Bytes::from("1"), Bytes::from("233"))],
    );
    check_iter_result(
        storage
            .scan(Bound::Excluded(b"1"), Bound::Excluded(b"3"))
            .unwrap(),
        vec![],
    );
}

#[test]
fn test_storage_scan_memtable_2_after_sync() {
    use crate::lsm_storage::LsmStorage;
    let dir = tempdir().unwrap();
    let storage = LsmStorage::open(&dir).unwrap();
    storage.put(b"1", b"233").unwrap();
    storage.put(b"2", b"2333").unwrap();
    storage.sync().unwrap();
    storage.put(b"3", b"23333").unwrap();
    storage.sync().unwrap();
    storage.delete(b"1").unwrap();
    check_iter_result(
        storage.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        vec![
            (Bytes::from("2"), Bytes::from("2333")),
            (Bytes::from("3"), Bytes::from("23333")),
        ],
    );
    check_iter_result(
        storage
            .scan(Bound::Included(b"1"), Bound::Included(b"2"))
            .unwrap(),
        vec![(Bytes::from("2"), Bytes::from("2333"))],
    );
    check_iter_result(
        storage
            .scan(Bound::Excluded(b"1"), Bound::Excluded(b"3"))
            .unwrap(),
        vec![(Bytes::from("2"), Bytes::from("2333"))],
    );
}
