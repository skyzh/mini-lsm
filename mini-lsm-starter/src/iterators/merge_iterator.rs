#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use std::cmp::{self};
use std::collections::BinaryHeap;
use std::collections::binary_heap::PeekMut;

use anyhow::{Ok, Result};

use crate::key::KeySlice;

use super::StorageIterator;

struct HeapWrapper<I: StorageIterator>(pub usize, pub Box<I>);

impl<I: StorageIterator> PartialEq for HeapWrapper<I> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == cmp::Ordering::Equal
    }
}

impl<I: StorageIterator> Eq for HeapWrapper<I> {}

// min heap, the smaller the key, the higher the priority, if keys are equal, the smaller the index is, the higher the priority
impl<I: StorageIterator> PartialOrd for HeapWrapper<I> {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<I: StorageIterator> Ord for HeapWrapper<I> {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.1
            .key()
            .cmp(&other.1.key())
            .then(self.0.cmp(&other.0))
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
        let mut heap = BinaryHeap::new();
        for (idx, iter) in iters.into_iter().enumerate() {
            if iter.is_valid() {
                heap.push(HeapWrapper(idx, iter));
            }
        }

        let current = heap.pop();
        Self {
            iters: heap,
            current,
        }
    }
}

impl<I: 'static + for<'a> StorageIterator<KeyType<'a> = KeySlice<'a>>> StorageIterator
    for MergeIterator<I>
{
    type KeyType<'a> = KeySlice<'a>;

    fn key(&'_ self) -> KeySlice<'_> {
        self.current.as_ref().unwrap().1.key()
    }

    fn value(&self) -> &[u8] {
        self.current.as_ref().unwrap().1.value()
    }

    fn is_valid(&self) -> bool {
        self.current.is_some() && self.current.as_ref().unwrap().1.is_valid()
    }

    fn next(&mut self) -> Result<()> {
        let current = self.current.as_mut().unwrap();

        while let Some(mut inner_iter) = self.iters.peek_mut() {
            if inner_iter.1.key() == current.1.key() {
                // drop PeekMut will call self.heap.sift_down(0), make 0 reordered, https://doc.rust-lang.org/src/alloc/collections/binary_heap/mod.rs.html#321,
                match inner_iter.1.next() {
                    anyhow::Result::Ok(_) => {
                        if !inner_iter.1.is_valid() {
                            PeekMut::pop(inner_iter);
                        }
                    }
                    // https://doc.rust-lang.org/reference/patterns.html#identifier-patterns
                    e @ Err(_) => {
                        PeekMut::pop(inner_iter);
                        return e;
                    }
                }
            } else {
                break;
            }
        }

        current.1.next()?;

        if !current.1.is_valid() {
            if let Some(c) = self.iters.pop() {
                *current = c;
            }
            return Ok(());
        }

        if let Some(mut n) = self.iters.peek_mut() {
            // in reverse order, so use > instead of <
            if *n > *current {
                // drop PeekMut will call self.heap.sift_down(0), make 0 reordered, https://doc.rust-lang.org/src/alloc/collections/binary_heap/mod.rs.html#321,
                std::mem::swap(current, &mut n);
            }
        }

        Ok(())
    }

    fn num_active_iterators(&self) -> usize {
        self.iters.len() + 1
    }
}
