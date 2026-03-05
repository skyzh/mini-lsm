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

use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::{fs::File, io::Write};

use anyhow::{Context, Ok, Result};

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
    /// (task, new_sst_ids)
    Compaction(CompactionTask, Vec<usize>),
}

// TODO: base on size or interval take snapshot of manifest in MANIFEST_SNAPSHOT file. after that, write to a new manifest file,
// and gc old MANIFEST/MANIFEST_SNAPSHOT files in background, recove change to ManiFEST_SNAPSHOT + redo MNIFEST file
impl Manifest {
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        let f = File::create_new(path.as_ref()).context("failed to create manifest")?;

        Ok(Self {
            file: Arc::new(Mutex::new(f)),
        })
    }

    pub fn recover(path: impl AsRef<Path>) -> Result<(Self, Vec<ManifestRecord>)> {
        let mut f = File::options()
            .read(true)
            .append(true)
            .open(path.as_ref())
            .context("failed to open recover manifest")?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;

        let mut ret = vec![];
        let records = serde_json::Deserializer::from_slice(&buf).into_iter::<ManifestRecord>();
        for record in records {
            ret.push(record?);
        }

        Ok((
            Self {
                file: Arc::new(Mutex::new(f)),
            },
            ret,
        ))
    }

    /// take a record of the changes in the LsmStorageState
    pub fn add_record(
        &self,
        _state_lock_observer: &MutexGuard<()>,
        record: ManifestRecord,
    ) -> Result<()> {
        self.add_record_when_init(record)
    }

    pub fn add_record_when_init(&self, record: ManifestRecord) -> Result<()> {
        let mut file = self.file.lock();
        let buf = serde_json::to_vec(&record)?;
        file.write_all(buf.as_slice())?;

        file.sync_all().context("failed to sync manifest")
    }
}
