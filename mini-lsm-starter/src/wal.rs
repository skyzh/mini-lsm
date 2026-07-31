// REMOVE THIS LINE after fully implementing this functionality
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

use anyhow::{Result, ensure};
use bytes::Bytes;
use crossbeam_skiplist::SkipMap;
use parking_lot::Mutex;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Arc;

use crate::key::KeySlice;

pub struct Wal {
    file: Arc<Mutex<BufWriter<File>>>,
}

impl Wal {
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        let file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .append(true)
            .open(path)?;
        file.sync_all()?;
        Ok(Self {
            file: Arc::new(Mutex::new(BufWriter::new(file))),
        })
    }

    pub fn recover(path: impl AsRef<Path>, skiplist: &SkipMap<Bytes, Bytes>) -> Result<Self> {
        let path = path.as_ref();
        let data = std::fs::read(path)?;
        let mut offset = 0usize;
        while offset < data.len() {
            let record_start = offset;
            ensure!(data.len() - offset >= 2, "truncated WAL key length");
            let key_len = u16::from_be_bytes(data[offset..offset + 2].try_into().unwrap()) as usize;
            let key_start = offset + 2;
            let key_end = key_start
                .checked_add(key_len)
                .ok_or_else(|| anyhow::anyhow!("WAL key length overflow"))?;
            ensure!(
                key_end.checked_add(2).is_some_and(|end| end <= data.len()),
                "truncated WAL key"
            );

            let value_len =
                u16::from_be_bytes(data[key_end..key_end + 2].try_into().unwrap()) as usize;
            let value_start = key_end + 2;
            let value_end = value_start
                .checked_add(value_len)
                .ok_or_else(|| anyhow::anyhow!("WAL value length overflow"))?;
            let record_end = value_end
                .checked_add(4)
                .ok_or_else(|| anyhow::anyhow!("WAL record length overflow"))?;
            ensure!(record_end <= data.len(), "truncated WAL value or checksum");

            let stored_checksum =
                u32::from_be_bytes(data[value_end..record_end].try_into().unwrap());
            ensure!(
                crc32fast::hash(&data[record_start..value_end]) == stored_checksum,
                "WAL checksum mismatch"
            );
            skiplist.insert(
                Bytes::copy_from_slice(&data[key_start..key_end]),
                Bytes::copy_from_slice(&data[value_start..value_end]),
            );
            offset = record_end;
        }

        let file = OpenOptions::new().read(true).append(true).open(path)?;
        Ok(Self {
            file: Arc::new(Mutex::new(BufWriter::new(file))),
        })
    }

    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        ensure!(key.len() <= u16::MAX as usize, "WAL key is too large");
        ensure!(value.len() <= u16::MAX as usize, "WAL value is too large");
        let mut record = Vec::with_capacity(2 + key.len() + 2 + value.len() + 4);
        record.extend_from_slice(&(key.len() as u16).to_be_bytes());
        record.extend_from_slice(key);
        record.extend_from_slice(&(value.len() as u16).to_be_bytes());
        record.extend_from_slice(value);
        let checksum = crc32fast::hash(&record);
        record.extend_from_slice(&checksum.to_be_bytes());
        self.file.lock().write_all(&record)?;
        Ok(())
    }

    /// Implement this in week 3, day 5.
    pub fn put_batch(&self, _data: &[(KeySlice, &[u8])]) -> Result<()> {
        unimplemented!()
    }

    pub fn sync(&self) -> Result<()> {
        let mut file = self.file.lock();
        file.flush()?;
        file.get_mut().sync_all()?;
        Ok(())
    }
}
