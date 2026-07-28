#![allow(dead_code)] // REMOVE THIS LINE once all modules are complete

// Copyright (c) 2022-2025 Alex Chi Z
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{
    collections::BTreeMap, ops::Bound, os::unix::fs::MetadataExt, path::Path, sync::Arc,
    time::Duration,
};

use anyhow::{Result, bail};
use bytes::Bytes;

use crate::{
    compact::{
        CompactionOptions, LeveledCompactionOptions, SimpleLeveledCompactionOptions,
        TieredCompactionOptions,
    },
    iterators::{StorageIterator, merge_iterator::MergeIterator},
    key::{KeySlice, TS_ENABLED},
    lsm_storage::{BlockCache, LsmStorageInner, LsmStorageState, MiniLsm},
    table::{SsTable, SsTableBuilder, SsTableIterator},
};

#[derive(Clone)]
pub struct MockIterator {
    pub data: Vec<(Bytes, Bytes)>,
    pub error_when: Option<usize>,
    pub index: usize,
}

impl MockIterator {
    pub fn new(data: Vec<(Bytes, Bytes)>) -> Self {
        Self {
            data,
            index: 0,
            error_when: None,
        }
    }

    pub fn new_with_error(data: Vec<(Bytes, Bytes)>, error_when: usize) -> Self {
        Self {
            data,
            index: 0,
            error_when: Some(error_when),
        }
    }
}

impl StorageIterator for MockIterator {
    type KeyType<'a> = KeySlice<'a>;

    fn next(&mut self) -> Result<()> {
        if self.index < self.data.len() {
            self.index += 1;
        }
        if let Some(error_when) = self.error_when
            && self.index == error_when
        {
            bail!("fake error!");
        }
        Ok(())
    }

    fn key(&self) -> KeySlice<'_> {
        if let Some(error_when) = self.error_when
            && self.index >= error_when
        {
            panic!("invalid access after next returns an error!");
        }
        KeySlice::for_testing_from_slice_no_ts(self.data[self.index].0.as_ref())
    }

    fn value(&self) -> &[u8] {
        if let Some(error_when) = self.error_when
            && self.index >= error_when
        {
            panic!("invalid access after next returns an error!");
        }
        self.data[self.index].1.as_ref()
    }

    fn is_valid(&self) -> bool {
        if let Some(error_when) = self.error_when
            && self.index >= error_when
        {
            panic!("invalid access after next returns an error!");
        }
        self.index < self.data.len()
    }
}

pub fn as_bytes(x: &[u8]) -> Bytes {
    Bytes::copy_from_slice(x)
}

pub fn check_iter_result_by_key<I>(iter: &mut I, expected: Vec<(Bytes, Bytes)>)
where
    I: for<'a> StorageIterator<KeyType<'a> = KeySlice<'a>>,
{
    for (k, v) in expected {
        assert!(iter.is_valid());
        assert_eq!(
            k,
            iter.key().for_testing_key_ref(),
            "expected key: {:?}, actual key: {:?}",
            k,
            as_bytes(iter.key().for_testing_key_ref()),
        );
        assert_eq!(
            v,
            iter.value(),
            "expected value: {:?}, actual value: {:?}",
            v,
            as_bytes(iter.value()),
        );
        iter.next().unwrap();
    }
    assert!(
        !iter.is_valid(),
        "iterator should not be valid at the end of the check"
    );
}

pub fn check_iter_result_by_key_and_ts<I>(iter: &mut I, expected: Vec<((Bytes, u64), Bytes)>)
where
    I: for<'a> StorageIterator<KeyType<'a> = KeySlice<'a>>,
{
    for ((k, ts), v) in expected {
        assert!(iter.is_valid());
        assert_eq!(
            (&k[..], ts),
            (
                iter.key().for_testing_key_ref(),
                iter.key().for_testing_ts()
            ),
            "expected key: {:?}@{}, actual key: {:?}@{}",
            k,
            ts,
            as_bytes(iter.key().for_testing_key_ref()),
            iter.key().for_testing_ts(),
        );
        assert_eq!(
            v,
            iter.value(),
            "expected value: {:?}, actual value: {:?}",
            v,
            as_bytes(iter.value()),
        );
        iter.next().unwrap();
    }
    assert!(!iter.is_valid());
}

pub fn check_lsm_iter_result_by_key<I>(iter: &mut I, expected: Vec<(Bytes, Bytes)>)
where
    I: for<'a> StorageIterator<KeyType<'a> = &'a [u8]>,
{
    for (k, v) in expected {
        assert!(iter.is_valid());
        assert_eq!(
            k,
            iter.key(),
            "expected key: {:?}, actual key: {:?}",
            k,
            as_bytes(iter.key()),
        );
        assert_eq!(
            v,
            iter.value(),
            "expected value: {:?}, actual value: {:?}",
            v,
            as_bytes(iter.value()),
        );
        iter.next().unwrap();
    }
    assert!(!iter.is_valid());
}

