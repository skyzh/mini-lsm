use bytes::Bytes;
use tempfile::tempdir;

use crate::{
    compact::CompactionOptions,
    lsm_storage::{CompactionFilter, LsmStorageOptions, MiniLsm, WriteBatchRecord},
};

use super::harness::{check_iter_result_by_key, construct_merge_iterator_over_storage};

#[test]
fn test_task3_mvcc_compaction() {
    let dir = tempdir().unwrap();
    let options = LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction);
    let storage = MiniLsm::open(&dir, options.clone()).unwrap();
    storage
        .write_batch(&[
            WriteBatchRecord::Put("table1_a", "1"),
            WriteBatchRecord::Put("table1_b", "1"),
            WriteBatchRecord::Put("table1_c", "1"),
            WriteBatchRecord::Put("table2_a", "1"),
            WriteBatchRecord::Put("table2_b", "1"),
            WriteBatchRecord::Put("table2_c", "1"),
        ])
        .unwrap();
    storage.force_flush().unwrap();
    let snapshot0 = storage.new_txn().unwrap();
    storage
        .write_batch(&[
            WriteBatchRecord::Put("table1_a", "2"),
            WriteBatchRecord::Del("table1_b"),
            WriteBatchRecord::Put("table1_c", "2"),
            WriteBatchRecord::Put("table2_a", "2"),
            WriteBatchRecord::Del("table2_b"),
            WriteBatchRecord::Put("table2_c", "2"),
        ])
        .unwrap();
    storage.force_flush().unwrap();
    storage.add_compaction_filter(CompactionFilter::Prefix(Bytes::from("table2_")));
    storage.force_full_compaction().unwrap();

    let mut iter = construct_merge_iterator_over_storage(&storage.inner.state.read());
    check_iter_result_by_key(
        &mut iter,
        vec![
            (Bytes::from("table1_a"), Bytes::from("2")),
            (Bytes::from("table1_a"), Bytes::from("1")),
            (Bytes::from("table1_b"), Bytes::new()),
            (Bytes::from("table1_b"), Bytes::from("1")),
            (Bytes::from("table1_c"), Bytes::from("2")),
            (Bytes::from("table1_c"), Bytes::from("1")),
            (Bytes::from("table2_a"), Bytes::from("2")),
            (Bytes::from("table2_b"), Bytes::new()),
            (Bytes::from("table2_c"), Bytes::from("2")),
        ],
    );

    drop(snapshot0);

    storage.force_full_compaction().unwrap();

    let mut iter = construct_merge_iterator_over_storage(&storage.inner.state.read());
    check_iter_result_by_key(
        &mut iter,
        vec![
            (Bytes::from("table1_a"), Bytes::from("2")),
            (Bytes::from("table1_c"), Bytes::from("2")),
        ],
    );
}
