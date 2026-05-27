pub mod builder;
pub mod reader;

pub use builder::ValueLogBuilder;
pub use reader::{ValueLogReader, VlogEntryMeta};

use anyhow::{Result, anyhow};
use bytes::{Buf, BufMut};

/// Magic number for vLog file header
const VLOG_MAGIC: u32 = 0x564C4F47; // "VLOG"

/// Per-entry header size (24 bytes)
const HEADER_SIZE: usize = 24;

/// Alignment boundary for vLog entries
const ALIGNMENT: usize = 8;

/// Magic tag byte that prefixes every encoded `ValuePointer`.
/// Retained as a cheap corruption/desync detector alongside KvKind.
const VALUE_POINTER_TAG: u8 = 0xFF;

/// Per-entry value-kind stored with every key-value entry: in the memtable, WAL,
/// and SST block metadata. This is the authoritative source of truth for
/// distinguishing inline values from vLog pointers.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KvKind {
    /// The value is stored inline in the SST block.
    Inline = 0,
    /// The value is a 17-byte encoded `ValuePointer` that references the vLog.
    ValuePointer = 1,
}

impl KvKind {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Inline),
            1 => Some(Self::ValuePointer),
            _ => None,
        }
    }
}

/// A pointer to a value stored in the Value Log.
/// Stored inline in the LSM tree instead of the actual value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValuePointer {
    /// Value log file ID
    pub file_id: u32,
    /// Offset within the file where the value starts
    pub offset: u64,
    /// Total size of the encoded entry on disk (header + key + value + padding).
    pub size: u32,
}

impl ValuePointer {
    /// Encode to bytes for storage in LSM tree.
    /// Layout (17 bytes): `[tag:1][file_id:4][offset:8][size:4]`
    pub fn encode(&self, mut buf: impl BufMut) {
        buf.put_u8(VALUE_POINTER_TAG);
        buf.put_u32_le(self.file_id);
        buf.put_u64_le(self.offset);
        buf.put_u32_le(self.size);
    }

    /// Decode from bytes. Returns an error if the buffer is malformed.
    pub fn decode(mut buf: &[u8]) -> Result<Self> {
        if buf.len() < Self::encoded_size() {
            return Err(anyhow!(
                "ValuePointer buffer too short: {} < {}",
                buf.len(),
                Self::encoded_size()
            ));
        }
        let tag = buf.get_u8();
        if tag != VALUE_POINTER_TAG {
            return Err(anyhow!(
                "ValuePointer tag mismatch: expected 0x{:02X}, got 0x{:02X}",
                VALUE_POINTER_TAG,
                tag
            ));
        }
        Ok(Self {
            file_id: buf.get_u32_le(),
            offset: buf.get_u64_le(),
            size: buf.get_u32_le(),
        })
    }

    /// Try to decode from bytes. Returns `None` if the buffer is too short or
    /// does not start with the `VALUE_POINTER_TAG` byte.
    pub fn try_decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < Self::encoded_size() || buf[0] != VALUE_POINTER_TAG {
            return None;
        }
        let mut b = &buf[1..];
        Some(Self {
            file_id: b.get_u32_le(),
            offset: b.get_u64_le(),
            size: b.get_u32_le(),
        })
    }

    /// Total encoded size: 17 bytes (1-byte tag + 4 + 8 + 4)
    pub const fn encoded_size() -> usize {
        1 + 4 + 8 + 4
    }
}

/// Configuration options for key-value separation.
#[derive(Clone, Debug)]
pub struct ValueSeparationOptions {
    /// Enable key-value separation
    pub enabled: bool,
    /// Minimum value size to trigger separation (bytes)
    pub min_value_size: usize,
    /// Maximum size of a single value (bytes)
    pub max_value_size: usize,
    /// Maximum size of a single vLog file
    pub max_vlog_file_size: usize,
    /// Ratio of stale data to trigger garbage collection
    pub gc_threshold_ratio: f64,
    /// Maximum number of vLog files to keep open
    pub max_open_vlog_files: usize,
}

