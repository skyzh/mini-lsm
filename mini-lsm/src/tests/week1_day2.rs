use std::{ops::Bound, sync::Arc};

use bytes::Bytes;
use tempfile::tempdir;

use crate::{
    iterators::{merge_iterator::MergeIterator, StorageIterator},
    lsm_iterator::FusedIterator,
    lsm_storage::{LsmStorageInner, LsmStorageOptions},
    mem_table::MemTable,
    tests::harness::check_lsm_iter_result_by_key,
};

use super::harness::{check_iter_result_by_key, expect_iter_error, MockIterator};

#[test]
fn test_task1_memtable_iter() {
    use std::ops::Bound;
    let memtable = MemTable::create(0);
    memtable.for_testing_put_slice(b"key1", b"value1").unwrap();
    memtable.for_testing_put_slice(b"key2", b"value2").unwrap();
    memtable.for_testing_put_slice(b"key3", b"value3").unwrap();

    {
        let mut iter = memtable.for_testing_scan_slice(Bound::Unbounded, Bound::Unbounded);
        assert_eq!(iter.key().for_testing_key_ref(), b"key1");
        assert_eq!(iter.value(), b"value1");
        assert!(iter.is_valid());
        iter.next().unwrap();
        assert_eq!(iter.key().for_testing_key_ref(), b"key2");
        assert_eq!(iter.value(), b"value2");
        assert!(iter.is_valid());
        iter.next().unwrap();
        assert_eq!(iter.key().for_testing_key_ref(), b"key3");
        assert_eq!(iter.value(), b"value3");
        assert!(iter.is_valid());
        iter.next().unwrap();
        assert!(!iter.is_valid());
    }

    {
        let mut iter =
            memtable.for_testing_scan_slice(Bound::Included(b"key1"), Bound::Included(b"key2"));
        assert_eq!(iter.key().for_testing_key_ref(), b"key1");
        assert_eq!(iter.value(), b"value1");
        assert!(iter.is_valid());
        iter.next().unwrap();
        assert_eq!(iter.key().for_testing_key_ref(), b"key2");
        assert_eq!(iter.value(), b"value2");
        assert!(iter.is_valid());
        iter.next().unwrap();
        assert!(!iter.is_valid());
    }

    {
        let mut iter =
            memtable.for_testing_scan_slice(Bound::Excluded(b"key1"), Bound::Excluded(b"key3"));
        assert_eq!(iter.key().for_testing_key_ref(), b"key2");
        assert_eq!(iter.value(), b"value2");
        assert!(iter.is_valid());
        iter.next().unwrap();
        assert!(!iter.is_valid());
    }
}

#[test]
fn test_task1_empty_memtable_iter() {
    use std::ops::Bound;
    let memtable = MemTable::create(0);
    {
        let iter =
            memtable.for_testing_scan_slice(Bound::Excluded(b"key1"), Bound::Excluded(b"key3"));
        assert!(!iter.is_valid());
    }
    {
        let iter =
            memtable.for_testing_scan_slice(Bound::Included(b"key1"), Bound::Included(b"key2"));
        assert!(!iter.is_valid());
    }
    {
        let iter = memtable.for_testing_scan_slice(Bound::Unbounded, Bound::Unbounded);
        assert!(!iter.is_valid());
    }
}

