use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::vlog::{
    ALIGNMENT, HEADER_SIZE, VLOG_MAGIC, ValuePointer, ValueSeparationOptions, VlogEntryHeader,
    VlogFileHeader,
};

/// Low-level sequential writer for a single vLog file.
///
/// Writes entries one at a time, maintaining the current file offset and size.
/// The file always starts with a 16-byte `VlogFileHeader`.
pub struct ValueLogWriter {
    file: BufWriter<File>,
    offset: u64,
    file_id: u32,
}

impl ValueLogWriter {
    /// Create a new vLog file and write the 16-byte file header.
    pub fn create(path: PathBuf, file_id: u32) -> Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .with_context(|| format!("failed to create vLog file {:?}", path))?;

        let mut writer = BufWriter::new(file);

        // Write the 16-byte VlogFileHeader
        let header = VlogFileHeader {
            magic: VLOG_MAGIC,
            version: 1,
            reserved: [0u8; 10],
        };
        let mut header_buf = [0u8; VlogFileHeader::SIZE];
        header.encode(&mut header_buf[..]);
        writer.write_all(&header_buf)?;

        Ok(Self {
            file: writer,
            offset: VlogFileHeader::SIZE as u64,
            file_id,
        })
    }

    /// Append a key-value entry to the vLog file.
    ///
    /// Returns the total number of bytes written (header + key + value + padding).
    pub fn append(&mut self, key: &[u8], value: &[u8]) -> Result<usize> {
        anyhow::ensure!(
            key.len() <= u16::MAX as usize,
            "key length {} exceeds u16 capacity",
            key.len()
        );
        anyhow::ensure!(
            value.len() <= u32::MAX as usize,
            "value length {} exceeds u32 capacity",
            value.len()
        );
        let value_crc32 = crc32fast::hash(value);

        let entry_header = VlogEntryHeader {
            header_crc32: 0, // placeholder, computed below
            value_crc32,
            value_len: value.len() as u32,
            key_len: key.len() as u16,
            flags: 0,
            _padding: [0u8; 8],
        };

        let header_crc32 = entry_header.compute_header_crc(key);

        let final_header = VlogEntryHeader {
            header_crc32,
            ..entry_header
        };

        // Serialize the header to a stack-allocated array
        let mut header_buf = [0u8; HEADER_SIZE];
        final_header.encode(&mut header_buf[..]);

        // Compute total entry size with alignment padding (overflow-safe)
        let total = VlogEntryHeader::compute_entry_size(key.len(), value.len())
            .context("entry size overflow")?;
        let padding = total - HEADER_SIZE - key.len() - value.len();

        // Write: header + key + value + padding
        self.file.write_all(&header_buf)?;
        self.file.write_all(key)?;
        self.file.write_all(value)?;
        if padding > 0 {
            self.file.write_all(&[0u8; 8][..padding])?;
        }

        self.offset += total as u64;

        Ok(total)
    }

    /// Current write offset within the file.
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Total bytes written so far (equivalent to offset after the file header).
    pub fn size(&self) -> u64 {
        self.offset
    }

    /// The file ID of this vLog file.
    pub fn file_id(&self) -> u32 {
        self.file_id
    }

    /// Flush all buffered data and sync to disk.
    pub fn close(mut self) -> Result<()> {
        self.file.flush()?;
        self.file.get_ref().sync_data()?;
        Ok(())
    }
}

/// Builder for constructing vLog entries during SST construction.
///
/// Owned by `SsTableBuilder`, this wraps a `ValueLogWriter` and validates
/// entries before writing them.
pub struct ValueLogBuilder {
    writer: ValueLogWriter,
    file_id: u32,
    options: ValueSeparationOptions,
}

impl ValueLogBuilder {
    /// Create a new `ValueLogBuilder` for the given file.
    pub fn create(path: PathBuf, file_id: u32, options: ValueSeparationOptions) -> Result<Self> {
        let writer = ValueLogWriter::create(path, file_id)?;
        Ok(Self {
            writer,
            file_id,
            options,
        })
    }

    /// Add a key-value pair to the vLog. Returns a `ValuePointer`.
    ///
    /// Validates key and value sizes before writing. The on-disk entry is
    /// padded to an 8-byte boundary, and the total size (including padding)
    /// is recorded in the returned `ValuePointer`.
    pub fn add(&mut self, key: &[u8], value: &[u8]) -> Result<ValuePointer> {
        let offset = self.writer.offset();

        // Validate before writing to avoid corrupting the vLog
        anyhow::ensure!(
            key.len() <= u16::MAX as usize,
            "key length {} exceeds vLog header u16 capacity",
            key.len()
        );
        anyhow::ensure!(
            value.len() <= self.options.max_value_size,
            "value length {} exceeds max_value_size {}",
            value.len(),
            self.options.max_value_size
        );
        let total = VlogEntryHeader::compute_entry_size(key.len(), value.len())
            .context("entry size overflow")?;
        anyhow::ensure!(
            total <= u32::MAX as usize,
            "vLog entry size {} exceeds u32 capacity",
            total
        );

        let written = self.writer.append(key, value)?;
        debug_assert_eq!(written, total);
        debug_assert_eq!(self.writer.offset() % ALIGNMENT as u64, 0);

        Ok(ValuePointer {
            file_id: self.file_id,
            offset,
            size: total as u32,
        })
    }

    /// The file ID of this builder's vLog file.
    pub fn file_id(&self) -> u32 {
        self.file_id
    }

    /// Flush and sync the underlying file to disk.
    pub fn close(self) -> Result<()> {
        self.writer.close()
    }
}
