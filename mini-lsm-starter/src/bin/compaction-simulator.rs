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

mod wrapper;
use wrapper::mini_lsm_wrapper;

use std::collections::HashMap;
use std::sync::Arc;

use bytes::{Buf, BufMut, BytesMut};
use clap::Parser;
use mini_lsm_wrapper::compact::{
    LeveledCompactionController, LeveledCompactionOptions, SimpleLeveledCompactionController,
    SimpleLeveledCompactionOptions, TieredCompactionController, TieredCompactionOptions,
};
use mini_lsm_wrapper::key::KeyBytes;
use mini_lsm_wrapper::lsm_storage::LsmStorageState;
use mini_lsm_wrapper::mem_table::MemTable;
use mini_lsm_wrapper::table::SsTable;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
enum Args {
    Simple {
        /// Dump the generated ID instead of where the original data comes from.
        /// For example, if SST 1, 2, 3 is compacted to another level, it should have
        /// a new SST ID 4, 5, 6 as SSTs are immutable and write-once. With this flag
        /// enabled, you will see the new level has SST 1, 2, 3 because the data of
        /// 4, 5, 6 are originated from 1, 2, 3.
        #[clap(long)]
        dump_real_id: bool,
        /// Only dump size information instead of the layer files. if this is enabled,
        /// it will print one row per compaction iteration.
        #[clap(long)]
        size_only: bool,
        #[clap(long, default_value = "2")]
        level0_file_num_compaction_trigger: usize,
        #[clap(long, default_value = "3")]
        max_levels: usize,
        #[clap(long, default_value = "200")]
        size_ratio_percent: usize,
        #[clap(long, default_value = "50")]
        iterations: usize,
    },
    Tiered {
        /// Dump the generated ID instead of where the original data comes from.
        /// For example, if SST 1, 2, 3 is compacted to another level, it should have
        /// a new SST ID 4, 5, 6 as SSTs are immutable and write-once. With this flag
        /// enabled, you will see the new level has SST 1, 2, 3 because the data of
        /// 4, 5, 6 are originated from 1, 2, 3.
        #[clap(long)]
        dump_real_id: bool,
        /// Only dump size information instead of the layer files. if this is enabled,
        /// it will print one row per compaction iteration.
        #[clap(long)]
        size_only: bool,
        #[clap(long, default_value = "8")]
        num_tiers: usize,
        #[clap(long, default_value = "200")]
        max_size_amplification_percent: usize,
        #[clap(long, default_value = "1")]
        size_ratio: usize,
        #[clap(long, default_value = "2")]
        min_merge_width: usize,
        #[clap(long)]
        max_merge_width: Option<usize>,
        #[clap(long, default_value = "50")]
        iterations: usize,
    },
    Leveled {
        /// Dump the generated ID instead of where the original data comes from.
        /// For example, if SST 1, 2, 3 is compacted to another level, it should have
        /// a new SST ID 4, 5, 6 as SSTs are immutable and write-once. With this flag
        /// enabled, you will see the new level has SST 1, 2, 3 because the data of
        /// 4, 5, 6 are originated from 1, 2, 3.
        #[clap(long)]
        dump_real_id: bool,
        /// Only dump size information instead of the layer files. if this is enabled,
        /// it will print one row per compaction iteration.
        #[clap(long)]
        size_only: bool,
        #[clap(long, default_value = "2")]
        level0_file_num_compaction_trigger: usize,
        #[clap(long, default_value = "2")]
        level_size_multiplier: usize,
        #[clap(long, default_value = "4")]
        max_levels: usize,
        #[clap(long, default_value = "128")]
        base_level_size_mb: usize,
        #[clap(long, default_value = "50")]
        iterations: usize,
        #[clap(long, default_value = "32")]
        sst_size_mb: usize,
    },
}

pub struct MockStorage {
    snapshot: LsmStorageState,
    next_sst_id: usize,
    /// Maps SST ID to the original flushed SST ID
    file_list: HashMap<usize, usize>,
    total_flushes: usize,
    total_writes: usize,
}

impl Default for MockStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl MockStorage {
    pub fn new() -> Self {
        let snapshot = LsmStorageState {
            memtable: Arc::new(MemTable::create(0)),
            imm_memtables: Vec::new(),
            l0_sstables: Vec::new(),
            levels: Vec::new(),
            sstables: Default::default(),
        };
        Self {
            snapshot,
            next_sst_id: 1,
            file_list: Default::default(),
            total_flushes: 0,
            total_writes: 0,
        }
    }

