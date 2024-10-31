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
    iter_counter: usize,
}

impl<I: StorageIterator> MergeIterator<I> {
    pub fn create(iters: Vec<Box<I>>) -> Self {
        let mut merge_iter = MergeIterator::<I> {
            iters: BinaryHeap::new(),
            current: None,
            iter_counter: 0,
        };

        for (i, iter) in iters.into_iter().enumerate() {
            if iter.is_valid() {
                merge_iter.iter_counter += iter.num_active_iterators();
                merge_iter.iters.push(HeapWrapper(i, iter));
            }
        }

        merge_iter.current = merge_iter.iters.pop();

        merge_iter
    }
}

impl<I: 'static + for<'a> StorageIterator<KeyType<'a> = KeySlice<'a>>> StorageIterator
    for MergeIterator<I>
{
    type KeyType<'a> = KeySlice<'a>;

    fn key(&self) -> KeySlice {
        if let Some(ref item) = self.current {
            item.1.key()
        } else {
            KeySlice::default()
        }
    }

    fn value(&self) -> &[u8] {
        if let Some(ref item) = self.current {
            item.1.value()
        } else {
            &[]
        }
    }

    fn is_valid(&self) -> bool {
        if let Some(ref item) = self.current {
            item.1.is_valid()
        } else {
            false
        }
    }

    /// 当调用 next 时，可以假定 current 是有效的，并且 key 是最小的
    /// 调用 next 后，current 需要继续有效，并且 key 是最小的
    /// 先迭代 key 与 current 相同的 peek_item，直到 key 不同，然后迭代 current
    /// current 如果失效，则 pop 出 heap 中下一个元素，这样可以保证 iters 中所有元素都有效
    /// 否则，比较 current 和 peak_item，取更大的作为 current
    fn next(&mut self) -> Result<()> {
        let item = self.current.as_mut().unwrap();
        while let Some(mut peek_item) = self.iters.peek_mut() {
            if !peek_item.1.is_valid() || item.1.key() < peek_item.1.key() {
                break;
            }
            if item.1.key() == peek_item.1.key() {
                if let e @ Err(_) = peek_item.1.next() {
                    PeekMut::pop(peek_item);
                    return e;
                }
                if !peek_item.1.is_valid() {
                    PeekMut::pop(peek_item);
                }
            }
        }

        item.1.next()?;

        if !item.1.is_valid() {
            if let Some(new_item) = self.iters.pop() {
                *item = new_item;
            }
            return Ok(());
        }

        if let Some(mut top) = self.iters.peek_mut() {
            if item < &mut top {
                std::mem::swap(item, &mut top);
            }
        }

        Ok(())
    }

    fn num_active_iterators(&self) -> usize {
        self.iter_counter
    }
}
