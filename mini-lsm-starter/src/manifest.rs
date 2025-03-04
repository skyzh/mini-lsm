// Copyright (c) 2022-2025 Alex Chi Z
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
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use parking_lot::{Mutex, MutexGuard};
use serde::{Deserialize, Serialize};
const USIZE_CONST: usize = std::mem::size_of::<usize>();

use crate::compact::CompactionTask;

pub struct Manifest {
    file: Arc<Mutex<File>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ManifestRecord {
    Flush(usize),
    NewMemtable(usize),
    Compaction(CompactionTask, Vec<usize>),
}

impl Manifest {
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        let raw_file = OpenOptions::new()
            .read(true)
            .create_new(true)
            .write(true)
            .open(path)?;
        Ok(Self {
            file: Arc::new(Mutex::new(raw_file)),
        })
    }

    pub fn recover(path: impl AsRef<Path>) -> Result<(Self, Vec<ManifestRecord>)> {
        let mut raw_file = OpenOptions::new().read(true).append(true).open(&path)?;
        let mut buf = Vec::new();

        raw_file.read_to_end(&mut buf)?;
        let len = buf.len();
        let mut i = 0;
        let mut records = Vec::new();
        while i < len {
            let len_record = usize::from_be_bytes(buf[i..i + USIZE_CONST].try_into().unwrap());
            i += USIZE_CONST;

            let record_json = &buf[i..i + len_record];
            let record = serde_json::from_slice(record_json);
            i += len_record;

            let checksum = u32::from_be_bytes(buf[i..i + 4].try_into().unwrap());
            if checksum != crc32fast::hash(record_json) {
                panic!("Manifest checksum does not match");
            }
            i += 4;

            records.push(record?);
        }

        Ok((
            Self {
                file: Arc::new(Mutex::new(raw_file)),
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
        let mut file_guard = self.file.lock();
        let vec_record = serde_json::to_vec(&record)?;
        // Write the data into the fileguard
        file_guard.write_all(&vec_record.len().to_be_bytes())?;
        file_guard.write_all(&vec_record)?;
        file_guard.write_all(&crc32fast::hash(&vec_record).to_be_bytes())?;
        file_guard.sync_all()?;
        Ok(())
    }
}
