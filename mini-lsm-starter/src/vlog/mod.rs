pub mod builder;
pub mod gc;
pub mod reader;

pub use builder::ValueLogBuilder;
pub use gc::GarbageCollector;
pub use reader::{ValueLogReader, VlogEntryMeta};

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use anyhow::{Result, anyhow};
use bytes::{Buf, BufMut, Bytes};
use moka::sync::Cache;
use parking_lot::{Mutex, RwLock};

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

// =============================================================================
// ValueLog Manager (Phase 2 — SSTable Integration)
// =============================================================================

/// Tracks which vLog files are referenced by each SST.
struct VlogReferencesInner {
    sst_to_vlogs: HashMap<usize, HashSet<u32>>,
    vlog_to_ssts: HashMap<u32, HashSet<usize>>,
}

/// Tracks which vLog files are referenced by each SST.
pub struct VlogReferences {
    inner: RwLock<VlogReferencesInner>,
}

impl Default for VlogReferences {
    fn default() -> Self {
        Self::new()
    }
}

impl VlogReferences {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(VlogReferencesInner {
                sst_to_vlogs: HashMap::new(),
                vlog_to_ssts: HashMap::new(),
            }),
        }
    }

    /// Register that `sst_id` references the given vLog file ids.
    /// Replaces any existing references for this `sst_id` to prevent leaks.
    pub fn register(&self, sst_id: usize, vlog_ids: &[u32]) {
        let mut inner = self.inner.write();
        // Remove old mappings first to prevent leaks on re-registration.
        if let Some(old_ids) = inner.sst_to_vlogs.remove(&sst_id) {
            for vid in old_ids {
                if let Some(ssts) = inner.vlog_to_ssts.get_mut(&vid) {
                    ssts.remove(&sst_id);
                    if ssts.is_empty() {
                        inner.vlog_to_ssts.remove(&vid);
                    }
                }
            }
        }
        if vlog_ids.is_empty() {
            return;
        }
        let set: HashSet<u32> = HashSet::from_iter(vlog_ids.iter().copied());
        for &vid in &set {
            inner.vlog_to_ssts.entry(vid).or_default().insert(sst_id);
        }
        inner.sst_to_vlogs.insert(sst_id, set);
    }

    /// Get the set of vLog file ids referenced by `sst_id`.
    pub fn get_sst_references(&self, sst_id: usize) -> Option<Vec<u32>> {
        let inner = self.inner.read();
        inner
            .sst_to_vlogs
            .get(&sst_id)
            .map(|set| set.iter().copied().collect())
    }

    /// Get the set of SST ids that reference `vlog_id`.
    pub fn get_ssts_referencing(&self, vlog_id: u32) -> Option<Vec<usize>> {
        let inner = self.inner.read();
        inner
            .vlog_to_ssts
            .get(&vlog_id)
            .map(|set| set.iter().copied().collect())
    }

    /// Unregister all references for `sst_id` and return the previously
    /// referenced vLog file ids.
    pub fn unregister(&self, sst_id: usize) -> Vec<u32> {
        let mut inner = self.inner.write();
        if let Some(vlog_ids) = inner.sst_to_vlogs.remove(&sst_id) {
            for vid in &vlog_ids {
                if let Some(ssts) = inner.vlog_to_ssts.get_mut(vid) {
                    ssts.remove(&sst_id);
                    if ssts.is_empty() {
                        inner.vlog_to_ssts.remove(vid);
                    }
                }
            }
            vlog_ids.into_iter().collect()
        } else {
            Vec::new()
        }
    }
}

/// Entry pending deletion: a vLog file scheduled for deferred removal.
pub struct PendingDeletion {
    pub file_id: u32,
}

/// Manager for the value log file set.
///
/// Owns the active writer (for flushing), a cache of open readers, and
/// reference tracking between SSTs and vLog files.
pub struct ValueLog {
    /// Root directory where vLog files are stored.
    pub path: PathBuf,
    /// Options controlling key-value separation.
    pub options: ValueSeparationOptions,
    /// Monotonically increasing file id for the *next* vLog file to create.
    next_file_id: AtomicU32,
    /// Cache of open readers keyed by `file_id`.
    readers: Cache<u32, Arc<ValueLogReader>>,
    /// Tracks which SSTs reference which vLog files.
    pub references: VlogReferences,
    /// Pending vLog files waiting for GC reclaim.
    pending_deletions: Mutex<Vec<PendingDeletion>>,
    /// Per-file GC locks to prevent concurrent GC on the same file.
    /// Shared across all GarbageCollector instances.
    gc_locks: Mutex<HashSet<u32>>,
}

