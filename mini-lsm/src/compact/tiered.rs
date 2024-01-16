use crate::lsm_storage::LsmStorageInner;
use crate::table::SsTable;

pub struct TieredCompactionTask {
    tiers: Vec<usize>,
}

pub struct TieredCompactionController {}

impl TieredCompactionController {
    pub fn generate_compaction_task(&self, snapshot: &LsmStorageInner) -> TieredCompactionTask {
        return TieredCompactionTask { tiers: Vec::new() };
    }

    pub fn apply_compaction_result(
        &self,
        snapshot: &LsmStorageInner,
        task: &TieredCompactionTask,
        output: &[usize],
    ) -> (LsmStorageInner, Vec<usize>) {
        (snapshot.clone(), Vec::new())
    }
}
