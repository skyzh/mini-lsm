// Copyright (c) 2022-2026 Alex Chi Z
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

use std::panic::{AssertUnwindSafe, catch_unwind};

use anyhow::Result;
use bytes::Bytes;
use tempfile::tempdir;

use crate::{
    compact::CompactionOptions,
    key::KeySlice,
    lsm_storage::{LsmStorageOptions, MiniLsm, WriteBatchRecord},
    manifest::{Manifest, ManifestRecord},
    table::{FileObject, SsTable, SsTableBuilder},
};

fn options(enable_wal: bool, serializable: bool) -> LsmStorageOptions {
    let mut options = LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction);
    options.enable_wal = enable_wal;
    options.serializable = serializable;
    options
}

fn assert_round_trip(enable_wal: bool, serializable: bool, key: &[u8], value: &[u8]) {
    let dir = tempdir().unwrap();
    let storage = MiniLsm::open(&dir, options(enable_wal, serializable)).unwrap();
    storage.put(key, value).unwrap();
    assert_eq!(
        storage.get(key).unwrap(),
        Some(Bytes::copy_from_slice(value))
    );
    storage.force_flush().unwrap();
    assert_eq!(
        storage.get(key).unwrap(),
        Some(Bytes::copy_from_slice(value))
    );
    storage.close().unwrap();
    drop(storage);

    let storage = MiniLsm::open(&dir, options(enable_wal, serializable)).unwrap();
    assert_eq!(
        storage.get(key).unwrap(),
        Some(Bytes::copy_from_slice(value))
    );
    storage.close().unwrap();
}

fn open_and_read_all_blocks(path: &std::path::Path) -> Result<()> {
    let table = SsTable::open_for_test(FileObject::open(path)?)?;
    for idx in 0..table.num_of_blocks() {
        table.read_block(idx)?;
    }
    Ok(())
}

#[test]
fn test_release_manifest_torn_tail_preserves_durable_prefix() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("MANIFEST");
    let manifest = Manifest::create(&path).unwrap();
    manifest
        .add_record_when_init(ManifestRecord::NewMemtable(1))
        .unwrap();
    manifest
        .add_record_when_init(ManifestRecord::NewMemtable(2))
        .unwrap();
    drop(manifest);

    let encoded = std::fs::read(&path).unwrap();
    let first_body_len = u64::from_be_bytes(encoded[..8].try_into().unwrap()) as usize;
    let first_frame_len = 8 + first_body_len + 4;

    for cutoff in 0..first_frame_len {
        let truncated = dir.path().join(format!("first-{cutoff}.manifest"));
        std::fs::write(&truncated, &encoded[..cutoff]).unwrap();
        let (_, records) = Manifest::recover(&truncated).unwrap();
        assert!(records.is_empty());
        assert_eq!(std::fs::metadata(truncated).unwrap().len(), 0);
    }

    for cutoff in first_frame_len + 1..encoded.len() {
        let truncated = dir.path().join(format!("second-{cutoff}.manifest"));
        std::fs::write(&truncated, &encoded[..cutoff]).unwrap();
        let (_, records) = Manifest::recover(&truncated).unwrap();
        assert_eq!(records.len(), 1);
        assert!(matches!(records[0], ManifestRecord::NewMemtable(1)));
        assert_eq!(
            std::fs::metadata(truncated).unwrap().len(),
            first_frame_len as u64
        );
    }
}

#[test]
fn test_release_manifest_corruption_never_panics() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("MANIFEST");
    let manifest = Manifest::create(&source).unwrap();
    manifest
        .add_record_when_init(ManifestRecord::NewMemtable(1))
        .unwrap();
    drop(manifest);
    let encoded = std::fs::read(source).unwrap();

    let mut checksum_corrupt = encoded.clone();
    *checksum_corrupt.last_mut().unwrap() ^= 0x80;
    let checksum_path = dir.path().join("checksum.manifest");
    std::fs::write(&checksum_path, checksum_corrupt).unwrap();
    assert!(Manifest::recover(checksum_path).is_err());

    for idx in 0..8 {
        let mut corrupt = encoded.clone();
        corrupt[idx] ^= 0x80;
        let path = dir.path().join(format!("length-{idx}.manifest"));
        std::fs::write(&path, corrupt).unwrap();
        let outcome = catch_unwind(AssertUnwindSafe(|| Manifest::recover(&path)));
        assert!(outcome.is_ok(), "length byte {idx} panicked");
    }
}

