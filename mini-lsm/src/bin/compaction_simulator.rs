use std::collections::HashSet;
use std::sync::Arc;

use clap::Parser;
use mini_lsm::compact::TieredCompactionController;
use mini_lsm::lsm_storage::LsmStorageInner;
use mini_lsm::mem_table::MemTable;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
enum Args {
    Tiered {},
    Leveled {},
}

pub struct MockStorage {
    snapshot: LsmStorageInner,
    next_sst_id: usize,
    file_list: HashSet<usize>,
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
        }
    }

    fn generate_sst_id(&mut self) -> usize {
        let id = self.next_sst_id;
        self.next_sst_id += 1;
        id
    }

    pub fn flush_sst(&mut self) {
        let id = self.generate_sst_id();
        self.snapshot.l0_sstables.push(id);
        self.file_list.insert(id);
    }

    pub fn remove(&mut self, files_to_remove: &[usize]) {
        for file_id in files_to_remove {
            self.file_list.remove(file_id);
        }
    }

    pub fn dump(&self) {
        print!("L0: {:?}", self.snapshot.l0_sstables);
        for (level, files) in &self.snapshot.levels {
            print!("L{level}: {:?}", files);
        }
    }
}

fn main() {
    let args = Args::parse();
    match args {
        Args::Tiered {} => {
            let controller = TieredCompactionController {};
            let mut storage = MockStorage::new();
            for i in 0..500 {
                println!("Iteration {i}");
                storage.flush_sst();
                let task = controller.generate_compaction_task(&storage.snapshot);
                let sst_id = storage.generate_sst_id();
                let (snapshot, del) =
                    controller.apply_compaction_result(&storage.snapshot, &task, &[sst_id]);
                storage.snapshot = snapshot;
                storage.remove(&del);
                storage.dump();
            }
        }
        Args::Leveled {} => {}
    }
}
