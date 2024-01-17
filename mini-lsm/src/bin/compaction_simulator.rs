use std::collections::HashMap;
use std::sync::Arc;

use clap::Parser;
use mini_lsm::compact::{TieredCompactionController, TieredCompactionOptions};
use mini_lsm::lsm_storage::LsmStorageInner;
use mini_lsm::mem_table::MemTable;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
enum Args {
    Tiered {
        #[clap(long)]
        dump_real_id: bool,
        #[clap(long, default_value = "3")]
        level0_file_num_compaction_trigger: usize,
        #[clap(long, default_value = "200")]
        max_size_amplification_percent: usize,
        #[clap(long, default_value = "1")]
        size_ratio: usize,
        #[clap(long, default_value = "2")]
        min_merge_width: usize,
        #[clap(long, default_value = "50")]
        iterations: usize,
    },
    Leveled {},
}

pub struct MockStorage {
    snapshot: LsmStorageInner,
    next_sst_id: usize,
    /// Maps SST ID to the original flushed SST ID
    file_list: HashMap<usize, usize>,
    total_flushes: usize,
    total_writes: usize,
}

impl MockStorage {
    pub fn new() -> Self {
        let snapshot = LsmStorageInner {
            memtable: Arc::new(MemTable::create()),
            imm_memtables: Vec::new(),
            l0_sstables: Vec::new(),
            levels: Vec::new(),
            sstables: Default::default(),
        };
        Self {
            snapshot,
            next_sst_id: 0,
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

    pub fn flush_sst_to_l0(&mut self) {
        let id = self.generate_sst_id();
        self.snapshot.l0_sstables.push(id);
        self.file_list.insert(id, id);
        self.total_flushes += 1;
        self.total_writes += 1;
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

    pub fn dump_original_id(&self) {
        if !self.snapshot.l0_sstables.is_empty() {
            println!("L0: {:?}", self.snapshot.l0_sstables);
        }
        for (level, files) in &self.snapshot.levels {
            println!(
                "L{level} ({}): {:?}",
                files.len(),
                files.iter().map(|x| self.file_list[x]).collect::<Vec<_>>()
            );
        }
    }

    pub fn dump_real_id(&self) {
        if !self.snapshot.l0_sstables.is_empty() {
            println!("L0: {:?}", self.snapshot.l0_sstables);
        }
        for (level, files) in &self.snapshot.levels {
            println!(
                "L{level} ({}): {:?}",
                files.len(),
                files.iter().map(|x| self.file_list[x]).collect::<Vec<_>>()
            );
        }
    }
}

fn main() {
    let args = Args::parse();
    match args {
        Args::Tiered {
            dump_real_id,
            level0_file_num_compaction_trigger,
            max_size_amplification_percent,
            size_ratio,
            min_merge_width,
            iterations,
        } => {
            let controller = TieredCompactionController::new(TieredCompactionOptions {
                level0_file_num_compaction_trigger,
                max_size_amplification_percent,
                size_ratio,
                min_merge_width,
            });
            let mut storage = MockStorage::new();
            let mut max_space = 0;
            for i in 0..iterations {
                println!("=== Iteration {i} ===");
                storage.flush_sst_to_new_tier();
                println!("--- After Flush ---");
                if dump_real_id {
                    storage.dump_real_id();
                } else {
                    storage.dump_original_id();
                }
                let task = controller.generate_compaction_task(&storage.snapshot);
                println!("--- Compaction Task ---");
                if let Some(task) = task {
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
                    if dump_real_id {
                        storage.dump_real_id();
                    } else {
                        storage.dump_original_id();
                    }
                } else {
                    println!("no compaction triggered");
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
                    "Space Amplification: {}/{}={:.3}x",
                    max_space,
                    storage.total_flushes,
                    max_space as f64 / storage.total_flushes as f64
                );
                println!();
            }
        }
        Args::Leveled {} => {}
    }
}
