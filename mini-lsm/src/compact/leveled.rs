use crate::lsm_storage::LsmStorageInner;

pub struct LeveledCompactionTask {
    upper_level: usize,
    upper_level_sst_ids: Vec<usize>,
    lower_level: usize,
    lower_level_sst_ids: Vec<usize>,
}

pub struct LeveledCompactionController {}

impl LeveledCompactionController {
    pub fn generate_compaction_task(&self, snapshot: &LsmStorageInner) -> LeveledCompactionTask {
        unimplemented!()
    }

    pub fn apply_compaction_result(
        &self,
        snapshot: &LsmStorageInner,
        task: &LeveledCompactionTask,
        output: &[usize],
    ) -> (LsmStorageInner, Vec<usize>) {
        unimplemented!()
    }
}
