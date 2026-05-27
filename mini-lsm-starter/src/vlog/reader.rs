#[cfg(not(unix))]
compile_error!("vLog reader currently requires Unix platforms");

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::os::unix::fs::FileExt;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use bytes::Buf;

use super::{HEADER_SIZE, ValuePointer, VlogEntry, VlogEntryHeader, VlogFileHeader};

/// Lightweight header-only entry metadata for GC analysis.
/// Contains the pointer, key, and value length without reading the value payload.
pub struct VlogEntryMeta {
    pub ptr: ValuePointer,
    pub key: Vec<u8>,
    pub value_len: u32,
    pub entry_size: usize,
}

/// Random-read vLog file reader.
///
/// Uses positional reads (`read_exact_at`) for concurrent, lock-free
/// random access on Unix platforms.
pub struct ValueLogReader {
    file: File,
    path: PathBuf,
    file_id: u32,
}

impl ValueLogReader {
    /// Open a vLog file and validate its file header.
    /// Reads and verifies the 16-byte `VlogFileHeader` at offset 0.
    pub fn open(path: PathBuf) -> Result<Self> {
        let mut file =
            File::open(&path).with_context(|| format!("failed to open vLog file {:?}", path))?;
        let mut header_buf = [0u8; VlogFileHeader::SIZE];
        file.read_exact(&mut header_buf)
            .with_context(|| format!("failed to read header of vLog file {:?}", path))?;
        VlogFileHeader::decode(&header_buf)
            .with_context(|| format!("failed to decode header of vLog file {:?}", path))?;
        Ok(Self {
            file,
            path,
            file_id: 0,
        })
    }

    /// Set the file ID for generated `ValuePointer`s.
    pub fn with_file_id(mut self, file_id: u32) -> Self {
        self.file_id = file_id;
        self
    }

    /// Read a single entry at the given offset with the given size.
    ///
    /// - Seeks to `offset`, reads `size` bytes
    /// - Parses the 24-byte `VlogEntryHeader`, then key and value
    /// - Validates `header_crc32` and `value_crc32`
    /// - Returns a `VlogEntry` with ptr, key, value, size
    #[allow(clippy::manual_is_multiple_of)]
    pub fn read_entry(&self, offset: u64, size: u32) -> Result<VlogEntry> {
        let size = size as usize;
        anyhow::ensure!(
            offset >= VlogFileHeader::SIZE as u64,
            "offset {} is within the file header region",
            offset
        );
        anyhow::ensure!(
            offset % super::ALIGNMENT as u64 == 0,
            "offset {} is not aligned to {} bytes",
            offset,
            super::ALIGNMENT
        );
        anyhow::ensure!(
            size >= HEADER_SIZE,
            "entry size {} is smaller than header size {}",
            size,
            HEADER_SIZE
        );

        // Guard against OOM from corrupted pointers before allocating.
        const MAX_ENTRY_SIZE: usize = 512 * 1024 * 1024;
        anyhow::ensure!(
            size <= MAX_ENTRY_SIZE,
            "entry size {} exceeds maximum allowed size ({})",
            size,
            MAX_ENTRY_SIZE
        );
        // Read the entire entry in a single system call to minimize I/O overhead.
        // If the entry is past EOF, `read_exact_at` will fail with UnexpectedEof.
        let mut buf = vec![0u8; size];
        self.file.read_exact_at(&mut buf, offset).map_err(|e| {
            anyhow!(
                "failed to read entry of size {} at offset {} from {:?}: {}",
                size,
                offset,
                self.path,
                e
            )
        })?;

        let mut hdr_bytes = &buf[..HEADER_SIZE];
        let header_crc32 = hdr_bytes.get_u32_le();
        let value_crc32 = hdr_bytes.get_u32_le();
        let value_len = hdr_bytes.get_u32_le() as usize;
        let key_len = hdr_bytes.get_u16_le() as usize;
        let flags = hdr_bytes.get_u16_le();
        let mut padding = [0u8; 8];
        hdr_bytes.copy_to_slice(&mut padding);

        // Validate that the caller-supplied size matches the expected aligned entry size
        let expected_size = VlogEntryHeader::compute_entry_size(key_len, value_len)
            .ok_or_else(|| anyhow!("entry size overflow"))?;
        anyhow::ensure!(
            size == expected_size,
            "entry size mismatch: expected {}, got {}",
            expected_size,
            size
        );

        let key_start = HEADER_SIZE;
        let key_end = key_start + key_len;
        let value_end = key_end + value_len;
        let key = buf[key_start..key_end].to_vec();
        let value = buf[key_end..value_end].to_vec();

        // Validate header CRC32: covers value_crc32 + value_len + key_len + flags + padding + key
        let entry_header = VlogEntryHeader {
            header_crc32,
            value_crc32,
            value_len: value_len as u32,
            key_len: key_len as u16,
            flags,
            _padding: padding,
        };
        let computed_header_crc = entry_header.compute_header_crc(&key);
        anyhow::ensure!(
            computed_header_crc == header_crc32,
            "header CRC32 mismatch: computed 0x{:08X}, stored 0x{:08X} at offset {}",
            computed_header_crc,
            header_crc32,
            offset
        );

        // Validate value CRC32
        let computed_value_crc = crc32fast::hash(&value);
        anyhow::ensure!(
            computed_value_crc == value_crc32,
            "value CRC32 mismatch: computed 0x{:08X}, stored 0x{:08X} at offset {}",
            computed_value_crc,
            value_crc32,
            offset
        );

        Ok(VlogEntry {
            ptr: ValuePointer {
                file_id: self.file_id,
                offset,
                size: size as u32,
            },
            key,
            value,
            size,
        })
    }