#[test]
fn test_task2_merge_1() {
    let i1 = MockIterator::new(vec![
        (Bytes::from("a"), Bytes::from("1.1")),
        (Bytes::from("b"), Bytes::from("2.1")),
        (Bytes::from("c"), Bytes::from("3.1")),
        (Bytes::from("e"), Bytes::new()),
    ]);
    let i2 = MockIterator::new(vec![
        (Bytes::from("a"), Bytes::from("1.2")),
        (Bytes::from("b"), Bytes::from("2.2")),
        (Bytes::from("c"), Bytes::from("3.2")),
        (Bytes::from("d"), Bytes::from("4.2")),
    ]);
    let i3 = MockIterator::new(vec![
        (Bytes::from("b"), Bytes::from("2.3")),
        (Bytes::from("c"), Bytes::from("3.3")),
        (Bytes::from("d"), Bytes::from("4.3")),
    ]);

    let mut iter = MergeIterator::create(vec![
        Box::new(i1.clone()),
        Box::new(i2.clone()),
        Box::new(i3.clone()),
    ]);

    check_iter_result_by_key(
        &mut iter,
        vec![
            (Bytes::from("a"), Bytes::from("1.1")),
            (Bytes::from("b"), Bytes::from("2.1")),
            (Bytes::from("c"), Bytes::from("3.1")),
            (Bytes::from("d"), Bytes::from("4.2")),
            (Bytes::from("e"), Bytes::new()),
        ],
    );

    let mut iter = MergeIterator::create(vec![Box::new(i3), Box::new(i1), Box::new(i2)]);

    check_iter_result_by_key(
        &mut iter,
        vec![
            (Bytes::from("a"), Bytes::from("1.1")),
            (Bytes::from("b"), Bytes::from("2.3")),
            (Bytes::from("c"), Bytes::from("3.3")),
            (Bytes::from("d"), Bytes::from("4.3")),
            (Bytes::from("e"), Bytes::new()),
        ],
    );
}

#[test]
fn test_task2_merge_2() {
    let i1 = MockIterator::new(vec![
        (Bytes::from("a"), Bytes::from("1.1")),
        (Bytes::from("b"), Bytes::from("2.1")),
        (Bytes::from("c"), Bytes::from("3.1")),
    ]);
    let i2 = MockIterator::new(vec![
        (Bytes::from("d"), Bytes::from("1.2")),
        (Bytes::from("e"), Bytes::from("2.2")),
        (Bytes::from("f"), Bytes::from("3.2")),
        (Bytes::from("g"), Bytes::from("4.2")),
    ]);
    let i3 = MockIterator::new(vec![
        (Bytes::from("h"), Bytes::from("1.3")),
        (Bytes::from("i"), Bytes::from("2.3")),
        (Bytes::from("j"), Bytes::from("3.3")),
        (Bytes::from("k"), Bytes::from("4.3")),
    ]);
    let i4 = MockIterator::new(vec![]);
    let result = vec![
        (Bytes::from("a"), Bytes::from("1.1")),
        (Bytes::from("b"), Bytes::from("2.1")),
        (Bytes::from("c"), Bytes::from("3.1")),
        (Bytes::from("d"), Bytes::from("1.2")),
        (Bytes::from("e"), Bytes::from("2.2")),
        (Bytes::from("f"), Bytes::from("3.2")),
        (Bytes::from("g"), Bytes::from("4.2")),
        (Bytes::from("h"), Bytes::from("1.3")),
        (Bytes::from("i"), Bytes::from("2.3")),
        (Bytes::from("j"), Bytes::from("3.3")),
        (Bytes::from("k"), Bytes::from("4.3")),
    ];

    let mut iter = MergeIterator::create(vec![
        Box::new(i1.clone()),
        Box::new(i2.clone()),
        Box::new(i3.clone()),
        Box::new(i4.clone()),
    ]);
    check_iter_result_by_key(&mut iter, result.clone());

    let mut iter = MergeIterator::create(vec![
        Box::new(i2.clone()),
        Box::new(i4.clone()),
        Box::new(i3.clone()),
        Box::new(i1.clone()),
    ]);
    check_iter_result_by_key(&mut iter, result.clone());

    let mut iter =
        MergeIterator::create(vec![Box::new(i4), Box::new(i3), Box::new(i2), Box::new(i1)]);
    check_iter_result_by_key(&mut iter, result);
}

#[test]
fn test_task2_merge_empty() {
    let mut iter = MergeIterator::<MockIterator>::create(vec![]);
    check_iter_result_by_key(&mut iter, vec![]);

    let i1 = MockIterator::new(vec![
        (Bytes::from("a"), Bytes::from("1.1")),
        (Bytes::from("b"), Bytes::from("2.1")),
        (Bytes::from("c"), Bytes::from("3.1")),
    ]);
    let i2 = MockIterator::new(vec![]);
    let mut iter = MergeIterator::<MockIterator>::create(vec![Box::new(i1), Box::new(i2)]);
    check_iter_result_by_key(
        &mut iter,
        vec![
            (Bytes::from("a"), Bytes::from("1.1")),
            (Bytes::from("b"), Bytes::from("2.1")),
            (Bytes::from("c"), Bytes::from("3.1")),
        ],
    );
}