pub fn expect_iter_error(mut iter: impl StorageIterator) {
    loop {
        match iter.next() {
            Ok(_) if iter.is_valid() => continue,
            Ok(_) => panic!("expect an error"),
            Err(_) => break,
        }
    }
}

pub fn generate_sst(
    id: usize,
    path: impl AsRef<Path>,
    data: Vec<(Bytes, Bytes)>,
    block_cache: Option<Arc<BlockCache>>,
) -> SsTable {
    let mut builder = SsTableBuilder::new(128);
    for (key, value) in data {
        builder.add(KeySlice::for_testing_from_slice_no_ts(&key[..]), &value[..]);
    }
    builder.build(id, block_cache, path.as_ref()).unwrap()
}

pub fn generate_sst_with_ts(
    id: usize,
    path: impl AsRef<Path>,
    data: Vec<((Bytes, u64), Bytes)>,
    block_cache: Option<Arc<BlockCache>>,
) -> SsTable {
    let mut builder = SsTableBuilder::new(128);
    for ((key, ts), value) in data {
        builder.add(
            KeySlice::for_testing_from_slice_with_ts(&key[..], ts),
            &value[..],
        );
    }
    builder.build(id, block_cache, path.as_ref()).unwrap()
}

pub fn sync(storage: &LsmStorageInner) {
    storage
        .force_freeze_memtable(&storage.state_lock.lock())
        .unwrap();
    storage.force_flush_next_imm_memtable().unwrap();
}

pub fn compaction_bench(storage: Arc<MiniLsm>) {
    let mut key_map = BTreeMap::<usize, usize>::new();
    let gen_key = |i| format!("{:010}", i); // 10B
    let gen_value = |i| format!("{:0110}", i); // 110B
    let mut max_key = 0;
    let overlaps = if TS_ENABLED { 10000 } else { 20000 };
    for iter in 0..10 {
        let range_begin = iter * 5000;
        for i in range_begin..(range_begin + overlaps) {
            // 120B per key, 4MB data populated
            let key: String = gen_key(i);
            let version = key_map.get(&i).copied().unwrap_or_default() + 1;
            let value = gen_value(version);
            key_map.insert(i, version);
            storage.put(key.as_bytes(), value.as_bytes()).unwrap();
            max_key = max_key.max(i);
        }
    }

    std::thread::sleep(Duration::from_secs(1)); // wait until all memtables flush
    while {
        let snapshot = storage.inner.state.read();
        !snapshot.imm_memtables.is_empty()
    } {
        storage.inner.force_flush_next_imm_memtable().unwrap();
    }

    let mut prev_snapshot = storage.inner.state.read().clone();
    while {
        std::thread::sleep(Duration::from_secs(1));
        let snapshot = storage.inner.state.read().clone();
        let to_cont = prev_snapshot.levels != snapshot.levels
            || prev_snapshot.l0_sstables != snapshot.l0_sstables;
        prev_snapshot = snapshot;
        to_cont
    } {
        println!("waiting for compaction to converge");
    }

    let mut expected_key_value_pairs = Vec::new();
    for i in 0..(max_key + 40000) {
        let key = gen_key(i);
        let value = storage.get(key.as_bytes()).unwrap();
        if let Some(val) = key_map.get(&i) {
            let expected_value = gen_value(*val);
            assert_eq!(value, Some(Bytes::from(expected_value.clone())));
            expected_key_value_pairs.push((Bytes::from(key), Bytes::from(expected_value)));
        } else {
            assert!(value.is_none());
        }
    }

    check_lsm_iter_result_by_key(
        &mut storage.scan(Bound::Unbounded, Bound::Unbounded).unwrap(),
        expected_key_value_pairs,
    );

    storage.dump_structure();

    println!(
        "This test case does not guarantee your compaction algorithm produces a LSM state as expected. It only does minimal checks on the size of the levels. Please use the compaction simulator to check if the compaction is correctly going on."
    );
}