    fn generate_sst_id(&mut self) -> usize {
        let id = self.next_sst_id;
        self.next_sst_id += 1;
        id
    }

    pub fn flush_sst_to_l0(&mut self) -> usize {
        let id = self.generate_sst_id();
        self.snapshot.l0_sstables.push(id);
        self.file_list.insert(id, id);
        self.total_flushes += 1;
        self.total_writes += 1;
        id
    }

    pub fn flush_sst_to_new_tier(&mut self) {
        let id = self.generate_sst_id();
        self.snapshot.levels.insert(0, (id, vec![id]));
        self.file_list.insert(id, id);
        self.total_flushes += 1;
        self.total_writes += 1;
    }

    pub fn remove(&mut self, files_to_remove: &[usize]) {
        for file_id in files_to_remove {
            let ret = self.file_list.remove(file_id);
            assert!(ret.is_some(), "failed to remove file {}", file_id);
        }
    }

    fn check_keys(&self) {
        for (level, files) in &self.snapshot.levels {
            if files.len() >= 2 {
                for id in 0..(files.len() - 1) {
                    let this_file = self.snapshot.sstables[&files[id]].clone();
                    let next_file = self.snapshot.sstables[&files[id + 1]].clone();
                    if this_file.last_key() >= next_file.first_key() {
                        panic!(
                            "invalid file arrangement in L{}: id={}, range={:x}..={:x}; id={}, range={:x}..={:x}",
                            level,
                            this_file.sst_id(),
                            this_file.first_key().for_testing_key_ref().get_u64(),
                            this_file.last_key().for_testing_key_ref().get_u64(),
                            next_file.sst_id(),
                            next_file.first_key().for_testing_key_ref().get_u64(),
                            next_file.last_key().for_testing_key_ref().get_u64()
                        );
                    }
                }
            }
        }
    }

    pub fn dump_size_only(&self) {
        print!("Levels: {}", self.snapshot.l0_sstables.len());
        for (_, files) in &self.snapshot.levels {
            print!(" {}", files.len());
        }
        println!();
    }

    pub fn dump_original_id(&self, always_show_l0: bool, with_key: bool) {
        if !self.snapshot.l0_sstables.is_empty() || always_show_l0 {
            println!(
                "L0 ({}): {:?}",
                self.snapshot.l0_sstables.len(),
                self.snapshot.l0_sstables,
            );
        }
        for (level, files) in &self.snapshot.levels {
            println!(
                "L{level} ({}): {:?}",
                files.len(),
                files.iter().map(|x| self.file_list[x]).collect::<Vec<_>>()
            );
        }
        if with_key {
            self.check_keys();
        }
    }

    pub fn dump_real_id(&self, always_show_l0: bool, with_key: bool) {
        if !self.snapshot.l0_sstables.is_empty() || always_show_l0 {
            println!(
                "L0 ({}): {:?}",
                self.snapshot.l0_sstables.len(),
                self.snapshot.l0_sstables,
            );
        }
        for (level, files) in &self.snapshot.levels {
            println!("L{level} ({}): {:?}", files.len(), files);
        }
        if with_key {
            self.check_keys();
        }
    }
}

fn generate_random_key_range() -> (KeyBytes, KeyBytes) {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let begin: usize = rng.gen_range(0..(1 << 31));
    let end: usize = begin + rng.gen_range((1 << 10)..(1 << 31));
    let mut begin_bytes = BytesMut::new();
    let mut end_bytes = BytesMut::new();
    begin_bytes.put_u64(begin as u64);
    end_bytes.put_u64(end as u64);
    (
        KeyBytes::for_testing_from_bytes_no_ts(begin_bytes.freeze()),
        KeyBytes::for_testing_from_bytes_no_ts(end_bytes.freeze()),
    )
}

