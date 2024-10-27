#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use std::cmp::{self};
use std::collections::binary_heap::PeekMut;
use std::collections::BinaryHeap;

use anyhow::Result;

use crate::key::KeySlice;

use super::StorageIterator;

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
        let mut idx = 0;
        // iters is Vec<StorageIterator<I>>
        for iter in iters {
            if iter.is_valid() {
                heap.push(HeapWrapper(idx, iter));
                idx += 1;
            }
        }

        let curr = heap.pop();
        Self {
            iters: heap,
            current: curr,
        }
    }
}

impl<I: 'static + for<'a> StorageIterator<KeyType<'a> = KeySlice<'a>>> StorageIterator
    for MergeIterator<I>
{
    type KeyType<'a> = KeySlice<'a>;

    fn value(&self) -> &[u8] {
        self.current.as_ref().unwrap().1.value()
    }

    fn key(&self) -> KeySlice {
        self.current.as_ref().unwrap().1.key()
    }

    fn is_valid(&self) -> bool {
        // It should be None once all the iterators are done.
        // map should ignore none
        self.current
            .as_ref()
            .map(|top| top.1.is_valid())
            .unwrap_or(false)
    }

    fn next(&mut self) -> Result<()> {
        // first check if the current has same values as the heap top, if yes? pop away / move next on the heap tops.
        let mut curr = self.current.as_mut().unwrap();
        while let Some(mut top) = self.iters.peek_mut() {
            if (curr.1.key()) == top.1.key() {
                if let err @ Err(_) = top.1.next() {
                    PeekMut::pop(top);
                    return err;
                }
                if !top.1.is_valid() {
                    PeekMut::pop(top);
                }
            } else {
                break;
            }
        }

        // now attempt performing next on current.
        curr.1.next()?;

        if !curr.1.is_valid() {
            if let Some(top) = self.iters.pop() {
                *curr = top;
            }
            return Ok(());
        }

        // maintain order between top of heap and current.
        if let Some(mut top) = self.iters.peek_mut() {
            if *curr < *top {
                std::mem::swap(&mut *curr, &mut *top);
            }
        }

        Ok(())
    }
}
