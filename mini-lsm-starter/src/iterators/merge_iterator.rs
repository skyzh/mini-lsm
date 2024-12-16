use anyhow::Result;
use std::cmp::{self};
use std::collections::binary_heap::PeekMut;
use std::collections::BinaryHeap;

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
        let heap_elements: Vec<HeapWrapper<I>> = iters
            .into_iter()
            .enumerate()
            .filter_map(|(index, iterator)| {
                if iterator.is_valid() {
                    Some(HeapWrapper {
                        0: index,
                        1: iterator,
                    })
                } else {
                    None
                }
            })
            .collect();

        if heap_elements.is_empty() || heap_elements.iter().all(|iterator| !iterator.1.is_valid()) {
            return MergeIterator {
                current: None,
                iters: BinaryHeap::new(),
            };
        }

        let mut heap = BinaryHeap::from(heap_elements);
        MergeIterator {
            current: heap.pop(),
            iters: heap,
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
            .map(|x| x.1.is_valid())
            .unwrap_or(false)
    }

    fn next(&mut self) -> Result<()> {
        let current = self.current.as_mut().unwrap();
        while let Some(mut inner) = self.iters.peek_mut() {
            if !inner.1.is_valid() {
                PeekMut::pop(inner);
            } else if current.1.key() == inner.1.key() {
                // Case 1: an error occurred when calling `next`.
                if let e @ Err(_) = inner.1.next() {
                    PeekMut::pop(inner);
                    return e;
                }
                if !inner.1.is_valid() {
                    PeekMut::pop(inner);
                }
            } else {
                break;
            }
        }

        current.1.next()?;

        if !current.1.is_valid() {
            if let Some(iter) = self.iters.pop() {
                *current = iter;
            }
            return Ok(());
        }

        if let Some(mut inner) = self.iters.peek_mut() {
            if *current < *inner {
                std::mem::swap(current, &mut *inner);
            }
        }
        Ok(())
    }
}
