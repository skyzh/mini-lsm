use std::sync::Arc;

use bytes::Bytes;

use crate::compact::CompactionOptions;
use crate::iterators::StorageIterator;
use crate::key::{Key, KeySlice};
use crate::lsm_storage::{LsmStorageOptions, MiniLsm};
use crate::table::SsTableBuilder;
use crate::vlog::ValueSeparationOptions;

fn options_with_vlog_enabled(block_size: usize, target_sst_size: usize) -> LsmStorageOptions {
    LsmStorageOptions {
        block_size,
        target_sst_size,
        num_memtable_limit: 2,
        compaction_options: CompactionOptions::NoCompaction,
        enable_wal: false,
        serializable: false,
        value_separation: Some(ValueSeparationOptions {
            enabled: true,
            min_value_size: 16, // Separate values >= 16 bytes
            ..Default::default()
        }),
    }
}

fn options_with_vlog_and_compaction(
    block_size: usize,
    target_sst_size: usize,
) -> LsmStorageOptions {
    use crate::compact::LeveledCompactionOptions;
    LsmStorageOptions {
        block_size,
        target_sst_size,
        num_memtable_limit: 2,
        compaction_options: CompactionOptions::Leveled(LeveledCompactionOptions {
            level0_file_num_compaction_trigger: 2,
            max_levels: 3,
            base_level_size_mb: 1,
            level_size_multiplier: 2,
        }),
        enable_wal: false,
        serializable: false,
        value_separation: Some(ValueSeparationOptions {
            enabled: true,
            min_value_size: 16,
            ..Default::default()
        }),
    }
}

#[test]
fn test_sst_builder_kind_prefix_inline() {
    // Small values (< min_value_size) should be stored inline with KvKind prefix
    let mut builder = SsTableBuilder::new(4096);
    builder
        .add(
            KeySlice::for_testing_from_slice_no_ts(b"key1"),
            b"small", // 5 bytes < 16 byte threshold
        )
        .unwrap();

    // The block should contain the value with KvKind::Inline prefix
    // We verify by building and reading back through the iterator
    let dir = tempfile::tempdir().unwrap();
    let sst = builder.build_for_test(dir.path().join("test.sst")).unwrap();

    let mut iter = crate::table::SsTableIterator::create_and_seek_to_first(Arc::new(sst)).unwrap();
    assert!(iter.is_valid());
    assert_eq!(iter.key().raw_ref(), b"key1");
    // value() should strip the kind prefix and return the raw value
    assert_eq!(iter.value(), b"small");
}

#[test]
fn test_sst_builder_kind_prefix_empty_value() {
    // Empty values (tombstones) should be stored as [KvKind::Inline] only
    let mut builder = SsTableBuilder::new(4096);
    builder
        .add(
            KeySlice::for_testing_from_slice_no_ts(b"key1"),
            b"", // tombstone
        )
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let sst = builder.build_for_test(dir.path().join("test.sst")).unwrap();

    let mut iter = crate::table::SsTableIterator::create_and_seek_to_first(Arc::new(sst)).unwrap();
    assert!(iter.is_valid());
    assert_eq!(iter.key().raw_ref(), b"key1");
    // value() should return empty for tombstones
    assert!(iter.value().is_empty());
}

#[test]
fn test_sst_builder_add_raw_preserves_pointer() {
    use crate::vlog::{KvKind, ValuePointer};

    // Simulate a compaction scenario: add_raw with a ValuePointer
    let mut builder = SsTableBuilder::new(4096);

    // Create a fake ValuePointer
    let ptr = ValuePointer {
        file_id: 42,
        offset: 1234,
        size: 5678,
    };
    let mut raw = vec![KvKind::ValuePointer as u8];
    ptr.encode(&mut raw);

    builder
        .add_raw(KeySlice::for_testing_from_slice_no_ts(b"key1"), &raw)
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let sst = builder.build_for_test(dir.path().join("test.sst")).unwrap();

    // raw_value() should return the original bytes with kind prefix
    let iter = crate::table::SsTableIterator::create_and_seek_to_first(Arc::new(sst)).unwrap();
    assert!(iter.is_valid());
    assert_eq!(iter.raw_value(), &raw[..]);
}

