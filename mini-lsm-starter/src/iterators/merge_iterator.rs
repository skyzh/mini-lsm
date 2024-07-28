#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use std::cmp::{self};
use std::collections::binary_heap::PeekMut;
use std::collections::BinaryHeap;

use anyhow::Result;

use crate::key::KeySlice;

use super::StorageIterator;

// tuple struct that stores index of the memtable (implying latest (0) to oldest (n)), and the actual memtable storage iterator
struct HeapWrapper<I: StorageIterator>(pub usize, pub Box<I>);

impl<I: StorageIterator> PartialEq for HeapWrapper<I> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == cmp::Ordering::Equal
    }
}

impl<I: StorageIterator> Eq for HeapWrapper<I> {}

impl<I: StorageIterator> PartialOrd for HeapWrapper<I> {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

// The heap is ordered such that the iterator on the top of the heap is the one
// with the "smallest" current key. In LSM trees, we want to be able to process
// keys in an ascending order.
impl<I: StorageIterator> Ord for HeapWrapper<I> {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.1
            .key()
            .cmp(&other.1.key()) // primary ordering on the current key of the iterator
            .then(self.0.cmp(&other.0)) // secondary order by index or creation time of memtable
            // Rust binary heaps are maxHeaps by default. Calling reverse() here
            // makes our heap of iterators a minHeap.
            .reverse()
    }
}

/// Merge multiple iterators of the same type. If the same key occurs multiple times in some
/// iterators, prefer the one with smaller index.
pub struct MergeIterator<I: StorageIterator> {
    iters: BinaryHeap<HeapWrapper<I>>,
    current: Option<HeapWrapper<I>>,
}

impl<I: StorageIterator> MergeIterator<I> {
    pub fn create(iters: Vec<Box<I>>) -> Self {
        let mut merged_iters: BinaryHeap<HeapWrapper<I>> = BinaryHeap::new();

        if iters.is_empty() {
            return Self {
                iters: merged_iters,
                current: None,
            };
        };

        // handle case when all iterators are invalid
        if iters.iter().all(|x| !x.is_valid()) {
            // All invalid, select the last one as the current. We do this to main
            // consistency with the data structures. When we read or call next()
            // on this "current" iterator, it will be anyway marked as invalid.
            let mut iters = iters;
            return Self {
                iters: merged_iters,
                current: Some(HeapWrapper(0, iters.pop().unwrap())),
            };
        }

        for (index, iterator) in iters.into_iter().enumerate() {
            if iterator.is_valid() {
                merged_iters.push(HeapWrapper(index, iterator));
            }
        }

        let current = merged_iters.pop();
        Self {
            iters: merged_iters,
            current,
        }
    }
}

impl<I: 'static + for<'a> StorageIterator<KeyType<'a> = KeySlice<'a>>> StorageIterator
    for MergeIterator<I>
{
    type KeyType<'a> = KeySlice<'a>;

    fn key(&self) -> KeySlice {
        self.current.as_ref().unwrap().1.key()
    }

    fn value(&self) -> &[u8] {
        self.current.as_ref().unwrap().1.value()
    }

    fn is_valid(&self) -> bool {
        self.current
            .as_ref()
            .map(|i| i.1.is_valid())
            .unwrap_or(false)
    }

    fn next(&mut self) -> Result<()> {
        let current = self.current.as_mut().unwrap();

        // handle case for duplicate keys
        while let Some(mut inner_iter) = self.iters.peek_mut() {
            assert!(
                current.1.key() <= inner_iter.1.key(),
                "heap is in an unstable state"
            );

            if inner_iter.1.key() == current.1.key() {
                // the key is same so the merge iterator needs to ensure that we don't re-read the same key. Hence,
                // we advance `inner_iter` because it was consturctued _after_ current.
                let result = inner_iter.1.next();

                // Case 1: an error occurred when calling `next`. This should never happen but if next returned an
                // error that means we need to kick it out anyway.
                if let e @ Err(_) = result {
                    PeekMut::pop(inner_iter);
                    return e;
                }

                // Case 2: iter is no longer valid. We don't need this iterator anymore as it's not valid
                if !inner_iter.1.is_valid() {
                    PeekMut::pop(inner_iter);
                }
            } else {
                // key is different so our job for this iterator's next() is done
                break;
            }
        }

        // let's advance current now, any duplicate cases were handled above but we haven't
        // really advanced our current yet
        current.1.next()?;

        // If the current iterator is invalid, pop the next iterator out of the heap and reset current
        if !current.1.is_valid() {
            if let Some(iter) = self.iters.pop() {
                *current = iter;
            }
            return Ok(());
        }

        // Otherwise, compare with heap top and swap if necessary.
        if let Some(mut inner_iter) = self.iters.peek_mut() {
            // it's '<' here because `cmp` does the reverse().. in above cases `cmp()` gets implicitly
            // called which handles it but this is a manual comparison so have to remember
            if *current < *inner_iter {
                std::mem::swap(&mut *inner_iter, current);
            }
        }

        Ok(())
    }
}