impl Default for ValueSeparationOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            min_value_size: 1024,
            max_value_size: 128 << 20,
            max_vlog_file_size: 64 << 20,
            gc_threshold_ratio: 0.5,
            max_open_vlog_files: 64,
        }
    }
}

/// Value log file header (first 16 bytes of each vLog file).
/// Serialized/deserialized field-by-field with explicit little-endian encoding.
pub struct VlogFileHeader {
    pub magic: u32,
    pub version: u16,
    pub reserved: [u8; 10],
}

impl VlogFileHeader {
    pub const SIZE: usize = 16;

    pub fn encode(&self, mut buf: impl BufMut) {
        buf.put_u32_le(self.magic);
        buf.put_u16_le(self.version);
        buf.put_slice(&self.reserved);
    }

    pub fn decode(mut buf: &[u8]) -> Result<Self> {
        if buf.len() < Self::SIZE {
            return Err(anyhow!("VlogFileHeader too short"));
        }
        let magic = buf.get_u32_le();
        if magic != VLOG_MAGIC {
            return Err(anyhow!("VlogFileHeader magic mismatch: 0x{:08X}", magic));
        }
        let version = buf.get_u16_le();
        anyhow::ensure!(version == 1, "unsupported vLog version: {}", version);
        let mut reserved = [0u8; 10];
        buf.copy_to_slice(&mut reserved);
        Ok(Self {
            magic,
            version,
            reserved,
        })
    }
}

/// Entry header (precedes each key-value pair in the vLog).
/// Always exactly 24 bytes. Serialized field-by-field.
pub struct VlogEntryHeader {
    pub header_crc32: u32,
    pub value_crc32: u32,
    pub value_len: u32,
    pub key_len: u16,
    pub flags: u16,
    pub _padding: [u8; 8],
}

impl VlogEntryHeader {
    pub const fn size() -> usize {
        HEADER_SIZE
    }

    /// Serialize the header to bytes (24 bytes, little-endian).
    pub fn encode(&self, mut buf: impl BufMut) {
        buf.put_u32_le(self.header_crc32);
        buf.put_u32_le(self.value_crc32);
        buf.put_u32_le(self.value_len);
        buf.put_u16_le(self.key_len);
        buf.put_u16_le(self.flags);
        buf.put_slice(&self._padding);
    }

    /// Compute the total entry size including header, key, value, and alignment padding.
    pub fn compute_entry_size(key_len: usize, value_len: usize) -> Option<usize> {
        let entry_size = HEADER_SIZE.checked_add(key_len)?.checked_add(value_len)?;
        let padding = (ALIGNMENT - (entry_size % ALIGNMENT)) % ALIGNMENT;
        entry_size.checked_add(padding)
    }

    /// Compute the CRC32 over (header_without_header_crc + key_bytes).
    /// The header_crc32 field itself is excluded from the CRC.
    pub fn compute_header_crc(&self, key: &[u8]) -> u32 {
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(&self.value_crc32.to_le_bytes());
        hasher.update(&self.value_len.to_le_bytes());
        hasher.update(&self.key_len.to_le_bytes());
        hasher.update(&self.flags.to_le_bytes());
        hasher.update(&self._padding);
        hasher.update(key);
        hasher.finalize()
    }
}

/// A single entry read from a vLog file.
pub struct VlogEntry {
    pub ptr: ValuePointer,
    pub key: Vec<u8>,
    pub value: Vec<u8>,
    pub size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vlog::builder::ValueLogWriter;
    use crate::vlog::reader::ValueLogReader;

