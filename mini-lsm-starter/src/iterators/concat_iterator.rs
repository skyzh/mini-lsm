use std::cmp::{self};
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

impl SstConcatIterator {
    pub fn create_and_seek_to_first(sstables: Vec<Arc<SsTable>>) -> Result<Self> {
        let mut ssconcatirer = SstConcatIterator {
            current: Option::None,
            next_sst_idx: 0,
            sstables,
        };
        if ssconcatirer.sstables.len() > 0 {
            ssconcatirer.current = Some(SsTableIterator::create_and_seek_to_first(
                ssconcatirer.sstables[0].clone(),
            )?);
            ssconcatirer.next_sst_idx = 1;
        }
        Ok(ssconcatirer)
    }

    pub fn create_and_seek_to_key(sstables: Vec<Arc<SsTable>>, key: KeySlice) -> Result<Self> {
        let mut ssconcatirer = SstConcatIterator {
            current: Option::None,
            next_sst_idx: 0,
            sstables,
        };
        // traverse sstables until find the sstable that contains the key
        for i in 0..ssconcatirer.sstables.len() {
            let cmp_res = ssconcatirer.sstables[i]
                .last_key()
                .partial_cmp(&key.to_key_vec().into_key_bytes());
            if cmp_res == Some(cmp::Ordering::Greater) || cmp_res == Some(cmp::Ordering::Equal) {
                ssconcatirer.current = Some(SsTableIterator::create_and_seek_to_key(
                    ssconcatirer.sstables[i].clone(),
                    key,
                )?);
                ssconcatirer.next_sst_idx = i + 1;
                break;
            }
        }
        Ok(ssconcatirer)
    }
}

impl StorageIterator for SstConcatIterator {
    type KeyType<'a> = KeySlice<'a>;

    fn key(&self) -> KeySlice {
        assert!(self.is_valid());
        self.current.as_ref().unwrap().key()
    }

    fn value(&self) -> &[u8] {
        assert!(self.is_valid());
        self.current.as_ref().unwrap().value()
    }

    fn is_valid(&self) -> bool {
        self.current.is_some()
    }

    fn next(&mut self) -> Result<()> {
        assert!(self.is_valid());
        // 1.current move to next
        self.current.as_mut().unwrap().next()?;
        // 2.current after next be invalid, move to next sstable
        if !self.current.as_ref().unwrap().is_valid() {
            if self.next_sst_idx < self.sstables.len() {
                self.current = Some(SsTableIterator::create_and_seek_to_first(
                    self.sstables[self.next_sst_idx].clone(),
                )?);
                self.next_sst_idx += 1;
            } else {
                self.current = None;
            }
        }
        Ok(())
    }

    fn num_active_iterators(&self) -> usize {
        1
    }
}