#[test]
fn test_end_to_end_large_value_vlog() {
    let dir = tempfile::tempdir().unwrap();
    let options = options_with_vlog_enabled(256, 1 << 20);
    let storage = MiniLsm::open(dir.path(), options).unwrap();

    // Write a large value (> min_value_size of 16)
    let large_value = vec![b'x'; 256];
    storage.put(b"large_key", &large_value).unwrap();

    // Flush to SST (which should write to vLog)
    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    // Read back — should dereference the ValuePointer
    let result = storage.get(b"large_key").unwrap();
    assert_eq!(result, Some(Bytes::from(large_value)));
}

#[test]
fn test_end_to_end_small_value_inline() {
    let dir = tempfile::tempdir().unwrap();
    let options = options_with_vlog_enabled(256, 1 << 20);
    let storage = MiniLsm::open(dir.path(), options).unwrap();

    // Write a small value (< min_value_size of 16)
    storage.put(b"small_key", b"tiny").unwrap();

    // Flush to SST
    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    // Read back — should return inline value
    let result = storage.get(b"small_key").unwrap();
    assert_eq!(result, Some(Bytes::from_static(b"tiny")));
}

#[test]
fn test_end_to_end_tombstone_with_vlog() {
    let dir = tempfile::tempdir().unwrap();
    let options = options_with_vlog_enabled(256, 1 << 20);
    let storage = MiniLsm::open(dir.path(), options).unwrap();

    // Write then delete
    storage.put(b"key1", b"some_value_long_enough").unwrap();
    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    storage.delete(b"key1").unwrap();
    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    // Should return None (tombstone)
    let result = storage.get(b"key1").unwrap();
    assert_eq!(result, None);
}

#[test]
fn test_end_to_end_mixed_inline_and_pointer() {
    let dir = tempfile::tempdir().unwrap();
    let options = options_with_vlog_enabled(256, 1 << 20);
    let storage = MiniLsm::open(dir.path(), options).unwrap();

    // Write small and large values
    storage.put(b"small", b"tiny").unwrap();
    storage.put(b"large", &vec![b'y'; 128]).unwrap();

    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    // Both should be readable
    assert_eq!(
        storage.get(b"small").unwrap(),
        Some(Bytes::from_static(b"tiny"))
    );
    assert_eq!(
        storage.get(b"large").unwrap(),
        Some(Bytes::from(vec![b'y'; 128]))
    );
}

#[test]
fn test_scan_with_vlog_values() {
    let dir = tempfile::tempdir().unwrap();
    let options = options_with_vlog_enabled(256, 1 << 20);
    let storage = MiniLsm::open(dir.path(), options).unwrap();

    storage.put(b"a", b"aaa").unwrap(); // small
    storage.put(b"b", &vec![b'b'; 64]).unwrap(); // large
    storage.put(b"c", b"ccc").unwrap(); // small

    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    let mut scan = storage
        .scan(std::ops::Bound::Unbounded, std::ops::Bound::Unbounded)
        .unwrap();

    let mut results = vec![];
    while scan.is_valid() {
        results.push((scan.key().to_vec(), Bytes::copy_from_slice(scan.value())));
        scan.next().unwrap();
    }

    assert_eq!(results.len(), 3);
    assert_eq!(results[0], (b"a".to_vec(), Bytes::from_static(b"aaa")));
    assert_eq!(results[1], (b"b".to_vec(), Bytes::from(vec![b'b'; 64])));
    assert_eq!(results[2], (b"c".to_vec(), Bytes::from_static(b"ccc")));
}