    // ---------------------------------------------------------------
    // 1. ValuePointer encode/decode round-trip
    // ---------------------------------------------------------------
    #[test]
    fn test_value_pointer_encode_decode() {
        let cases = [
            ValuePointer {
                file_id: 0,
                offset: 0,
                size: 0,
            },
            ValuePointer {
                file_id: 42,
                offset: 1024,
                size: 256,
            },
            ValuePointer {
                file_id: u32::MAX,
                offset: u64::MAX,
                size: u32::MAX,
            },
        ];

        for ptr in &cases {
            let mut buf = Vec::new();
            ptr.encode(&mut buf);
            assert_eq!(buf.len(), ValuePointer::encoded_size());
            assert_eq!(buf.len(), 17);
            assert_eq!(buf[0], VALUE_POINTER_TAG);

            let decoded = ValuePointer::decode(&buf).unwrap();
            assert_eq!(*ptr, decoded);

            // Extra trailing bytes should be ignored.
            let mut extended = buf.clone();
            extended.extend_from_slice(&[0xAB; 32]);
            let decoded2 = ValuePointer::decode(&extended).unwrap();
            assert_eq!(*ptr, decoded2);
        }
    }

    // ---------------------------------------------------------------
    // 2. ValuePointer try_decode edge cases
    // ---------------------------------------------------------------
    #[test]
    fn test_value_pointer_try_decode() {
        let ptr = ValuePointer {
            file_id: 7,
            offset: 999,
            size: 128,
        };
        let mut buf = Vec::new();
        ptr.encode(&mut buf);

        // Valid data
        assert_eq!(ValuePointer::try_decode(&buf), Some(ptr));

        // Short buffer (< 17 bytes)
        for len in 0..ValuePointer::encoded_size() {
            assert_eq!(ValuePointer::try_decode(&buf[..len]), None);
        }

        // Correct length but wrong tag byte
        let mut bad_tag = buf.clone();
        bad_tag[0] = 0x00;
        assert_eq!(ValuePointer::try_decode(&bad_tag), None);

        let mut bad_tag2 = buf.clone();
        bad_tag2[0] = 0xFE;
        assert_eq!(ValuePointer::try_decode(&bad_tag2), None);
    }

    // ---------------------------------------------------------------
    // 3. KvKind::from_u8
    // ---------------------------------------------------------------
    #[test]
    fn test_kv_kind_from_u8() {
        assert_eq!(KvKind::from_u8(0), Some(KvKind::Inline));
        assert_eq!(KvKind::from_u8(1), Some(KvKind::ValuePointer));

        for v in [2u8, 3, 100, 254, 255] {
            assert_eq!(
                KvKind::from_u8(v),
                None,
                "KvKind::from_u8({v}) should be None"
            );
        }
    }

    // ---------------------------------------------------------------
    // 4. VlogEntryHeader::compute_header_crc determinism + coverage
    // ---------------------------------------------------------------
    #[test]
    fn test_vlog_entry_header_crc() {
        let key = b"test_key_for_crc";

        let hdr = VlogEntryHeader {
            header_crc32: 0,
            value_crc32: 0xDEADBEEF,
            value_len: 4096,
            key_len: 16,
            flags: 1,
            _padding: [0xAA; 8],
        };
        let crc1 = hdr.compute_header_crc(key);

        // Deterministic: same inputs produce same CRC.
        let crc2 = hdr.compute_header_crc(key);
        assert_eq!(crc1, crc2);

        // Different key -> different CRC.
        let crc_different_key = hdr.compute_header_crc(b"other_key");
        assert_ne!(crc1, crc_different_key);

        // Changing each covered field must change the CRC.
        let h = VlogEntryHeader {
            value_crc32: 0xDEADBEEF ^ 1,
            ..hdr
        };
        assert_ne!(h.compute_header_crc(key), crc1);

        let h = VlogEntryHeader {
            value_len: 4096 ^ 1,
            ..hdr
        };
        assert_ne!(h.compute_header_crc(key), crc1);

        let h = VlogEntryHeader {
            key_len: 16 ^ 1,
            ..hdr
        };
        assert_ne!(h.compute_header_crc(key), crc1);

        let h = VlogEntryHeader {
            flags: 1 ^ 1,
            ..hdr
        };
        assert_ne!(h.compute_header_crc(key), crc1);

        let mut different_padding = [0xAA; 8];
        different_padding[0] ^= 0xFF;
        let h = VlogEntryHeader {
            _padding: different_padding,
            ..hdr
        };
        assert_ne!(h.compute_header_crc(key), crc1);

        // header_crc32 itself is excluded from the CRC computation.
        let h = VlogEntryHeader {
            header_crc32: 0x12345678,
            ..hdr
        };
        assert_eq!(h.compute_header_crc(key), crc1);
    }

