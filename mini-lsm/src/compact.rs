mod leveled;
mod simple_leveled;
mod tiered;

use std::sync::Arc;

use anyhow::Result;
pub use leveled::{LeveledCompactionController, LeveledCompactionOptions, LeveledCompactionTask};
pub use simple_leveled::{
    SimpleLeveledCompactionController, SimpleLeveledCompactionOptions, SimpleLeveledCompactionTask,
};
pub use tiered::{TieredCompactionController, TieredCompactionOptions, TieredCompactionTask};

use crate::iterators::merge_iterator::MergeIterator;
use crate::iterators::StorageIterator;
use crate::lsm_storage::LsmStorageInner;
use crate::table::{SsTable, SsTableBuilder, SsTableIterator};

pub(crate) enum CompactionTask {
    Leveled(LeveledCompactionTask),
    Tiered(TieredCompactionTask),
    Simple(SimpleLeveledCompactionTask),
}

struct CompactOptions {
    block_size: usize,
    target_sst_size: usize,
}

pub(crate) enum CompactionController {
    Leveled(LeveledCompactionController),
    Tiered(TieredCompactionController),
    Simple(SimpleLeveledCompactionController),
    NoCompaction,
}

impl CompactionController {
    pub fn flush_to_l0(&self) -> bool {
        if let Self::Leveled(_) | Self::Simple(_) | Self::NoCompaction = self {
            true
        } else {
            false
        }
    }
}

pub enum CompactionOptions {
    /// Leveled compaction with partial compaction + dynamic level support (= RocksDB's Leveled
    /// Compaction)
    Leveled(LeveledCompactionOptions),
    /// Tiered compaction (= RocksDB's universal compaction)
    Tiered(TieredCompactionOptions),
    /// Simple leveled compaction
    Simple(SimpleLeveledCompactionOptions),
    /// In no compaction mode (week 1), always flush to L0
    NoCompaction,
}

impl LsmStorageInner {
    #[allow(dead_code)]
    fn compact(
        &self,
        tables: Vec<Arc<SsTable>>,
        options: CompactOptions,
    ) -> Result<Vec<Arc<SsTable>>> {
        let mut iters = Vec::new();
        iters.reserve(tables.len());
        for table in tables.iter() {
            iters.push(Box::new(SsTableIterator::create_and_seek_to_first(
                table.clone(),
            )?));
        }
        let mut iter = MergeIterator::create(iters);

        let mut builder = None;
        let mut new_sst = vec![];

        let compact_to_bottom_level = false;

        while iter.is_valid() {
            if builder.is_none() {
                builder = Some(SsTableBuilder::new(options.block_size));
            }
            let builder_inner = builder.as_mut().unwrap();
            if compact_to_bottom_level {
                if !iter.value().is_empty() {
                    builder_inner.add(iter.key(), iter.value());
                }
            } else {
                builder_inner.add(iter.key(), iter.value());
            }
            iter.next()?;

            if builder_inner.estimated_size() >= options.target_sst_size {
                let sst_id = self.next_sst_id(); // lock dropped here
                let builder = builder.take().unwrap();
                let sst = Arc::new(builder.build(
                    sst_id,
                    Some(self.block_cache.clone()),
                    self.path_of_sst(sst_id),
                )?);
                new_sst.push(sst);
            }
        }
        if let Some(builder) = builder {
            let sst_id = self.next_sst_id(); // lock dropped here
            let sst = Arc::new(builder.build(
                sst_id,
                Some(self.block_cache.clone()),
                self.path_of_sst(sst_id),
            )?);
            new_sst.push(sst);
        }
        Ok(new_sst)
    }

    pub(crate) fn spawn_compaction_thread(
        self: &Arc<Self>,
        rx: std::sync::mpsc::Receiver<()>,
    ) -> Result<Option<std::thread::JoinHandle<()>>> {
        Ok(None)
    }
}
