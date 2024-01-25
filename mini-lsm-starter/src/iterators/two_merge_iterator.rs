#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use anyhow::Result;

use crate::key::KeySlice;

use super::StorageIterator;

/// Merges two iterators of different types into one. If the two iterators have the same key, only
/// produce the key once and prefer the entry from A.
pub struct TwoMergeIterator<A: StorageIterator, B: StorageIterator> {
    a: A,
    b: B,
    // Add fields as need
}

impl<
        A: 'static + for<'a> StorageIterator<KeyType<'a> = KeySlice<'a>>,
        B: 'static + for<'a> StorageIterator<KeyType<'a> = KeySlice<'a>>,
    > TwoMergeIterator<A, B>
{
    pub fn create(a: A, b: B) -> Result<Self> {
        unimplemented!()
    }
}

impl<
        A: 'static + for<'a> StorageIterator<KeyType<'a> = KeySlice<'a>>,
        B: 'static + for<'a> StorageIterator<KeyType<'a> = KeySlice<'a>>,
    > StorageIterator for TwoMergeIterator<A, B>
{
    type KeyType<'a> = KeySlice<'a>;

    fn key(&self) -> KeySlice {
        unimplemented!()
    }

    fn value(&self) -> &[u8] {
        unimplemented!()
    }

    fn is_valid(&self) -> bool {
        unimplemented!()
    }

    fn next(&mut self) -> Result<()> {
        unimplemented!()
    }
}
