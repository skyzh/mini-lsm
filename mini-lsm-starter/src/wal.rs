#![allow(dead_code)] // REMOVE THIS LINE after fully implementing this functionality

use crate::key::{KeyBytes, KeySlice};
use anyhow::Result;
use bytes::Bytes;
use bytes::{Buf, BufMut};
use crossbeam_skiplist::SkipMap;
use parking_lot::Mutex;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::sync::Arc;
const USIZE_BYTES: usize = std::mem::size_of::<usize>();

pub struct Wal {
    file: Arc<Mutex<BufWriter<File>>>,
}

impl Wal {
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            file: Arc::new(Mutex::new(BufWriter::new(File::create(path.as_ref())?))),
        })
    }

    pub fn recover(path: impl AsRef<Path>, skiplist: &SkipMap<KeyBytes, Bytes>) -> Result<Self> {
        let mut buffer = Vec::new();
        let file = OpenOptions::new()
            .read(true) // Allow reading
            .write(true) // Allow writing
            .create(false) // Ensure it does NOT create a new file
            .open(path)?;

        let mut buf_reader = BufReader::new(&file);
        buf_reader.read_to_end(&mut buffer)?;

        let len = buffer.len();
        let mut i = 0;
        while i < len {
            let key_len = usize::from_be_bytes(buffer[i..i + USIZE_BYTES].try_into().unwrap());
            i += USIZE_BYTES;
            let key = Bytes::copy_from_slice(&buffer[i..i + key_len][..]);
            i += key_len;
            let key_ts = (&buffer[i..i + 8]).get_u64(); // Position auto advances
            i += 8;
            let value_len = usize::from_be_bytes(buffer[i..i + USIZE_BYTES].try_into().unwrap());
            i += USIZE_BYTES;
            let value = Bytes::copy_from_slice(&buffer[i..i + value_len][..]);
            i += value_len;
            let checksum = u32::from_be_bytes(buffer[i..i + 4].try_into().unwrap());
            if checksum
                != crc32fast::hash(&buffer[i - (2 * USIZE_BYTES + key_len + value_len + 8)..i])
            {
                panic!("The checksum of WAL does not match");
            }
            i += 4;
            skiplist.insert(KeyBytes::from_bytes_with_ts(key, key_ts), value);
        }

        Ok(Self {
            file: Arc::new(Mutex::new(BufWriter::new(file))),
        })
    }

    pub fn put(&self, key: KeySlice, value: &[u8]) -> Result<()> {
        let key_len = key.key_len();
        let value_len = value.len();

        {
            let mut buf_writer = self.file.lock();
            let mut buf = Vec::<u8>::new();
            buf.extend(&key_len.to_be_bytes());
            buf.extend(key.key_ref());
            buf.put_u64(key.ts());
            buf.extend(&value_len.to_be_bytes());
            buf.extend(value);
            // Write the checksum
            buf.extend(crc32fast::hash(&buf).to_be_bytes());
            buf_writer.write_all(&buf)?;
        }
        Ok(())
    }

    /// Implement this in week 3, day 5.
    pub fn put_batch(&self, _data: &[(&[u8], &[u8])]) -> Result<()> {
        unimplemented!()
    }

    pub fn sync(&self) -> Result<()> {
        let mut file_guard = self.file.lock();
        file_guard.flush()?;
        file_guard.get_mut().sync_all()?;
        Ok(())
    }
}
