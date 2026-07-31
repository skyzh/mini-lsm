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

use std::path::Path;

use crossbeam_skiplist::SkipMap;
use tempfile::tempdir;

use crate::compact::CompactionOptions;
use crate::key::KeySlice;
use crate::lsm_storage::{LsmStorageOptions, MiniLsm, WriteBatchRecord};
use crate::manifest::{Manifest, ManifestRecord};
use crate::table::bloom::Bloom;
use crate::table::{FileObject, SsTable, SsTableBuilder};
use crate::wal::Wal;

fn build_test_sst(path: &Path) -> SsTable {
    let mut builder = SsTableBuilder::new(128);
    builder.add(KeySlice::from_slice(b"key"), b"value");
    builder.build_for_test(path).unwrap()
}

#[test]
fn checkpoint4_data_block_checksum_rejects_corruption() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("block.sst");
    let table = build_test_sst(&path);
    let block_offset = table.block_meta[0].offset;
    drop(table);

    let mut encoded = std::fs::read(&path).unwrap();
    encoded[block_offset] ^= 0x01;
    std::fs::write(&path, encoded).unwrap();

    let table = SsTable::open_for_test(FileObject::open(&path).unwrap()).unwrap();
    let error = table.read_block(0).err().expect("corrupt block must fail");
    assert!(error.to_string().contains("data block checksum mismatch"));
}

#[test]
fn checkpoint4_metadata_checksum_rejects_corruption() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("metadata.sst");
    let table = build_test_sst(&path);
    let metadata_offset = table.block_meta_offset;
    drop(table);

    let mut encoded = std::fs::read(&path).unwrap();
    encoded[metadata_offset] ^= 0x01;
    std::fs::write(&path, encoded).unwrap();

    let error = SsTable::open_for_test(FileObject::open(&path).unwrap())
        .err()
        .expect("corrupt metadata must fail");
    assert!(
        error
            .to_string()
            .contains("block metadata checksum mismatch")
    );
}

#[test]
fn checkpoint4_bloom_checksum_respects_section_boundary() {
    let bloom = Bloom::build_from_key_hashes(&[1, 2, 3], 10);
    let mut encoded = vec![0xaa, 0xbb, 0xcc];
    let bloom_start = encoded.len();
    bloom.encode(&mut encoded);

    encoded[0] ^= 0x01;
    Bloom::decode(&encoded[bloom_start..]).unwrap();

    let mut corrupt_bloom = encoded[bloom_start..].to_vec();
    corrupt_bloom[0] ^= 0x01;
    let error = Bloom::decode(&corrupt_bloom)
        .err()
        .expect("corrupt Bloom payload must fail");
    assert!(error.to_string().contains("Bloom checksum mismatch"));
    assert!(Bloom::decode(&corrupt_bloom[..4]).is_err());
}

#[test]
fn checkpoint4_wal_truncation_does_not_replay_record() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("truncated.wal");
    let wal = Wal::create(&path).unwrap();
    wal.put(b"key", b"value").unwrap();
    wal.sync().unwrap();
    drop(wal);

    let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_len(file.metadata().unwrap().len() - 1).unwrap();
    drop(file);

    let map = SkipMap::new();
    assert!(Wal::recover(&path, &map).is_err());
    assert!(map.is_empty());
}

#[test]
fn checkpoint4_manifest_rejects_oversized_and_truncated_frames() {
    let dir = tempdir().unwrap();
    let oversized_path = dir.path().join("oversized.manifest");
    let manifest = Manifest::create(&oversized_path).unwrap();
    manifest
        .add_record_when_init(ManifestRecord::Flush(7))
        .unwrap();
    drop(manifest);

    let valid = std::fs::read(&oversized_path).unwrap();
    let mut oversized = valid.clone();
    oversized[..4].copy_from_slice(&u32::MAX.to_be_bytes());
    std::fs::write(&oversized_path, oversized).unwrap();
    assert!(Manifest::recover(&oversized_path).is_err());

    let truncated_path = dir.path().join("truncated.manifest");
    std::fs::write(&truncated_path, &valid[..valid.len() - 1]).unwrap();
    assert!(Manifest::recover(&truncated_path).is_err());
}

#[test]
fn checkpoint4_write_batch_survives_sync_and_recovery() {
    let dir = tempdir().unwrap();
    let mut options = LsmStorageOptions::default_for_week2_test(CompactionOptions::NoCompaction);
    options.enable_wal = true;

    let storage = MiniLsm::open(&dir, options.clone()).unwrap();
    storage
        .write_batch(&[
            WriteBatchRecord::Put(b"a".to_vec(), b"1".to_vec()),
            WriteBatchRecord::Put(b"b".to_vec(), b"2".to_vec()),
            WriteBatchRecord::Del(b"a".to_vec()),
        ])
        .unwrap();
    storage.sync().unwrap();
    storage.close().unwrap();
    drop(storage);

    let storage = MiniLsm::open(&dir, options).unwrap();
    assert_eq!(storage.get(b"a").unwrap(), None);
    assert_eq!(storage.get(b"b").unwrap().as_deref(), Some(b"2".as_slice()));
    storage.close().unwrap();
}