pub fn check_compaction_ratio(storage: Arc<MiniLsm>) {
    let state = storage.inner.state.read().clone();
    let compaction_options = storage.inner.options.compaction_options.clone();
    let mut level_size = Vec::new();
    let l0_sst_num = state.l0_sstables.len();
    for (_, files) in &state.levels {
        let size = match &compaction_options {
            CompactionOptions::Leveled(_) => files
                .iter()
                .map(|x| state.sstables.get(x).as_ref().unwrap().table_size())
                .sum::<u64>(),
            CompactionOptions::Simple(_) | CompactionOptions::Tiered(_) => files.len() as u64,
            _ => unreachable!(),
        };
        level_size.push(size);
    }
    let extra_iterators = if TS_ENABLED {
        1 /* txn local iterator for OCC */
    } else {
        0
    };
    let num_iters = storage
        .scan(Bound::Unbounded, Bound::Unbounded)
        .unwrap()
        .num_active_iterators();
    let num_memtables = storage.inner.state.read().imm_memtables.len() + 1;
    match compaction_options {
        CompactionOptions::NoCompaction => unreachable!(),
        CompactionOptions::Simple(SimpleLeveledCompactionOptions {
            size_ratio_percent,
            level0_file_num_compaction_trigger,
            max_levels,
        }) => {
            assert!(l0_sst_num < level0_file_num_compaction_trigger);
            assert!(level_size.len() <= max_levels);
            for idx in 1..level_size.len() {
                let prev_size = level_size[idx - 1];
                let this_size = level_size[idx];
                if prev_size == 0 && this_size == 0 {
                    continue;
                }
                assert!(
                    this_size as f64 / prev_size as f64 >= size_ratio_percent as f64 / 100.0,
                    "L{}/L{}, {}/{}<{}%",
                    state.levels[idx - 1].0,
                    state.levels[idx].0,
                    this_size,
                    prev_size,
                    size_ratio_percent
                );
            }
            assert!(
                num_iters <= l0_sst_num + num_memtables + max_levels + extra_iterators,
                "we found {num_iters} iterators in your implementation, (l0_sst_num={l0_sst_num}, num_memtables={num_memtables}, max_levels={max_levels}) did you use concat iterators?"
            );
        }
        CompactionOptions::Leveled(LeveledCompactionOptions {
            level_size_multiplier,
            level0_file_num_compaction_trigger,
            max_levels,
            ..
        }) => {
            assert!(l0_sst_num < level0_file_num_compaction_trigger);
            assert!(level_size.len() <= max_levels);
            let last_level_size = *level_size.last().unwrap();
            let mut multiplier = 1.0;
            for idx in (1..level_size.len()).rev() {
                multiplier *= level_size_multiplier as f64;
                let this_size = level_size[idx - 1];
                assert!(
                    // do not add hard requirement on level size multiplier considering bloom filters...
                    this_size as f64 / last_level_size as f64 <= 1.0 / multiplier + 0.5,
                    "L{}/L_max, {}/{}>>1.0/{}",
                    state.levels[idx - 1].0,
                    this_size,
                    last_level_size,
                    multiplier
                );
            }
            assert!(
                num_iters <= l0_sst_num + num_memtables + max_levels + extra_iterators,
                "we found {num_iters} iterators in your implementation, (l0_sst_num={l0_sst_num}, num_memtables={num_memtables}, max_levels={max_levels}) did you use concat iterators?"
            );
        }
        CompactionOptions::Tiered(TieredCompactionOptions {
            num_tiers,
            max_size_amplification_percent,
            size_ratio,
            min_merge_width,
            ..
        }) => {
            let size_ratio_trigger = (100.0 + size_ratio as f64) / 100.0;
            assert_eq!(l0_sst_num, 0);
            assert!(level_size.len() <= num_tiers);
            let mut sum_size = level_size[0];
            for idx in 1..level_size.len() {
                let this_size = level_size[idx];
                if level_size.len() > min_merge_width {
                    assert!(
                        sum_size as f64 / this_size as f64 <= size_ratio_trigger,
                        "violation of size ratio: sum(⬆️L{})/L{}, {}/{}>{}",
                        state.levels[idx - 1].0,
                        state.levels[idx].0,
                        sum_size,
                        this_size,
                        size_ratio_trigger
                    );
                }
                if idx + 1 == level_size.len() {
                    assert!(
                        sum_size as f64 / this_size as f64
                            <= max_size_amplification_percent as f64 / 100.0,
                        "violation of space amp: sum(⬆️L{})/L{}, {}/{}>{}%",
                        state.levels[idx - 1].0,
                        state.levels[idx].0,
                        sum_size,
                        this_size,
                        max_size_amplification_percent
                    );
                }
                sum_size += this_size;
            }
            assert!(
                num_iters <= num_memtables + num_tiers + extra_iterators,
                "we found {num_iters} iterators in your implementation, (num_memtables={num_memtables}, num_tiers={num_tiers}) did you use concat iterators?"
            );
        }
    }
}

pub fn dump_files_in_dir(path: impl AsRef<Path>) {
    println!("--- DIR DUMP ---");
    for f in path.as_ref().read_dir().unwrap() {
        let f = f.unwrap();
        print!("{}", f.path().display());
        println!(
            ", size={:.3}KB",
            f.metadata().unwrap().size() as f64 / 1024.0
        );
    }
}

pub fn construct_merge_iterator_over_storage(
    state: &LsmStorageState,
) -> MergeIterator<SsTableIterator> {
    let mut iters = Vec::new();
    for t in &state.l0_sstables {
        iters.push(Box::new(
            SsTableIterator::create_and_seek_to_first(state.sstables.get(t).cloned().unwrap())
                .unwrap(),
        ));
    }
    for (_, files) in &state.levels {
        for f in files {
            iters.push(Box::new(
                SsTableIterator::create_and_seek_to_first(state.sstables.get(f).cloned().unwrap())
                    .unwrap(),
            ));
        }
    }
    MergeIterator::create(iters)
}
