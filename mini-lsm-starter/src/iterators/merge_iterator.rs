#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use std::cmp::{self};
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
// Doubtful if we can change the intermediate value in a structure since it is immutable

impl<I: StorageIterator> MergeIterator<I> {
    pub fn create(iters: Vec<Box<I>>) -> Self {
        if iters.is_empty() {
            return Self {
                iters: BinaryHeap::new(),
                current: None,
            };
        }

        let mut temp_iters = BinaryHeap::new();

        // No iterators are valid, so returning base case
        if iters.iter().all(|x| !x.is_valid()) {
            let mut iters = iters;
            let current_base = Some(HeapWrapper(0, iters.pop().unwrap()));
            return Self {
                iters: temp_iters,
                current: current_base,
            };
        }

        // Only load the iterators that are valid here
        let mut i = 0;
        for v in iters.into_iter() {
            if v.is_valid() {
                // println!("Here {:?}",v.value());
                temp_iters.push(HeapWrapper(i, v));
                i += 1
            }
        }

        let current_temp = temp_iters.pop().unwrap();
        MergeIterator {
            iters: temp_iters,
            current: Some(current_temp),
        }
    }
}

impl<I: 'static + for<'a> StorageIterator<KeyType<'a> = KeySlice<'a>>> StorageIterator
    for MergeIterator<I>
{
    type KeyType<'a> = KeySlice<'a>;

    fn key(&self) -> KeySlice {
        return self.current.as_ref().unwrap().1.key();
    }

    fn value(&self) -> &[u8] {
        return self.current.as_ref().unwrap().1.value();
    }

    fn is_valid(&self) -> bool {
        return self
            .current
            .as_ref()
            .map(|x| x.1.is_valid())
            .unwrap_or(false);
    }

    fn next(&mut self) -> Result<()> {
        // Iterate and go to the next element
        while !self.iters.is_empty()
            && (self.current.as_ref().unwrap().1.key() == self.iters.peek().unwrap().1.key())
        {
            let mut iter_dummy = self.iters.pop();
            if let Some(ref mut dummy) = iter_dummy {
                if dummy.1.is_valid() {
                    if let e @ Err(_) = dummy.1.next() {
                        return e;
                    }
                    if dummy.1.is_valid() {
                        self.iters
                            .push(iter_dummy.expect("This code should not execute"));
                    }
                }
            }
        }

        let current = self.current.as_mut().unwrap();
        current.1.next()?;

        if !current.1.is_valid() {
            if let Some(iter) = self.iters.pop() {
                *current = iter;
            }
            return Ok(());
        }

        if let Some(mut iter) = self.iters.peek_mut() {
            if *current < *iter {
                std::mem::swap(current, &mut iter)
            }
        }

        Ok(())
    }

    fn num_active_iterators(&self) -> usize {
        let mut total = 0;
        for i in self.iters.iter() {
            total += i.1.num_active_iterators();
        }
        total += self
            .current
            .as_ref()
            .map(|x| x.1.num_active_iterators())
            .unwrap_or(0);
        total
    }
}