fn generate_random_split(
    begin_bytes: KeyBytes,
    end_bytes: KeyBytes,
    split: usize,
) -> Vec<(KeyBytes, KeyBytes)> {
    let begin = begin_bytes.for_testing_key_ref().get_u64();
    let end = end_bytes.for_testing_key_ref().get_u64();
    let len = end - begin + 1;
    let mut result = Vec::new();
    let split = split as u64;
    assert!(len >= split, "well, this is unfortunate... run again!");
    for i in 0..split {
        let nb = begin + len * i / split;
        let ne = begin + len * (i + 1) / split - 1;
        let mut begin_bytes = BytesMut::new();
        let mut end_bytes = BytesMut::new();
        begin_bytes.put_u64(nb);
        end_bytes.put_u64(ne);
        result.push((
            KeyBytes::for_testing_from_bytes_no_ts(begin_bytes.freeze()),
            KeyBytes::for_testing_from_bytes_no_ts(end_bytes.freeze()),
        ));
    }
    result
}

fn main() {
    let args = Args::parse();
    match args {
        Args::Simple {
            dump_real_id,
            size_only,
            size_ratio_percent,
            iterations,
            level0_file_num_compaction_trigger,
            max_levels,
        } => {
            // TODO(chi): use unified logic for all 3 compactions...
            let controller =
                SimpleLeveledCompactionController::new(SimpleLeveledCompactionOptions {
                    size_ratio_percent,
                    level0_file_num_compaction_trigger,
                    max_levels,
                });
            let mut storage = MockStorage::new();
            for i in 0..max_levels {
                storage.snapshot.levels.push((i + 1, Vec::new()));
            }
            let mut max_space = 0;
            for i in 0..iterations {
                println!("=== Iteration {i} ===");
                storage.flush_sst_to_l0();
                println!("--- After Flush ---");
                if size_only {
                    storage.dump_size_only();
                } else if dump_real_id {
                    storage.dump_real_id(true, false);
                } else {
                    storage.dump_original_id(true, false);
                }
                let mut num_compactions = 0;
                while let Some(task) = {
                    if !size_only {
                        println!("--- Compaction Task ---");
                    }
                    controller.generate_compaction_task(&storage.snapshot)
                } {
                    let mut sst_ids = Vec::new();
                    for file in task
                        .upper_level_sst_ids
                        .iter()
                        .chain(task.lower_level_sst_ids.iter())
                    {
                        let new_sst_id = storage.generate_sst_id();
                        sst_ids.push(new_sst_id);
                        storage.file_list.insert(new_sst_id, *file);
                        storage.total_writes += 1;
                    }
                    print!(
                        "Upper L{} {:?} ",
                        task.upper_level.unwrap_or_default(),
                        task.upper_level_sst_ids
                    );
                    print!(
                        "Lower L{} {:?} ",
                        task.lower_level, task.lower_level_sst_ids
                    );
                    println!("-> {:?}", sst_ids);
                    max_space = max_space.max(storage.file_list.len());
                    let (snapshot, del) =
                        controller.apply_compaction_result(&storage.snapshot, &task, &sst_ids);
                    storage.snapshot = snapshot;
                    storage.remove(&del);
                    println!("--- After Compaction ---");
                    if size_only {
                        storage.dump_size_only();
                    } else if dump_real_id {
                        storage.dump_real_id(true, false);
                    } else {
                        storage.dump_original_id(true, false);
                    }
                    num_compactions += 1;
                    if num_compactions >= max_levels * 2 {
                        panic!("compaction does not converge?");
                    }
                }
                if num_compactions == 0 {
                    println!("no compaction triggered");
                } else {
                    println!("{num_compactions} compaction triggered in this iteration");
                }
                max_space = max_space.max(storage.file_list.len());
                println!("--- Statistics ---");
                println!(
                    "Write Amplification: {}/{}={:.3}x",
                    storage.total_writes,
                    storage.total_flushes,
                    storage.total_writes as f64 / storage.total_flushes as f64
                );
                println!(
                    "Maximum Space Usage: {}/{}={:.3}x",
                    max_space,
                    storage.total_flushes,
                    max_space as f64 / storage.total_flushes as f64
                );
                println!(
                    "Read Amplification: {}x",
                    storage.snapshot.l0_sstables.len()
                        + storage
                            .snapshot
                            .levels
                            .iter()
                            .filter(|(_, f)| !f.is_empty())
                            .count()
                );
                println!();
            }
        }
        Args::Tiered {
            dump_real_id,
            size_only,
            num_tiers: level0_file_num_compaction_trigger,
            max_size_amplification_percent,
            size_ratio,
            min_merge_width,
            max_merge_width,
            iterations,
        } => {
            let controller = TieredCompactionController::new(TieredCompactionOptions {
                num_tiers: level0_file_num_compaction_trigger,
                max_size_amplification_percent,
                size_ratio,
                min_merge_width,
                max_merge_width,
            });
            let mut storage = MockStorage::new();
            let mut max_space = 0;
            for i in 0..iterations {
                println!("=== Iteration {i} ===");
                storage.flush_sst_to_new_tier();
                println!("--- After Flush ---");
                if size_only {
                    storage.dump_size_only();
                } else if dump_real_id {
                    storage.dump_real_id(false, false);
                } else {
                    storage.dump_original_id(false, false);
                }
                if !size_only {
                    println!("--- Compaction Task ---");
                }
                let mut num_compactions = 0;
                while let Some(task) = {
                    if !size_only {
                        println!("--- Compaction Task ---");
                    }
                    controller.generate_compaction_task(&storage.snapshot)
                } {
                    let mut sst_ids = Vec::new();
                    for (tier_id, files) in &task.tiers {
                        for file in files {
                            let new_sst_id = storage.generate_sst_id();
                            sst_ids.push(new_sst_id);
                            storage.file_list.insert(new_sst_id, *file);
                            storage.total_writes += 1;
                        }
                        print!("L{} {:?} ", tier_id, files);
                    }
                    println!("-> {:?}", sst_ids);
                    max_space = max_space.max(storage.file_list.len());
                    let (snapshot, del) =
                        controller.apply_compaction_result(&storage.snapshot, &task, &sst_ids);
                    storage.snapshot = snapshot;
                    storage.remove(&del);
                    println!("--- After Compaction ---");
                    if size_only {
                        storage.dump_size_only();
                    } else if dump_real_id {
                        storage.dump_real_id(false, false);
                    } else {
                        storage.dump_original_id(false, false);
                    }
                    num_compactions += 1;
                    if num_compactions >= level0_file_num_compaction_trigger * 3 {
                        panic!("compaction does not converge?");
                    }
                }
                if num_compactions == 0 {
                    println!("no compaction triggered");
                } else {
                    println!("{num_compactions} compaction triggered in this iteration");
                }
                max_space = max_space.max(storage.file_list.len());
                println!("--- Statistics ---");
                println!(
                    "Write Amplification: {}/{}={:.3}x",
                    storage.total_writes,
                    storage.total_flushes,
                    storage.total_writes as f64 / storage.total_flushes as f64
                );
                println!(
                    "Maximum Space Usage: {}/{}={:.3}x",
                    max_space,
                    storage.total_flushes,
                    max_space as f64 / storage.total_flushes as f64
                );
                println!(
                    "Read Amplification: {}x",
                    storage.snapshot.l0_sstables.len()
                        + storage
                            .snapshot
                            .levels
                            .iter()
                            .filter(|(_, f)| !f.is_empty())
                            .count()
                );
                println!();
            }
        }
        Args::Leveled {
            dump_real_id,
            size_only,
            level0_file_num_compaction_trigger,
            level_size_multiplier,
            max_levels,
            base_level_size_mb,
            iterations,
            sst_size_mb,
        } => {
            let controller = LeveledCompactionController::new(LeveledCompactionOptions {
                level0_file_num_compaction_trigger,
                level_size_multiplier,
                max_levels,
                base_level_size_mb,
            });

            let mut storage = MockStorage::new();
            for i in 0..max_levels {
                storage.snapshot.levels.push((i + 1, Vec::new()));
            }
            let mut max_space = 0;
            for i in 0..iterations {
                println!("=== Iteration {i} ===");
                let id = storage.flush_sst_to_l0();
                let (first_key, last_key) = generate_random_key_range();
                storage.snapshot.sstables.insert(
                    id,
                    Arc::new(SsTable::create_meta_only(
                        id,
                        sst_size_mb as u64 * 1024 * 1024,
                        first_key,
                        last_key,
                    )),
                );
                println!("--- After Flush ---");
                if size_only {
                    storage.dump_size_only();
                } else if dump_real_id {
                    storage.dump_real_id(false, true);
                } else {
                    storage.dump_original_id(false, true);
                }
                let mut num_compactions = 0;
                while let Some(task) = {
                    if !size_only {
                        println!("--- Compaction Task ---");
                    }
                    controller.generate_compaction_task(&storage.snapshot)
                } {
                    let mut sst_ids = Vec::new();
                    let split_num = task.upper_level_sst_ids.len() + task.lower_level_sst_ids.len();
                    let mut first_keys = Vec::new();
                    let mut last_keys = Vec::new();
                    for file in task
                        .upper_level_sst_ids
                        .iter()
                        .chain(task.lower_level_sst_ids.iter())
                    {
                        first_keys.push(storage.snapshot.sstables[file].first_key().clone());
                        last_keys.push(storage.snapshot.sstables[file].last_key().clone());
                    }
                    let begin = first_keys.into_iter().min().unwrap();
                    let end = last_keys.into_iter().max().unwrap();
                    let splits = generate_random_split(begin, end, split_num);
                    for (id, file) in task
                        .upper_level_sst_ids
                        .iter()
                        .chain(task.lower_level_sst_ids.iter())
                        .enumerate()
                    {
                        let new_sst_id = storage.generate_sst_id();
                        sst_ids.push(new_sst_id);
                        storage.file_list.insert(new_sst_id, *file);
                        storage.total_writes += 1;
                        storage.snapshot.sstables.insert(
                            new_sst_id,
                            Arc::new(SsTable::create_meta_only(
                                new_sst_id,
                                sst_size_mb as u64 * 1024 * 1024,
                                splits[id].0.clone(),
                                splits[id].1.clone(),
                            )),
                        );
                    }
                    print!(
                        "Upper L{} [{}] ",
                        task.upper_level.unwrap_or_default(),
                        task.upper_level_sst_ids
                            .iter()
                            .map(|id| format!(
                                "{}.sst {:x}..={:x}",
                                id,
                                storage.snapshot.sstables[id]
                                    .first_key()
                                    .for_testing_key_ref()
                                    .get_u64(),
                                storage.snapshot.sstables[id]
                                    .last_key()
                                    .for_testing_key_ref()
                                    .get_u64()
                            ))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    print!(
                        "Lower L{} [{}] ",
                        task.lower_level,
                        task.lower_level_sst_ids
                            .iter()
                            .map(|id| format!(
                                "{}.sst {:x}..={:x}",
                                id,
                                storage.snapshot.sstables[id]
                                    .first_key()
                                    .for_testing_key_ref()
                                    .get_u64(),
                                storage.snapshot.sstables[id]
                                    .last_key()
                                    .for_testing_key_ref()
                                    .get_u64()
                            ))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    println!(
                        "-> [{}]",
                        sst_ids
                            .iter()
                            .map(|id| format!(
                                "{}.sst {:x}..={:x}",
                                id,
                                storage.snapshot.sstables[id]
                                    .first_key()
                                    .for_testing_key_ref()
                                    .get_u64(),
                                storage.snapshot.sstables[id]
                                    .last_key()
                                    .for_testing_key_ref()
                                    .get_u64()
                            ))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    max_space = max_space.max(storage.file_list.len());
                    let (snapshot, del) = controller.apply_compaction_result(
                        &storage.snapshot,
                        &task,
                        &sst_ids,
                        false,
                    );
                    storage.snapshot = snapshot;
                    storage.remove(&del);
                    println!("--- After Compaction ---");
                    if size_only {
                        storage.dump_size_only();
                    } else if dump_real_id {
                        storage.dump_real_id(true, true);
                    } else {
                        storage.dump_original_id(true, true);
                    }
                    num_compactions += 1;
                    if num_compactions >= level0_file_num_compaction_trigger * max_levels * 2 {
                        panic!("compaction does not converge?");
                    }
                }
                if num_compactions == 0 {
                    println!("no compaction triggered");
                } else {
                    println!("{num_compactions} compaction triggered in this iteration");
                }
                max_space = max_space.max(storage.file_list.len());
                println!("--- Statistics ---");
                println!(
                    "Write Amplification: {}/{}={:.3}x",
                    storage.total_writes,
                    storage.total_flushes,
                    storage.total_writes as f64 / storage.total_flushes as f64
                );
                println!(
                    "Maximum Space Usage: {}/{}={:.3}x",
                    max_space,
                    storage.total_flushes,
                    max_space as f64 / storage.total_flushes as f64
                );
                println!(
                    "Read Amplification: {}x",
                    storage.snapshot.l0_sstables.len()
                        + storage
                            .snapshot
                            .levels
                            .iter()
                            .filter(|(_, f)| !f.is_empty())
                            .count()
                );
                println!();
            }
        }
    }
}