impl ValueLog {
    /// Open the vLog manager. Scans `path` for existing `*.vlog` files
    /// and sets `next_file_id` to one past the highest found id.
    pub fn open(path: impl AsRef<Path>, options: ValueSeparationOptions) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        std::fs::create_dir_all(&path)?;
        anyhow::ensure!(
            options.max_open_vlog_files >= 1,
            "max_open_vlog_files must be at least 1, got {}",
            options.max_open_vlog_files
        );

        let mut max_id: Option<u32> = None;
        for entry in std::fs::read_dir(&path)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name();
            if let Some(name) = name.to_str()
                && let Some(stem) = name.strip_suffix(".vlog")
                && let Ok(id) = stem.parse::<u32>()
            {
                max_id = Some(max_id.map_or(id, |m| m.max(id)));
            }
        }

        let readers = Cache::new(options.max_open_vlog_files as u64);

        Ok(Self {
            path,
            options,
            next_file_id: AtomicU32::new(max_id.map_or(0, |id| id + 1)),
            readers,
            references: VlogReferences::new(),
            pending_deletions: Mutex::new(Vec::new()),
            gc_locks: Mutex::new(HashSet::new()),
        })
    }

    /// Return the next vLog file id to use for a new file.
    pub fn next_file_id(&self) -> u32 {
        self.next_file_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Full path for a given vLog file id.
    pub fn path_of_file(&self, file_id: u32) -> PathBuf {
        self.path.join(format!("{}.vlog", file_id))
    }

    /// Get a cached reader for `file_id`, opening it on cache miss.
    pub fn get_reader(&self, file_id: u32) -> Result<Arc<ValueLogReader>> {
        self.readers
            .try_get_with(file_id, || {
                let path = self.path_of_file(file_id);
                let reader = ValueLogReader::open(path)?.with_file_id(file_id);
                Ok::<_, anyhow::Error>(Arc::new(reader))
            })
            .map_err(|e| anyhow!("failed to open vlog reader {}: {}", file_id, e))
    }

    /// Read the value at `ptr`, verifying that the stored key matches
    /// `expected_key`.
    pub fn read(&self, ptr: &ValuePointer, expected_key: &[u8]) -> Result<Bytes> {
        let reader = self.get_reader(ptr.file_id)?;
        let entry = reader.read_entry(ptr.offset, ptr.size)?;
        if entry.key != expected_key {
            return Err(anyhow!(
                "vlog key mismatch: expected {:?}, got {:?}",
                expected_key,
                entry.key
            ));
        }
        Ok(Bytes::from(entry.value))
    }

    /// Read a full vLog entry (key, value) at the given pointer.
    /// Used by GC to re-read entries for rewriting.
    pub fn read_entry(&self, ptr: &ValuePointer) -> Result<(Vec<u8>, Vec<u8>)> {
        let reader = self.get_reader(ptr.file_id)?;
        let entry = reader.read_entry(ptr.offset, ptr.size)?;
        Ok((entry.key, entry.value))
    }

    /// Register that `sst_id` references the given vLog file ids.
    pub fn register_sst_references(&self, sst_id: usize, vlog_ids: &[u32]) {
        self.references.register(sst_id, vlog_ids);
    }

    /// Get the vLog file ids referenced by `sst_id`.
    pub fn get_sst_references(&self, sst_id: usize) -> Option<Vec<u32>> {
        self.references.get_sst_references(sst_id)
    }

    /// Get the SST ids that reference `vlog_id`.
    pub fn get_ssts_referencing(&self, vlog_id: u32) -> Option<Vec<usize>> {
        self.references.get_ssts_referencing(vlog_id)
    }

    /// Remove all reference tracking for `sst_id`.
    pub fn unregister_sst_references(&self, sst_id: usize) -> Vec<u32> {
        self.references.unregister(sst_id)
    }

    /// Remove a vLog file from disk and the reader cache.
    pub fn remove_file(&self, file_id: u32) -> Result<()> {
        let path = self.path_of_file(file_id);
        if let Err(e) = std::fs::remove_file(&path)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            return Err(e.into());
        }
        self.readers.invalidate(&file_id);
        Ok(())
    }

    // -----------------------------------------------------------------
    // Phase 3 placeholders (GC)
    // -----------------------------------------------------------------

    /// Try to acquire the GC lock for a file. Returns false if already locked.
    /// Shared across all GarbageCollector instances to prevent concurrent GC
    /// on the same file.
    pub fn try_acquire_gc_lock(&self, file_id: u32) -> bool {
        self.gc_locks.lock().insert(file_id)
    }

    /// Release the GC lock for a file.
    pub fn release_gc_lock(&self, file_id: u32) {
        self.gc_locks.lock().remove(&file_id);
    }

    /// Schedule a vLog file for later deletion once all SST references
    /// are dropped. Called during GC.
    pub fn schedule_deletion(&self, file_id: u32) {
        self.pending_deletions
            .lock()
            .push(PendingDeletion { file_id });
    }

    /// Attempt to delete any pending vLog files that are no longer
    /// referenced by any SST.
    pub fn reclaim_pending_deletions(&self) -> Result<usize> {
        let mut to_process: Vec<PendingDeletion> = {
            let mut pending = self.pending_deletions.lock();
            std::mem::take(&mut *pending)
        };
        to_process.sort_unstable_by_key(|p| p.file_id);
        to_process.dedup_by_key(|p| p.file_id);

        let mut remaining = Vec::new();
        let mut deleted = 0usize;
        let mut first_err = None;
        for entry in to_process {
            if self
                .get_ssts_referencing(entry.file_id)
                .unwrap_or_default()
                .is_empty()
            {
                match self.remove_file(entry.file_id) {
                    Ok(()) => deleted += 1,
                    Err(e) => {
                        if first_err.is_none() {
                            first_err = Some(e);
                        }
                        remaining.push(entry);
                    }
                }
            } else {
                remaining.push(entry);
            }
        }
        {
            let mut pending = self.pending_deletions.lock();
            pending.extend(remaining);
        }
        match first_err {
            Some(e) => Err(e),
            None => Ok(deleted),
        }
    }
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

    // ================================================================
    // ValueLog Manager tests
    // ================================================================

    use std::io::Write;

    fn make_test_options() -> ValueSeparationOptions {
        ValueSeparationOptions {
            enabled: true,
            min_value_size: 4,
            max_value_size: 1 << 20,
            max_vlog_file_size: 1 << 20,
            gc_threshold_ratio: 0.5,
            max_open_vlog_files: 4,
        }
    }

    #[test]
    fn test_vlog_manager_open_scans_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        // Create a couple of vLog files out-of-band.
        for id in [3u32, 7, 12] {
            let path = dir.path().join(format!("{}.vlog", id));
            let mut f = std::fs::File::create(&path).unwrap();
            let mut buf = Vec::new();
            VlogFileHeader {
                magic: VLOG_MAGIC,
                version: 1,
                reserved: [0u8; 10],
            }
            .encode(&mut buf);
            f.write_all(&buf).unwrap();
        }

        let vlog = ValueLog::open(dir.path(), make_test_options()).unwrap();
        assert_eq!(vlog.next_file_id(), 13); // 12 + 1
    }

    #[test]
    fn test_vlog_manager_reader_cache_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let vlog = ValueLog::open(dir.path(), make_test_options()).unwrap();
        let file_id = vlog.next_file_id();

        // Write an entry manually.
        let key = b"hello";
        let value = b"world";
        let path = vlog.path_of_file(file_id);
        {
            let mut writer = ValueLogWriter::create(path.clone(), file_id).unwrap();
            writer.append(key, value).unwrap();
            writer.close().unwrap();
        }

        // Read back via manager.
        let ptr = ValuePointer {
            file_id,
            offset: VlogFileHeader::SIZE as u64,
            size: VlogEntryHeader::compute_entry_size(key.len(), value.len()).unwrap() as u32,
        };
        let got = vlog.read(&ptr, key).unwrap();
        assert_eq!(got.as_ref(), value);

        // Cached reader is returned on second request.
        let h1 = vlog.get_reader(file_id).unwrap();
        let h2 = vlog.get_reader(file_id).unwrap();
        assert!(Arc::ptr_eq(&h1, &h2));
    }

    #[test]
    fn test_vlog_manager_reference_tracking() {
        let refs = VlogReferences::new();
        refs.register(1, &[10, 20]);
        refs.register(2, &[20, 30]);

        let mut r1 = refs.get_sst_references(1).unwrap();
        r1.sort();
        assert_eq!(r1, vec![10, 20]);

        let mut r20 = refs.get_ssts_referencing(20).unwrap();
        r20.sort();
        assert_eq!(r20, vec![1, 2]);

        let mut removed = refs.unregister(1);
        removed.sort();
        assert_eq!(removed, vec![10, 20]);
        assert!(refs.get_sst_references(1).is_none());
        assert_eq!(refs.get_ssts_referencing(10), None);
        // SST 2 still references 20.
        assert_eq!(refs.get_ssts_referencing(20).unwrap(), vec![2]);
    }

    #[test]
    fn test_vlog_manager_remove_file() {
        let dir = tempfile::tempdir().unwrap();
        let vlog = ValueLog::open(dir.path(), make_test_options()).unwrap();
        let file_id = vlog.next_file_id();

        let path = vlog.path_of_file(file_id);
        {
            let mut f = std::fs::File::create(&path).unwrap();
            let mut buf = Vec::new();
            VlogFileHeader {
                magic: VLOG_MAGIC,
                version: 1,
                reserved: [0u8; 10],
            }
            .encode(&mut buf);
            f.write_all(&buf).unwrap();
        }
        assert!(path.exists());

        vlog.remove_file(file_id).unwrap();
        assert!(!path.exists());
    }
}
