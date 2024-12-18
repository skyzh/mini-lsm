use std::sync::Arc;

use anyhow::Result;

use super::StorageIterator;
use crate::{
    key::KeySlice,
    table::{SsTable, SsTableIterator},
};

/// Concat multiple iterators ordered in key order and their key ranges do not overlap. We do not want to create the
/// iterators when initializing this iterator to reduce the overhead of seeking.
pub struct SstConcatIterator {
    current: Option<SsTableIterator>,
    next_sst_idx: usize,
    sstables: Vec<Arc<SsTable>>,
}

/// 这里假定传入的 sstables 是有序的
impl SstConcatIterator {
    pub fn create_and_seek_to_first(sstables: Vec<Arc<SsTable>>) -> Result<Self> {
        let mut iter = Self {
            current: None,
            next_sst_idx: 0,
            sstables,
        };

        if !iter.sstables.is_empty() {
            iter.next()?;
        }

        Ok(iter)
    }

    pub fn create_and_seek_to_key(sstables: Vec<Arc<SsTable>>, key: KeySlice) -> Result<Self> {
        let mut iter = Self {
            current: None,
            next_sst_idx: 0,
            sstables,
        };

        let maybe_sst_index = iter
            .sstables
            .iter()
            .position(|sst| sst.last_key().raw_ref() >= key.raw_ref());

        match maybe_sst_index {
            Some(sst_index) => {
                iter.next_sst_idx = sst_index + 1;
                iter.current = Some(SsTableIterator::create_and_seek_to_key(
                    iter.sstables[sst_index].clone(),
                    key,
                )?);
            }
            None => {
                iter.next_sst_idx = iter.sstables.len();
            }
        }

        Ok(iter)
    }
}

impl StorageIterator for SstConcatIterator {
    type KeyType<'a> = KeySlice<'a>;

    fn key(&self) -> KeySlice {
        match self.current {
            Some(ref iter) => iter.key(),
            _ => KeySlice::from_slice(&[]),
        }
    }

    fn value(&self) -> &[u8] {
        match self.current {
            Some(ref iter) => iter.value(),
            _ => &[],
        }
    }

    fn is_valid(&self) -> bool {
        if let Some(ref iter) = self.current {
            iter.is_valid()
        } else {
            self.next_sst_idx < self.sstables.len()
        }
    }

    fn next(&mut self) -> Result<()> {
        let use_next_sst = match &self.current {
            Some(iter) => !iter.is_valid(),
            None => true,
        };

        if use_next_sst {
            self.current = Some(SsTableIterator::create_and_seek_to_first(
                self.sstables[self.next_sst_idx].clone(),
            )?);
            self.next_sst_idx += 1;
        } else if let Some(ref mut iter) = self.current {
            iter.next()?;
        }

        Ok(())
    }

    fn num_active_iterators(&self) -> usize {
        1
    }
}