#[test]
fn test_task2_merge_error() {
    let mut iter = MergeIterator::<MockIterator>::create(vec![]);
    check_iter_result_by_key(&mut iter, vec![]);

    let i1 = MockIterator::new(vec![
        (Bytes::from("a"), Bytes::from("1.1")),
        (Bytes::from("b"), Bytes::from("2.1")),
        (Bytes::from("c"), Bytes::from("3.1")),
    ]);
    let i2 = MockIterator::new_with_error(
        vec![
            (Bytes::from("a"), Bytes::from("1.1")),
            (Bytes::from("b"), Bytes::from("2.1")),
            (Bytes::from("c"), Bytes::from("3.1")),
        ],
        1,
    );
    let iter = MergeIterator::<MockIterator>::create(vec![Box::new(i1), Box::new(i2)]);
    // your implementation should correctly throw an error instead of panic
    expect_iter_error(iter);
}

#[test]
fn test_task3_fused_iterator() {
    let iter = MockIterator::new(vec![]);
    let mut fused_iter = FusedIterator::new(iter);
    assert!(!fused_iter.is_valid());
    fused_iter.next().unwrap();
    fused_iter.next().unwrap();
    fused_iter.next().unwrap();
    assert!(!fused_iter.is_valid());

    let iter = MockIterator::new_with_error(
        vec![
            (Bytes::from("a"), Bytes::from("1.1")),
            (Bytes::from("a"), Bytes::from("1.1")),
        ],
        1,
    );
    let mut fused_iter = FusedIterator::new(iter);
    assert!(fused_iter.is_valid());
    assert!(fused_iter.next().is_err());
    assert!(!fused_iter.is_valid());
    assert!(fused_iter.next().is_err());
    assert!(fused_iter.next().is_err());
}

#[test]
fn test_task4_integration() {
    let dir = tempdir().unwrap();
    let storage = Arc::new(
        LsmStorageInner::open(dir.path(), LsmStorageOptions::default_for_week1_test()).unwrap(),
    );
    storage.put(b"1", b"233").unwrap();
    storage.put(b"2", b"2333").unwrap();
    storage.put(b"3", b"23333").unwrap();
    storage
        .force_freeze_memtable(&storage.state_lock.lock())
        .unwrap();
    storage.delete(b"1").unwrap();
    storage.delete(b"2").unwrap();
    storage.put(b"3", b"2333").unwrap();
    storage.put(b"4", b"23333").unwrap();
    storage
        .force_freeze_memtable(&storage.state_lock.lock())
        .unwrap();
    storage.put(b"1", b"233333").unwrap();
    storage.put(b"3", b"233333").unwrap();
    {
        let mut iter = storage.scan(Bound::Unbounded, Bound::Unbounded).unwrap();
        check_lsm_iter_result_by_key(
            &mut iter,
            vec![
                (Bytes::from_static(b"1"), Bytes::from_static(b"233333")),
                (Bytes::from_static(b"3"), Bytes::from_static(b"233333")),
                (Bytes::from_static(b"4"), Bytes::from_static(b"23333")),
            ],
        );
        assert!(!iter.is_valid());
        iter.next().unwrap();
        iter.next().unwrap();
        iter.next().unwrap();
        assert!(!iter.is_valid());
    }
    {
        let mut iter = storage
            .scan(Bound::Included(b"2"), Bound::Included(b"3"))
            .unwrap();
        check_lsm_iter_result_by_key(
            &mut iter,
            vec![(Bytes::from_static(b"3"), Bytes::from_static(b"233333"))],
        );
        assert!(!iter.is_valid());
        iter.next().unwrap();
        iter.next().unwrap();
        iter.next().unwrap();
        assert!(!iter.is_valid());
    }
}