    /// Return an iterator that yields `VlogEntryMeta` for each entry,
    /// reading only headers + keys (skipping values for efficiency).
    pub fn iter_headers(&self) -> Result<VlogHeaderIterator> {
        // Open an independent file descriptor so the iterator has its own
        // file offset, avoiding corruption when multiple iterators run
        // concurrently. Wrap it in a BufReader for efficient sequential reads.
        let mut file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(VlogFileHeader::SIZE as u64))?;
        let file_size = file.metadata()?.len();
        Ok(VlogHeaderIterator {
            reader: BufReader::new(file),
            offset: VlogFileHeader::SIZE as u64,
            file_size,
            file_id: self.file_id,
        })
    }
}

/// Header-only iterator for GC analysis.
/// Reads only the header and key for each entry, skipping value payloads.
pub struct VlogHeaderIterator {
    reader: BufReader<File>,
    offset: u64,
    file_size: u64,
    file_id: u32,
}

impl VlogHeaderIterator {
    /// Set the file ID for generated `ValuePointer`s.
    pub fn with_file_id(mut self, file_id: u32) -> Self {
        self.file_id = file_id;
        self
    }
}

impl Iterator for VlogHeaderIterator {
    type Item = Result<VlogEntryMeta>;

    fn next(&mut self) -> Option<Self::Item> {
        // Stop at EOF
        if self.offset >= self.file_size {
            return None;
        }

        let result = (|| -> Result<VlogEntryMeta> {
            // Read the 24-byte entry header sequentially
            let mut hdr_buf = [0u8; HEADER_SIZE];
            self.reader.read_exact(&mut hdr_buf)?;

            let mut hdr_bytes: &[u8] = &hdr_buf;
            let header_crc32 = hdr_bytes.get_u32_le();
            let value_crc32 = hdr_bytes.get_u32_le();
            let value_len = hdr_bytes.get_u32_le();
            let key_len = hdr_bytes.get_u16_le() as usize;
            let flags = hdr_bytes.get_u16_le();
            let mut padding = [0u8; 8];
            hdr_bytes.copy_to_slice(&mut padding);

            // Compute total entry size with alignment padding
            let entry_size = VlogEntryHeader::compute_entry_size(key_len, value_len as usize)
                .ok_or_else(|| anyhow!("entry size overflow"))? as u64;

            anyhow::ensure!(entry_size <= u32::MAX as u64, "entry size exceeds u32::MAX");
            let next_offset = self
                .offset
                .checked_add(entry_size)
                .ok_or_else(|| anyhow!("offset overflow"))?;
            anyhow::ensure!(next_offset <= self.file_size, "entry extends past EOF");

            // Read only the key bytes (skip the value payload for efficiency).
            let mut key = vec![0u8; key_len];
            self.reader.read_exact(&mut key)?;

            // Skip value + padding to position cursor for the next entry.
            let skip = entry_size as i64 - HEADER_SIZE as i64 - key_len as i64;
            if skip > 0 {
                self.reader.seek(SeekFrom::Current(skip))?;
            }

            // Validate header CRC32
            let entry_header = VlogEntryHeader {
                header_crc32,
                value_crc32,
                value_len,
                key_len: key_len as u16,
                flags,
                _padding: padding,
            };
            let computed_header_crc = entry_header.compute_header_crc(&key);
            anyhow::ensure!(
                computed_header_crc == header_crc32,
                "header CRC32 mismatch: computed 0x{:08X}, stored 0x{:08X} at offset {}",
                computed_header_crc,
                header_crc32,
                self.offset
            );

            let current_offset = self.offset;
            self.offset = next_offset;

            Ok(VlogEntryMeta {
                ptr: ValuePointer {
                    file_id: self.file_id,
                    offset: current_offset,
                    size: entry_size as u32,
                },
                key,
                value_len,
                entry_size: entry_size as usize,
            })
        })();

        if result.is_err() {
            // Prevent infinite loop on corruption / I/O error.
            self.offset = self.file_size;
        }

        Some(result)
    }
}
