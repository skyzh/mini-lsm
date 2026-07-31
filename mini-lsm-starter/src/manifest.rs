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

#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Result, ensure};
use parking_lot::{Mutex, MutexGuard};
use serde::{Deserialize, Serialize};

use crate::compact::CompactionTask;

pub struct Manifest {
    file: Arc<Mutex<File>>,
}

#[derive(Serialize, Deserialize)]
pub enum ManifestRecord {
    Flush(usize),
    NewMemtable(usize),
    Compaction(CompactionTask, Vec<usize>),
}

impl Manifest {
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        let file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .append(true)
            .open(path)?;
        file.sync_all()?;
        Ok(Self {
            file: Arc::new(Mutex::new(file)),
        })
    }

    pub fn recover(path: impl AsRef<Path>) -> Result<(Self, Vec<ManifestRecord>)> {
        let path = path.as_ref();
        let data = std::fs::read(path)?;
        let mut records = Vec::new();
        let mut offset = 0usize;
        while offset < data.len() {
            ensure!(data.len() - offset >= 4, "truncated manifest length");
            let payload_len =
                u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
            let payload_start = offset + 4;
            let payload_end = payload_start
                .checked_add(payload_len)
                .ok_or_else(|| anyhow::anyhow!("manifest payload length overflow"))?;
            let frame_end = payload_end
                .checked_add(4)
                .ok_or_else(|| anyhow::anyhow!("manifest frame length overflow"))?;
            ensure!(frame_end <= data.len(), "truncated manifest record");

            let payload = &data[payload_start..payload_end];
            let stored_checksum =
                u32::from_be_bytes(data[payload_end..frame_end].try_into().unwrap());
            ensure!(
                crc32fast::hash(payload) == stored_checksum,
                "manifest checksum mismatch"
            );
            records.push(serde_json::from_slice(payload)?);
            offset = frame_end;
        }

        let file = OpenOptions::new().read(true).append(true).open(path)?;
        Ok((
            Self {
                file: Arc::new(Mutex::new(file)),
            },
            records,
        ))
    }

    pub fn add_record(
        &self,
        _state_lock_observer: &MutexGuard<()>,
        record: ManifestRecord,
    ) -> Result<()> {
        self.add_record_when_init(record)
    }

    pub fn add_record_when_init(&self, record: ManifestRecord) -> Result<()> {
        let payload = serde_json::to_vec(&record)?;
        ensure!(
            payload.len() <= u32::MAX as usize,
            "manifest record is too large"
        );
        let mut frame = Vec::with_capacity(4 + payload.len() + 4);
        frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        frame.extend_from_slice(&payload);
        frame.extend_from_slice(&crc32fast::hash(&payload).to_be_bytes());

        let mut file = self.file.lock();
        file.write_all(&frame)?;
        file.sync_all()?;
        Ok(())
    }
}
