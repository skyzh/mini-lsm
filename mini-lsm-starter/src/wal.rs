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

use crate::key::{KeyBytes, KeySlice};

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

    pub fn recover(path: impl AsRef<Path>, skiplist: &SkipMap<KeyBytes, Bytes>) -> Result<Self> {
        let path = path.as_ref();
        let data = std::fs::read(path)?;
        let mut offset = 0usize;
        let mut recovered_entries = Vec::new();
        while offset < data.len() {
            if data.len() - offset < 4 {
                anyhow::bail!("truncated WAL batch length");
            }
            let len = u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
            let end = match offset.checked_add(4 + len + 4) {
                Some(v) if v <= data.len() => v,
                _ => anyhow::bail!("truncated WAL batch frame"),
            };
            let body = &data[offset + 4..offset + 4 + len];
            let sum = u32::from_be_bytes(data[offset + 4 + len..end].try_into().unwrap());
            if crc32fast::hash(body) != sum {
                anyhow::bail!("WAL batch checksum mismatch");
            }
            let mut p = 0;
            let mut entries = Vec::new();
            while p < body.len() {
                if p + 2 > body.len() {
                    anyhow::bail!("truncated WAL key length");
                };
                let n = u16::from_be_bytes(body[p..p + 2].try_into().unwrap()) as usize;
                p += 2;
                if p + n + 8 + 2 > body.len() {
                    anyhow::bail!("truncated WAL key record");
                };
                let k = Bytes::copy_from_slice(&body[p..p + n]);
                p += n;
                let ts = u64::from_be_bytes(body[p..p + 8].try_into().unwrap());
                p += 8;
                let v = u16::from_be_bytes(body[p..p + 2].try_into().unwrap()) as usize;
                p += 2;
                if p + v > body.len() {
                    anyhow::bail!("truncated WAL value record");
                };
                entries.push((
                    KeyBytes::from_bytes_with_ts(k, ts),
                    Bytes::copy_from_slice(&body[p..p + v]),
                ));
                p += v;
            }
            recovered_entries.extend(entries);
            offset = end;
        }
        for (k, v) in recovered_entries {
            skiplist.insert(k, v);
        }

        let file = OpenOptions::new().read(true).append(true).open(path)?;
        Ok(Self {
            file: Arc::new(Mutex::new(BufWriter::new(file))),
        })
    }

    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.put_batch(&[(KeySlice::from_slice(key), value)])
    }

    /// Implement this in week 3, day 5.
    pub fn put_batch(&self, data: &[(KeySlice, &[u8])]) -> Result<()> {
        let mut body = Vec::new();
        for (key, value) in data {
            ensure!(key.key_len() <= u16::MAX as usize, "WAL key is too large");
            ensure!(value.len() <= u16::MAX as usize, "WAL value is too large");
            body.extend_from_slice(&(key.key_len() as u16).to_be_bytes());
            body.extend_from_slice(key.key_ref());
            body.extend_from_slice(&key.ts().to_be_bytes());
            body.extend_from_slice(&(value.len() as u16).to_be_bytes());
            body.extend_from_slice(value);
        }
        ensure!(body.len() <= u32::MAX as usize, "WAL batch is too large");
        let mut frame = Vec::with_capacity(4 + body.len() + 4);
        frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
        frame.extend_from_slice(&body);
        frame.extend_from_slice(&crc32fast::hash(&body).to_be_bytes());
        self.file.lock().write_all(&frame)?;
        Ok(())
    }

    pub fn sync(&self) -> Result<()> {
        let mut file = self.file.lock();
        file.flush()?;
        file.get_mut().sync_all()?;
        Ok(())
    }
}
