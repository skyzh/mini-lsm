use super::*;
use crate::iterators::two_merge_iterator::TwoMergeIterator;

fn check_iter_result(iter: impl StorageIterator, expected: Vec<(Bytes, Bytes)>) {
    let mut iter = iter;
    for (k, v) in expected {
        assert!(iter.is_valid());
        assert_eq!(iter.key(), k.as_ref());
        assert_eq!(iter.value(), v.as_ref());
        iter.next().unwrap();
    }
    assert!(!iter.is_valid());
}

#[test]
fn test_merge_1() {
    let i1 = MockIterator::new(vec![
        (Bytes::from("a"), Bytes::from("1.1")),
        (Bytes::from("b"), Bytes::from("2.1")),
        (Bytes::from("c"), Bytes::from("3.1")),
    ]);
    let i2 = MockIterator::new(vec![
        (Bytes::from("a"), Bytes::from("1.2")),
        (Bytes::from("b"), Bytes::from("2.2")),
        (Bytes::from("c"), Bytes::from("3.2")),
        (Bytes::from("d"), Bytes::from("4.2")),
    ]);
    let iter = TwoMergeIterator::create(i1, i2).unwrap();
    check_iter_result(
        iter,
        vec![
            (Bytes::from("a"), Bytes::from("1.1")),
            (Bytes::from("b"), Bytes::from("2.1")),
            (Bytes::from("c"), Bytes::from("3.1")),
            (Bytes::from("d"), Bytes::from("4.2")),
        ],
    )
}

#[test]
fn test_merge_2() {
    let i2 = MockIterator::new(vec![
        (Bytes::from("a"), Bytes::from("1.1")),
        (Bytes::from("b"), Bytes::from("2.1")),
        (Bytes::from("c"), Bytes::from("3.1")),
    ]);
    let i1 = MockIterator::new(vec![
        (Bytes::from("a"), Bytes::from("1.2")),
        (Bytes::from("b"), Bytes::from("2.2")),
        (Bytes::from("c"), Bytes::from("3.2")),
        (Bytes::from("d"), Bytes::from("4.2")),
    ]);
    let iter = TwoMergeIterator::create(i1, i2).unwrap();
    check_iter_result(
        iter,
        vec![
            (Bytes::from("a"), Bytes::from("1.2")),
            (Bytes::from("b"), Bytes::from("2.2")),
            (Bytes::from("c"), Bytes::from("3.2")),
            (Bytes::from("d"), Bytes::from("4.2")),
        ],
    )
}

#[test]
fn test_merge_3() {
    let i2 = MockIterator::new(vec![
        (Bytes::from("a"), Bytes::from("1.1")),
        (Bytes::from("b"), Bytes::from("2.1")),
        (Bytes::from("c"), Bytes::from("3.1")),
    ]);
    let i1 = MockIterator::new(vec![
        (Bytes::from("b"), Bytes::from("2.2")),
        (Bytes::from("c"), Bytes::from("3.2")),
        (Bytes::from("d"), Bytes::from("4.2")),
    ]);
    let iter = TwoMergeIterator::create(i1, i2).unwrap();
    check_iter_result(
        iter,
        vec![
            (Bytes::from("a"), Bytes::from("1.1")),
            (Bytes::from("b"), Bytes::from("2.2")),
            (Bytes::from("c"), Bytes::from("3.2")),
            (Bytes::from("d"), Bytes::from("4.2")),
        ],
    )
}

#[test]
fn test_merge_4() {
    let i2 = MockIterator::new(vec![]);
    let i1 = MockIterator::new(vec![
        (Bytes::from("b"), Bytes::from("2.2")),
        (Bytes::from("c"), Bytes::from("3.2")),
        (Bytes::from("d"), Bytes::from("4.2")),
    ]);
    let iter = TwoMergeIterator::create(i1, i2).unwrap();
    check_iter_result(
        iter,
        vec![
            (Bytes::from("b"), Bytes::from("2.2")),
            (Bytes::from("c"), Bytes::from("3.2")),
            (Bytes::from("d"), Bytes::from("4.2")),
        ],
    );
    let i1 = MockIterator::new(vec![]);
    let i2 = MockIterator::new(vec![
        (Bytes::from("b"), Bytes::from("2.2")),
        (Bytes::from("c"), Bytes::from("3.2")),
        (Bytes::from("d"), Bytes::from("4.2")),
    ]);
    let iter = TwoMergeIterator::create(i1, i2).unwrap();
    check_iter_result(
        iter,
        vec![
            (Bytes::from("b"), Bytes::from("2.2")),
            (Bytes::from("c"), Bytes::from("3.2")),
            (Bytes::from("d"), Bytes::from("4.2")),
        ],
    );
}

#[test]
fn test_merge_5() {
    let i2 = MockIterator::new(vec![]);
    let i1 = MockIterator::new(vec![]);
    let iter = TwoMergeIterator::create(i1, i2).unwrap();
    check_iter_result(iter, vec![])
}