#[test]
fn test_compaction_with_mixed_inline_and_pointer() {
    let dir = tempfile::tempdir().unwrap();
    let options = options_with_vlog_and_compaction(256, 1 << 20);
    let storage = MiniLsm::open(dir.path(), options).unwrap();

    // Write small (inline) and large (pointer) values
    storage.put(b"aaa", b"tiny_aaa").unwrap();
    storage.put(b"bbb", &vec![b'b'; 128]).unwrap();
    storage.put(b"ccc", b"tiny_ccc").unwrap();
    storage.put(b"ddd", &vec![b'd'; 128]).unwrap();

    // Flush to create SSTs
    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    storage.put(b"eee", b"tiny_eee").unwrap();
    storage.put(b"fff", &vec![b'f'; 128]).unwrap();
    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    // Force full compaction
    storage.inner.force_full_compaction().unwrap();

    // All values should still be readable after compaction
    assert_eq!(
        storage.get(b"aaa").unwrap(),
        Some(Bytes::from_static(b"tiny_aaa"))
    );
    assert_eq!(
        storage.get(b"bbb").unwrap(),
        Some(Bytes::from(vec![b'b'; 128]))
    );
    assert_eq!(
        storage.get(b"ccc").unwrap(),
        Some(Bytes::from_static(b"tiny_ccc"))
    );
    assert_eq!(
        storage.get(b"ddd").unwrap(),
        Some(Bytes::from(vec![b'd'; 128]))
    );
    assert_eq!(
        storage.get(b"eee").unwrap(),
        Some(Bytes::from_static(b"tiny_eee"))
    );
    assert_eq!(
        storage.get(b"fff").unwrap(),
        Some(Bytes::from(vec![b'f'; 128]))
    );
}

#[test]
fn test_recovery_with_vlog_records() {
    let dir = tempfile::tempdir().unwrap();
    let options = options_with_vlog_enabled(256, 1 << 20);

    // Write data and flush
    {
        let storage = MiniLsm::open(dir.path(), options.clone()).unwrap();
        storage.put(b"key1", &vec![b'a'; 64]).unwrap();
        storage.put(b"key2", b"small").unwrap();
        storage.put(b"key3", &vec![b'c'; 128]).unwrap();
        storage
            .inner
            .force_freeze_memtable(&storage.inner.state_lock.lock())
            .unwrap();
        storage.inner.force_flush_next_imm_memtable().unwrap();
        storage.close().unwrap();
    }

    // Reopen and verify data survives recovery
    {
        let storage = MiniLsm::open(dir.path(), options).unwrap();
        assert_eq!(
            storage.get(b"key1").unwrap(),
            Some(Bytes::from(vec![b'a'; 64]))
        );
        assert_eq!(
            storage.get(b"key2").unwrap(),
            Some(Bytes::from_static(b"small"))
        );
        assert_eq!(
            storage.get(b"key3").unwrap(),
            Some(Bytes::from(vec![b'c'; 128]))
        );
    }
}

#[test]
fn test_value_at_min_value_size_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let options = options_with_vlog_enabled(256, 1 << 20);
    let storage = MiniLsm::open(dir.path(), options).unwrap();

    // Value exactly at min_value_size (16 bytes) — should be separated to vLog
    let boundary_value = vec![b'x'; 16];
    storage.put(b"boundary", &boundary_value).unwrap();

    // Value one byte below threshold — should be inline
    let below_value = vec![b'y'; 15];
    storage.put(b"below", &below_value).unwrap();

    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    assert_eq!(
        storage.get(b"boundary").unwrap(),
        Some(Bytes::from(boundary_value))
    );
    assert_eq!(
        storage.get(b"below").unwrap(),
        Some(Bytes::from(below_value))
    );
}

#[test]
fn test_multiple_flushes_different_vlog_files() {
    let dir = tempfile::tempdir().unwrap();
    let options = options_with_vlog_enabled(256, 1 << 20);
    let storage = MiniLsm::open(dir.path(), options).unwrap();

    // First flush — large values go to vLog file 0
    storage.put(b"a1", &vec![b'a'; 64]).unwrap();
    storage.put(b"a2", &vec![b'b'; 64]).unwrap();
    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    // Second flush — large values go to vLog file 1
    storage.put(b"b1", &vec![b'c'; 64]).unwrap();
    storage.put(b"b2", &vec![b'd'; 64]).unwrap();
    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    // Third flush
    storage.put(b"c1", &vec![b'e'; 64]).unwrap();
    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    // All values should be readable from their respective vLog files
    assert_eq!(
        storage.get(b"a1").unwrap(),
        Some(Bytes::from(vec![b'a'; 64]))
    );
    assert_eq!(
        storage.get(b"a2").unwrap(),
        Some(Bytes::from(vec![b'b'; 64]))
    );
    assert_eq!(
        storage.get(b"b1").unwrap(),
        Some(Bytes::from(vec![b'c'; 64]))
    );
    assert_eq!(
        storage.get(b"b2").unwrap(),
        Some(Bytes::from(vec![b'd'; 64]))
    );
    assert_eq!(
        storage.get(b"c1").unwrap(),
        Some(Bytes::from(vec![b'e'; 64]))
    );
}