#[test]
fn test_release_sst_corruption_and_truncation_never_panic() {
    let dir = tempdir().unwrap();
    let source = dir.path().join("source.sst");
    let mut builder = SsTableBuilder::new(32);
    builder.add(KeySlice::from_slice_with_ts(b"a", 2), b"1");
    builder.add(KeySlice::from_slice_with_ts(b"b", 1), b"2");
    drop(builder.build_for_test(&source).unwrap());
    let encoded = std::fs::read(&source).unwrap();

    for cutoff in 0..encoded.len() {
        let path = dir.path().join(format!("truncated-{cutoff}.sst"));
        std::fs::write(&path, &encoded[..cutoff]).unwrap();
        let outcome = catch_unwind(AssertUnwindSafe(|| open_and_read_all_blocks(&path)));
        assert!(outcome.is_ok(), "SST cutoff {cutoff} panicked");
        assert!(
            outcome.unwrap().is_err(),
            "SST cutoff {cutoff} was accepted"
        );
    }

    for idx in 0..encoded.len() {
        let mut corrupt = encoded.clone();
        corrupt[idx] ^= 0x80;
        let path = dir.path().join(format!("corrupt-{idx}.sst"));
        std::fs::write(&path, corrupt).unwrap();
        let outcome = catch_unwind(AssertUnwindSafe(|| open_and_read_all_blocks(&path)));
        assert!(outcome.is_ok(), "SST byte {idx} panicked");
        assert!(outcome.unwrap().is_err(), "SST byte {idx} was accepted");
    }
}

#[test]
fn test_release_public_write_size_boundaries() {
    let max_key = vec![b'k'; usize::from(u16::MAX)];
    let max_value = vec![b'v'; usize::from(u16::MAX)];
    for enable_wal in [false, true] {
        for serializable in [false, true] {
            assert_round_trip(enable_wal, serializable, b"value-boundary", &max_value);
            assert_round_trip(enable_wal, serializable, &max_key, b"key-boundary");

            let dir = tempdir().unwrap();
            let storage = MiniLsm::open(&dir, options(enable_wal, serializable)).unwrap();
            let oversized = vec![0; usize::from(u16::MAX) + 1];
            assert!(storage.put(b"oversized-value", &oversized).is_err());
            assert!(storage.put(&oversized, b"oversized-key").is_err());
            let batch = [
                WriteBatchRecord::Put(b"would-be-partial".as_slice(), b"value".as_slice()),
                WriteBatchRecord::Put(b"oversized".as_slice(), oversized.as_slice()),
            ];
            assert!(storage.write_batch(&batch).is_err());
            assert_eq!(storage.get(b"would-be-partial").unwrap(), None);
            assert_eq!(storage.get(b"oversized-value").unwrap(), None);
            storage.close().unwrap();
            drop(storage);

            let storage = MiniLsm::open(&dir, options(enable_wal, serializable)).unwrap();
            assert_eq!(storage.get(b"would-be-partial").unwrap(), None);
            assert_eq!(storage.get(b"oversized-value").unwrap(), None);
            storage.close().unwrap();
        }
    }
}

#[test]
fn test_release_first_key_max_sst_restart_advances_timestamp() {
    let dir = tempdir().unwrap();
    let storage = MiniLsm::open(&dir, options(false, false)).unwrap();
    storage.put(b"b", b"ts1").unwrap();
    storage.put(b"a", b"ts2").unwrap();
    assert_eq!(storage.inner.mvcc().latest_commit_ts(), 2);
    storage.force_flush().unwrap();
    storage.close().unwrap();
    drop(storage);

    let storage = MiniLsm::open(&dir, options(false, false)).unwrap();
    assert_eq!(storage.inner.mvcc().latest_commit_ts(), 2);
    assert_eq!(storage.get(b"a").unwrap(), Some(Bytes::from_static(b"ts2")));
    storage.put(b"c", b"ts3").unwrap();
    assert_eq!(storage.inner.mvcc().latest_commit_ts(), 3);
    assert_eq!(storage.get(b"a").unwrap(), Some(Bytes::from_static(b"ts2")));
    storage.close().unwrap();
}

#[test]
fn test_release_live_wal_restart_advances_timestamp() {
    let dir = tempdir().unwrap();
    let storage = MiniLsm::open(&dir, options(true, false)).unwrap();
    storage.put(b"a", b"ts1").unwrap();
    storage.put(b"c", b"ts2").unwrap();
    storage.put(b"b", b"ts3").unwrap();
    assert_eq!(storage.inner.mvcc().latest_commit_ts(), 3);
    storage.close().unwrap();
    drop(storage);

    let storage = MiniLsm::open(&dir, options(true, false)).unwrap();
    assert_eq!(storage.inner.mvcc().latest_commit_ts(), 3);
    assert_eq!(storage.get(b"b").unwrap(), Some(Bytes::from_static(b"ts3")));
    storage.put(b"d", b"ts4").unwrap();
    assert_eq!(storage.inner.mvcc().latest_commit_ts(), 4);
    assert_eq!(storage.get(b"b").unwrap(), Some(Bytes::from_static(b"ts3")));
    storage.close().unwrap();
}
