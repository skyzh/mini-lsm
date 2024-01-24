use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use bytes::{Buf, BufMut, Bytes};
use crossbeam_skiplist::SkipMap;
use parking_lot::Mutex;

use crate::key::{Key, KeyBytes, KeySlice};

pub struct Wal {
    file: Arc<Mutex<File>>,
}

impl Wal {
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            file: Arc::new(Mutex::new(
                OpenOptions::new()
                    .read(true)
                    .create_new(true)
                    .write(true)
                    .open(path)
                    .context("failed to create WAL")?,
            )),
        })
    }

    pub fn recover(path: impl AsRef<Path>, skiplist: &SkipMap<KeyBytes, Bytes>) -> Result<Self> {
        let path = path.as_ref();
        let mut file = OpenOptions::new()
            .read(true)
            .append(true)
            .open(path)
            .context("failed to recover from WAL")?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        let mut rbuf: &[u8] = buf.as_slice();
        while rbuf.has_remaining() {
            let key_len = rbuf.get_u16() as usize;
            let key = Bytes::copy_from_slice(&rbuf[..key_len]);
            rbuf.advance(key_len);
            let key_ts = rbuf.get_u64();
            let value_len = rbuf.get_u16() as usize;
            let value = Bytes::copy_from_slice(&rbuf[..value_len]);
            rbuf.advance(value_len);
            skiplist.insert(KeyBytes(key, key_ts), value);
        }
        Ok(Self {
            file: Arc::new(Mutex::new(file)),
        })
    }

    pub fn put(&self, key: KeySlice, value: &[u8]) -> Result<()> {
        let mut file = self.file.lock();
        let mut buf: Vec<u8> =
            Vec::with_capacity(key.raw_len() + value.len() + std::mem::size_of::<u16>());
        buf.put_u16(key.key_len() as u16);
        buf.put_slice(key.0);
        // only in week 3
        buf.put_u64(key.1);
        buf.put_u16(value.len() as u16);
        buf.put_slice(value);
        file.write_all(&buf)?;
        Ok(())
    }

    pub fn sync(&self) -> Result<()> {
        let file = self.file.lock();
        file.sync_all()?;
        Ok(())
    }
}