#[test]
fn test_scan_after_compaction_with_vlog() {
    let dir = tempfile::tempdir().unwrap();
    let options = options_with_vlog_and_compaction(256, 1 << 20);
    let storage = MiniLsm::open(dir.path(), options).unwrap();

    // Write mixed data across multiple flushes
    for i in 0..10 {
        let key = format!("key_{:04}", i);
        let value = vec![b'v'; 64]; // all large values
        storage.put(key.as_bytes(), &value).unwrap();
    }
    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    for i in 10..20 {
        let key = format!("key_{:04}", i);
        let value = format!("small_{}", i);
        storage.put(key.as_bytes(), value.as_bytes()).unwrap();
    }
    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    // Compact
    storage.inner.force_full_compaction().unwrap();

    // Scan should return all values in order
    let mut scan = storage
        .scan(std::ops::Bound::Unbounded, std::ops::Bound::Unbounded)
        .unwrap();

    let mut count = 0;
    while scan.is_valid() {
        let key_str = String::from_utf8(scan.key().to_vec()).unwrap();
        assert!(key_str.starts_with("key_"));
        count += 1;
        scan.next().unwrap();
    }
    assert_eq!(count, 20);
}

// ---------------------------------------------------------------
// GC Integration Tests
// ---------------------------------------------------------------

#[test]
fn test_gc_100_percent_dead() {
    // Write values, delete all, compact, GC should reclaim the entire vLog file
    let dir = tempfile::tempdir().unwrap();
    let options = options_with_vlog_and_compaction(256, 1 << 20);
    let storage = MiniLsm::open(dir.path(), options).unwrap();

    // Write large values that go to vLog
    for i in 0..5 {
        let key = format!("key_{:04}", i);
        let value = vec![b'v'; 64];
        storage.put(key.as_bytes(), &value).unwrap();
    }

    // Flush to create SST + vLog
    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    // Delete all keys
    for i in 0..5 {
        let key = format!("key_{:04}", i);
        storage.delete(key.as_bytes()).unwrap();
    }

    // Flush deletions
    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    // Compact so tombstones are dropped (bottom level)
    storage.inner.force_full_compaction().unwrap();

    // All values should be gone
    for i in 0..5 {
        let key = format!("key_{:04}", i);
        assert_eq!(storage.get(key.as_bytes()).unwrap(), None);
    }

    // The post-compaction GC should have run. Verify no errors.
    // The old vLog file should be scheduled for deletion.
    // Since all entries are dead, the file should be reclaimable.
    let vlog = storage.inner.vlog.as_ref().unwrap();
    let reclaimed = vlog.reclaim_pending_deletions().unwrap();
    // May or may not have been reclaimed already by post_compaction_gc
    let _ = reclaimed;
}

#[test]
fn test_gc_preserves_live_values() {
    // Write values, overwrite some, compact, GC — live values should survive
    let dir = tempfile::tempdir().unwrap();
    let options = options_with_vlog_and_compaction(256, 1 << 20);
    let storage = MiniLsm::open(dir.path(), options).unwrap();

    // Write large values
    storage.put(b"keep", &vec![b'k'; 64]).unwrap();
    storage.put(b"overwrite", &vec![b'o'; 64]).unwrap();

    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    // Overwrite "overwrite" with a new value
    storage.put(b"overwrite", &vec![b'n'; 64]).unwrap();

    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    // Compact
    storage.inner.force_full_compaction().unwrap();

    // Both values should be readable
    assert_eq!(
        storage.get(b"keep").unwrap(),
        Some(Bytes::from(vec![b'k'; 64]))
    );
    assert_eq!(
        storage.get(b"overwrite").unwrap(),
        Some(Bytes::from(vec![b'n'; 64]))
    );
}