    // ---------------------------------------------------------------
    // 5. Write + read round-trip using ValueLogWriter / ValueLogReader
    // ---------------------------------------------------------------
    #[test]
    fn test_vlog_write_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("00001.vlog");
        let file_id = 1u32;

        let entries: Vec<(&[u8], &[u8])> = vec![
            (b"key1", b"value1"),
            (b"key2", b"value2_value2"),
            (b"longer_key_name", b"short"),
            (b"k", b"another_value_here"),
        ];

        // Write entries with ValueLogWriter
        let mut writer = ValueLogWriter::create(path.clone(), file_id).unwrap();
        let mut pointers = Vec::new();
        for (k, v) in &entries {
            let offset = writer.offset();
            let total = writer.append(k, v).unwrap();
            pointers.push(ValuePointer {
                file_id,
                offset,
                size: total as u32,
            });
        }
        writer.close().unwrap();

        // Read back with ValueLogReader
        let reader = ValueLogReader::open(path).unwrap();
        for (i, (expected_key, expected_value)) in entries.iter().enumerate() {
            let entry = reader
                .read_entry(pointers[i].offset, pointers[i].size)
                .unwrap();
            assert_eq!(entry.key, *expected_key);
            assert_eq!(entry.value, *expected_value);
        }
    }

    // ---------------------------------------------------------------
    // 6. 8-byte alignment (using ValueLogWriter)
    // ---------------------------------------------------------------
    #[test]
    fn test_vlog_alignment() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("align.vlog");

        let mut writer = ValueLogWriter::create(path, 0).unwrap();

        // After the 16-byte file header, the writer offset starts at 16.
        assert_eq!(writer.offset() as usize % ALIGNMENT, 0);

        let test_cases: Vec<(&[u8], &[u8])> = vec![
            (b"k1", b"v"),         // 24 + 2 + 1 = 27 -> pad to 32
            (b"key2", b"value"),   // 24 + 4 + 6 = 34 -> pad to 40
            (b"k", b"0123456789"), // 24 + 1 + 10 = 35 -> pad to 40
        ];

        for (key, value) in &test_cases {
            let total = writer.append(key, value).unwrap();

            // Total must be a multiple of 8.
            assert_eq!(
                total % ALIGNMENT,
                0,
                "entry for key={:?} wrote {} bytes, not 8-byte aligned",
                key,
                total
            );

            // Writer offset must remain 8-byte aligned.
            assert_eq!(writer.offset() as usize % ALIGNMENT, 0);

            // Verify the written size matches expected padding.
            let expected_raw = HEADER_SIZE + key.len() + value.len();
            let expected_pad = (ALIGNMENT - (expected_raw % ALIGNMENT)) % ALIGNMENT;
            assert_eq!(total, expected_raw + expected_pad);
        }
    }

    // ---------------------------------------------------------------
    // 7. Large entry (10 KB value)
    // ---------------------------------------------------------------
    #[test]
    fn test_vlog_large_entry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large.vlog");
        let file_id = 5u32;

        let key = b"big";
        let value = vec![0xAB_u8; 10 * 1024];

        let mut writer = ValueLogWriter::create(path.clone(), file_id).unwrap();
        let offset = writer.offset();
        let total = writer.append(key, &value).unwrap();
        writer.close().unwrap();

        let reader = ValueLogReader::open(path).unwrap();
        let entry = reader.read_entry(offset, total as u32).unwrap();

        assert_eq!(entry.key, key);
        assert_eq!(entry.value.len(), 10 * 1024);
        assert!(entry.value.iter().all(|&b| b == 0xAB));
        assert_eq!(entry.ptr.size as usize, entry.size);
        assert_eq!(entry.size % ALIGNMENT, 0);
    }

    // ---------------------------------------------------------------
    // 8. Header-only iterator via iter_headers()
    // ---------------------------------------------------------------
    #[test]
    fn test_vlog_header_iterator() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("iter.vlog");
        let file_id = 3u32;

        let entries: Vec<(&[u8], &[u8])> =
            vec![(b"alpha", b"aaa"), (b"beta", b"bbbbbb"), (b"gamma", b"g")];

        // Write entries
        let mut writer = ValueLogWriter::create(path.clone(), file_id).unwrap();
        let mut expected_meta = Vec::new();
        for (k, v) in &entries {
            let offset = writer.offset();
            let total = writer.append(k, v).unwrap();
            expected_meta.push((k.to_vec(), total, offset));
        }
        writer.close().unwrap();

        // Iterate headers
        let reader = ValueLogReader::open(path).unwrap();
        let iter = reader.iter_headers().unwrap().with_file_id(file_id);
        let meta_list: Vec<_> = iter.map(|r| r.unwrap()).collect();

        assert_eq!(meta_list.len(), entries.len());
        for (i, meta) in meta_list.iter().enumerate() {
            assert_eq!(meta.key, entries[i].0, "key mismatch at index {i}");
            assert_eq!(
                meta.ptr.offset, expected_meta[i].2,
                "offset mismatch at index {i}"
            );
            assert_eq!(
                meta.entry_size, expected_meta[i].1,
                "size mismatch at index {i}"
            );
            assert_eq!(
                meta.value_len,
                entries[i].1.len() as u32,
                "value_len mismatch at index {i}"
            );
        }
    }

    // ---------------------------------------------------------------
    // 9. CRC mutation detection
    // ---------------------------------------------------------------
    #[test]
    fn test_vlog_crc_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crc_mut.vlog");
        let file_id = 9u32;

        let key = b"crc_test";
        let value = b"original_value";

        // Write one entry.
        let mut writer = ValueLogWriter::create(path.clone(), file_id).unwrap();
        let offset = writer.offset();
        let total = writer.append(key, value).unwrap();
        writer.close().unwrap();

        // Read should succeed before corruption.
        let reader = ValueLogReader::open(path.clone()).unwrap();
        reader
            .read_entry(offset, total as u32)
            .expect("read should succeed on clean data");

        // Corrupt a byte in the value region on disk.
        // On-disk layout: [file_hdr(16)] [entry_hdr(24)] [key(8)] [value(14)] [padding]
        // Value starts at 16 + 24 + 8 = 48.
        let mut raw = std::fs::read(&path).unwrap();
        let value_start = VlogFileHeader::SIZE + HEADER_SIZE + key.len();
        assert_eq!(raw[value_start], value[0]);
        raw[value_start] ^= 0xFF;
        std::fs::write(&path, &raw).unwrap();

        // Header CRC is still valid (it covers key, not value),
        // but value CRC must fail.
        let reader = ValueLogReader::open(path.clone()).unwrap();
        let err = match reader.read_entry(offset, total as u32) {
            Ok(_) => panic!("expected CRC failure after value corruption"),
            Err(e) => e,
        };
        let msg = format!("{err:#}");
        assert!(
            msg.contains("value CRC") || msg.contains("value crc"),
            "expected value CRC error, got: {msg}"
        );

        // Restore value byte, corrupt the key instead.
        raw[value_start] ^= 0xFF;
        let key_start = VlogFileHeader::SIZE + HEADER_SIZE;
        raw[key_start] ^= 0xFF;
        std::fs::write(&path, &raw).unwrap();

        let reader = ValueLogReader::open(path).unwrap();
        let err = match reader.read_entry(offset, total as u32) {
            Ok(_) => panic!("expected CRC failure after key corruption"),
            Err(e) => e,
        };
        let msg = format!("{err:#}");
        assert!(
            msg.contains("header CRC") || msg.contains("header crc"),
            "expected header CRC error, got: {msg}"
        );
    }
}