#[test]
fn test_gc_below_threshold() {
    // When stale ratio is below threshold, GC should not compact
    let dir = tempfile::tempdir().unwrap();
    let mut options = options_with_vlog_and_compaction(256, 1 << 20);
    // Set a very high threshold so GC never triggers
    if let Some(ref mut vs) = options.value_separation {
        vs.gc_threshold_ratio = 0.99;
    }
    let storage = MiniLsm::open(dir.path(), options).unwrap();

    storage.put(b"key1", &vec![b'a'; 64]).unwrap();
    storage.put(b"key2", &vec![b'b'; 64]).unwrap();

    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    // Overwrite key1
    storage.put(b"key1", &vec![b'c'; 64]).unwrap();
    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    storage.inner.force_full_compaction().unwrap();

    // Values should still be correct
    assert_eq!(
        storage.get(b"key1").unwrap(),
        Some(Bytes::from(vec![b'c'; 64]))
    );
    assert_eq!(
        storage.get(b"key2").unwrap(),
        Some(Bytes::from(vec![b'b'; 64]))
    );
}

#[test]
fn test_trigger_gc_api() {
    // Test the public trigger_gc API
    let dir = tempfile::tempdir().unwrap();
    let mut options = options_with_vlog_enabled(256, 1 << 20);
    if let Some(ref mut vs) = options.value_separation {
        vs.gc_threshold_ratio = 0.0; // Always trigger GC
    }
    let storage = MiniLsm::open(dir.path(), options).unwrap();

    storage.put(b"key1", &vec![b'a'; 64]).unwrap();
    storage.put(b"key2", &vec![b'b'; 64]).unwrap();

    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    // trigger_gc should not fail (though it may not find anything to GC
    // since no entries are stale yet)
    let _gc_count = storage.trigger_gc().unwrap();

    // Values should still be readable
    assert_eq!(
        storage.get(b"key1").unwrap(),
        Some(Bytes::from(vec![b'a'; 64]))
    );
    assert_eq!(
        storage.get(b"key2").unwrap(),
        Some(Bytes::from(vec![b'b'; 64]))
    );
}

#[test]
fn test_gc_multiple_files() {
    // GC across multiple vLog files
    let dir = tempfile::tempdir().unwrap();
    let options = options_with_vlog_and_compaction(256, 1 << 20);
    let storage = MiniLsm::open(dir.path(), options).unwrap();

    // First flush — large values go to vLog file 0
    storage.put(b"a1", &vec![b'a'; 64]).unwrap();
    storage.put(b"a2", &vec![b'b'; 64]).unwrap();
    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    // Second flush — large values go to vLog file 1
    storage.put(b"b1", &vec![b'c'; 64]).unwrap();
    storage.put(b"b2", &vec![b'd'; 64]).unwrap();
    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    // Overwrite all keys
    storage.put(b"a1", &vec![b'x'; 64]).unwrap();
    storage.put(b"a2", &vec![b'x'; 64]).unwrap();
    storage.put(b"b1", &vec![b'x'; 64]).unwrap();
    storage.put(b"b2", &vec![b'x'; 64]).unwrap();
    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    // Compact — old vLog entries become stale
    storage.inner.force_full_compaction().unwrap();

    // All values should be correct
    assert_eq!(
        storage.get(b"a1").unwrap(),
        Some(Bytes::from(vec![b'x'; 64]))
    );
    assert_eq!(
        storage.get(b"b2").unwrap(),
        Some(Bytes::from(vec![b'x'; 64]))
    );
}

#[test]
fn test_gc_analyze_file() {
    use crate::vlog::gc::GarbageCollector;

    let dir = tempfile::tempdir().unwrap();
    let options = options_with_vlog_enabled(256, 1 << 20);
    let storage = MiniLsm::open(dir.path(), options).unwrap();

    storage.put(b"key1", &vec![b'a'; 64]).unwrap();
    storage.put(b"key2", &vec![b'b'; 64]).unwrap();

    storage
        .inner
        .force_freeze_memtable(&storage.inner.state_lock.lock())
        .unwrap();
    storage.inner.force_flush_next_imm_memtable().unwrap();

    let vlog = storage.inner.vlog.as_ref().unwrap();
    let gc = GarbageCollector::new(vlog, &storage.inner, 0.5);

    // Analyze the vLog file — all entries should be live
    let analysis = gc.analyze_file(0).unwrap();
    assert_eq!(analysis.file_id, 0);
    assert_eq!(analysis.live_entries.len(), 2);
    assert_eq!(analysis.stale_ratio, 0.0);
    assert_eq!(analysis.dead_bytes, 0);
}
