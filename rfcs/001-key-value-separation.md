# RFC: Key-Value Separation for Mini-LSM

**Status**: Draft  
**Author**: Mini-LSM Contributors  
**Created**: 2026-03-08  
**Target Version**: Post-Week 3  
**Tracking Issue**: TBD

---

## Summary

This RFC proposes adding key-value separation support to Mini-LSM, inspired by [WiscKey](https://www.usenix.org/system/files/conference/fast16/fast16-papers-lu.pdf) and production systems like BadgerDB and RocksDB's BlobDB. Key-value separation stores large values separately in dedicated Value Log (vLog) files while keeping keys and value pointers in the LSM tree. This significantly reduces write amplification and improves compaction performance for workloads with large values.

## Motivation

### Current Architecture Limitations

In the current Mini-LSM implementation, both keys and values are stored together in SSTable blocks:

```
┌─────────────────────────────────────────────────────────────┐
│  Block Format (Current)                                     │
├─────────────────────────────────────────────────────────────┤
│  ┌──────────┬──────────┬────────┬──────────┬──────────┐    │
│  │ key_len  │ key      │ val_len│ value    │ offset   │    │
│  │ (2B)     │ (var)    │ (2B)   │ (var)    │ (2B)     │    │
│  └──────────┴──────────┴────────┴──────────┴──────────┘    │
└─────────────────────────────────────────────────────────────┘
```

This design has several issues with large values:

1. **High Write Amplification**: During compaction, the entire key-value pair is rewritten even though keys are typically much smaller than values.
2. **Inefficient Range Scans**: Range scans must read through large values even when only keys are needed.
3. **Cache Pollution**: Large values consume block cache space inefficiently.
4. **Slower Compaction**: Moving large amounts of data during compaction increases I/O pressure.

### Example Scenario

Consider a workload with:
- Key size: 100 bytes
- Value size: 10 KB
- Total data: 10 GB (100M key-value pairs)

With leveled compaction (amplification ~10x), the system writes ~100 GB during compactions. With key-value separation, only ~1 GB of keys are rewritten, reducing amplification by **10x**.

## Design Overview

### Value Log (vLog) Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Key-Value Separation Architecture            │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   LSM Tree (Keys + Value Pointers)                             │
│   ┌─────────────────────────────────────────────────────────┐  │
│   │  Key: "user:1001" → ValuePtr: {vlog_id: 5, offset: 1024}│  │
│   │  Key: "user:1002" → ValuePtr: {vlog_id: 5, offset: 2048}│  │
│   └─────────────────────────────────────────────────────────┘  │
│                              │                                  │
│                              ▼                                  │
│   Value Log Files (.vlog)                                      │
│   ┌─────────────────────────────────────────────────────────┐  │
│   │  vlog_00001.vlog                                       │  │
│   │  ┌──────────┬────────┬──────────┬──────────┬──────────┐ │  │
│   │  │ checksum │ key_len│ key      │ val_len  │ value    │ │  │
│   │  │ (4B)     │ (2B)   │ (var)    │ (4B)     │ (var)    │ │  │
│   │  └──────────┴────────┴──────────┴──────────┴──────────┘ │  │
│   └─────────────────────────────────────────────────────────┘  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Key Components

1. **ValueLog**: Manages value log files, handles writes and garbage collection
2. **ValuePointer**: A reference to a value stored in vLog (file_id, offset, size)
3. **ValueLogBuilder**: Builds vLog files during SSTable construction
4. **GarbageCollector**: Reclaims space from stale values during compaction

## Detailed Design

### 1. Value Pointer Format

```rust
/// A pointer to a value stored in the Value Log.
/// Stored inline in the LSM tree instead of the actual value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValuePointer {
    /// Value log file ID
    pub file_id: u32,
    /// Offset within the file where the value starts
    pub offset: u64,
    /// Total size of the encoded entry on disk (header + key + value + padding).
    /// u32 limits individual entries to ~4GB. In practice, max_value_size should
    /// be set well below this (e.g., 128MB) to keep GC scan times reasonable.
    pub size: u32,
}

/// Per-entry value-kind stored with every key-value entry: in the memtable, WAL,
/// and SST block metadata. This is the authoritative source of truth for
/// distinguishing inline values from vLog pointers. A single-byte tag prefix in
/// the value payload (see `VALUE_POINTER_TAG`) is also present as a fast-path
/// sanity check, but the `KvKind` is what the reader trusts — it eliminates the
/// collision risk where a user value whose first byte happens to be `0xFF`
/// would otherwise be misclassified as a pointer.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KvKind {
    /// The value is stored inline in the SST block.
    Inline = 0,
    /// The value is a 17-byte encoded `ValuePointer` that references the vLog.
    ValuePointer = 1,
}
// NOTE: Normal user writes store full values with KvKind::Inline in the WAL and
// memtable. GC rewrites store encoded ValuePointers with KvKind::ValuePointer.
// The tag byte is only a corruption/desync check; payload sniffing is never
// used as the authoritative classifier, so user values may freely start with
// 0xFF.

impl KvKind {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Inline),
            1 => Some(Self::ValuePointer),
            _ => None,
        }
    }
}

/// Magic tag byte that prefixes every encoded `ValuePointer`.
///
/// Serves as a fast-path sanity check: if the first byte of a candidate value
/// is not `0xFF`, the value is definitely not a pointer. However the
/// authoritative classification comes from the entry's `KvKind` metadata,
/// because a user value can legitimately start with `0xFF`.
/// Fast-path sanity byte prefix on encoded ValuePointers.
/// With KvKind as authoritative metadata, this tag is technically redundant
/// for classification. It is retained as a cheap corruption/desync detector:
/// if a reader sees KvKind::ValuePointer but the payload doesn't start with
/// 0xFF, something is wrong. The 1-byte overhead (17 vs 16 bytes) is negligible
/// compared to the values it references. Removing it would save one byte per
/// pointer but lose the cross-check; a future optimization can drop it if the
/// encoded size becomes a bottleneck.
const VALUE_POINTER_TAG: u8 = 0xFF;

impl ValuePointer {
    /// Encode to bytes for storage in LSM tree.
    ///
    /// Layout (17 bytes): `[tag:1][file_id:4][offset:8][size:4]`
    pub fn encode(&self, buf: &mut Vec<u8>) {
        buf.put_u8(VALUE_POINTER_TAG);
        buf.put_u32_le(self.file_id);
        buf.put_u64_le(self.offset);
        buf.put_u32_le(self.size);
    }

    /// Decode from bytes. Returns an error if the buffer is malformed.
    pub fn decode(mut buf: &[u8]) -> Result<Self> {
        if buf.len() < Self::encoded_size() {
            return Err(anyhow!("ValuePointer buffer too short: {} < {}", buf.len(), Self::encoded_size()));
        }
        let tag = buf.get_u8();
        if tag != VALUE_POINTER_TAG {
            return Err(anyhow!("ValuePointer tag mismatch: expected 0x{:02X}, got 0x{:02X}", VALUE_POINTER_TAG, tag));
        }
        Ok(Self {
            file_id: buf.get_u32_le(),
            offset: buf.get_u64_le(),
            size: buf.get_u32_le(),
        })
    }

    /// Try to decode from bytes. Returns `None` if the buffer is too short or
    /// does not start with the `VALUE_POINTER_TAG` byte.
    ///
/// Callers should check the entry's `KvKind` metadata first (it is
/// authoritative) and only use `try_decode` as a fast-path filter. This avoids
/// the edge-case collision where a user value whose first byte is `0xFF` could
/// be misclassified as a pointer.
    pub fn try_decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < Self::encoded_size() || buf[0] != VALUE_POINTER_TAG {
            return None;
        }
        // Inline field decoding to avoid redundant length/tag checks
        // and anyhow error construction in the hot path.
        let mut b = &buf[1..];
        Some(Self {
            file_id: b.get_u32_le(),
            offset: b.get_u64_le(),
            size: b.get_u32_le(),
        })
    }

    /// Total encoded size: 17 bytes (1-byte tag + 4 + 8 + 4)
    pub const fn encoded_size() -> usize {
        1 + 4 + 8 + 4 // 17 bytes
    }
}
```

### 2. Value Log File Format

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Value Log Entry Format                           │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌───────────┬─────────┬───────────┬───────────┐                   │
│  │ Header    │ Key     │ Value     │ Padding   │                   │
│  │ (24 bytes)│ (var)   │ (var)     │ (0-7 bytes)│                  │
│  └───────────┴─────────┴───────────┴───────────┘                   │
│                                                                     │
│  Header Format (24 bytes total):                                    │
│  ┌─────────────┬─────────────┬─────────────┬───────────────┬─────────────┬──────────┐
│  │ header_crc32│ value_crc32 │ value_length│ key_length    │ flags       │ padding  │
│  │ (4 bytes)   │ (4 bytes)   │ (4 bytes)   │ (2 bytes)     │ (2 bytes)   │ (8 bytes)│
│  └─────────────┴─────────────┴─────────────┴───────────────┴─────────────┴──────────┘
│                                                                     │
│  Header CRC32: Covers (header_without_header_crc) + key so          │
│         header-only GC scans can validate length fields and skip     │
│         values safely.                                              │
│  Value CRC32: Covers only the value payload and is checked when      │
│         the value is read.                                          │
│                                                                     │
│  Alignment: Each entry (header + key + value) is padded to an       │
│             8-byte boundary on disk; the trailing pad bytes are     │
│             included in the entry's `size` so readers can skip      │
│             cleanly to the next entry.                              │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

```rust
/// Magic number for vLog file header
const VLOG_MAGIC: u32 = 0x564C4F47; // "VLOG"

/// Value log file header (first 16 bytes of each vLog file)
#[repr(C)]
#[derive(Clone, Debug)]
pub struct VlogFileHeader {
    pub magic: u32,           // 4 bytes
    pub version: u16,         // 2 bytes
    pub reserved: [u8; 10],   // 10 bytes padding to align to 16 bytes total
}

/// Entry header (precedes each key-value pair).
///
/// Field order is chosen so that all u32 fields come before u16 fields, which
/// keeps the C struct layout naturally 4-byte-aligned with no implicit padding
/// between the declared fields. The trailing `_padding` brings the total to a
/// flat 24 bytes and preserves the file's 8-byte alignment guarantee.
///
/// **Serialization note:** Do NOT cast raw byte buffers to `&VlogEntryHeader`
/// — vLog entries are read from arbitrary file offsets where 4-byte alignment
/// is not guaranteed, making pointer casts undefined behavior.  Instead,
/// serialize/deserialize each field individually using explicit little-endian
/// encoding (e.g., `bytes::Buf::get_u32_le()` / `bytes::BufMut::put_u32_le()`).
/// This also makes `#[repr(C)]` and `std::mem::size_of` unnecessary; the
/// header is always exactly 24 bytes by construction.
pub struct VlogEntryHeader {
    pub header_crc32: u32,    // CRC32 of the rest of the header + key (4 bytes)
    pub value_crc32: u32,     // CRC32 of the value payload (4 bytes)
    pub value_len: u32,       // Value length (max 4GB) (4 bytes)
    pub key_len: u16,         // Key length (max 64KB). Large keys must be stored inline. (2 bytes)
    pub flags: u16,           // Flags (tombstone, etc.) (2 bytes)
    pub _padding: [u8; 8],    // Reserved / padding to a 24-byte total
}
// The split checksum is intentional: GC analysis can validate header + key
// without reading large values, while normal reads still verify payload bytes.

const HEADER_SIZE: usize = 24; // VlogEntryHeader is always 24 bytes
const ALIGNMENT: usize = 8;
```

### 2.5 ValueLogBuilder

The `ValueLogBuilder` constructs vLog entries during SSTable building. It is owned by `SsTableBuilder` and writes sequentially to the current vLog file.

```rust
/// Builder for constructing vLog entries during SST construction.
pub struct ValueLogBuilder {
    writer: ValueLogWriter,
    file_id: u32,
}

impl ValueLogBuilder {
    /// Add a key-value pair to the vLog. Returns a `ValuePointer`.
    ///
    /// The on-disk footprint of an entry is `header + key + value`, padded up
    /// to the next `ALIGNMENT` (8-byte) boundary. The pad bytes are written to
    /// disk *and* counted in `ValuePointer::size`, so a reader can validate
    /// the entry and advance to the next one without re-reading the header.
    pub fn add(&mut self, key: &[u8], value: &[u8]) -> Result<ValuePointer> {
        let offset = self.writer.offset();

        // Validate BEFORE writing to avoid corrupting the vLog with an oversized entry.
        anyhow::ensure!(
            key.len() <= u16::MAX as usize,
            "key length {} exceeds vLog header u16 capacity",
            key.len()
        );
        let entry_size = HEADER_SIZE + key.len() + value.len();
        let padding = (ALIGNMENT - (entry_size % ALIGNMENT)) % ALIGNMENT;
        let total = entry_size + padding;
        anyhow::ensure!(
            total <= u32::MAX as usize,
            "vLog entry size {} exceeds u32 capacity — increase max_value_size or reduce key/value size",
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
}
```

### 3. ValueLog Module Structure

```
src/
├── vlog/
│   ├── mod.rs           # ValueLog manager
│   ├── builder.rs       # ValueLogBuilder for constructing vLog files
│   ├── reader.rs        # ValueLogReader for reading values
│   └── gc.rs            # GarbageCollector for space reclamation
```

### 4. Configuration Options

```rust
#[derive(Clone, Debug)]
pub struct ValueSeparationOptions {
    /// Enable key-value separation
    pub enabled: bool,

    /// Minimum value size to trigger separation (bytes)
    /// Values smaller than this are stored inline
    pub min_value_size: usize,

    /// Maximum size of a single value (bytes). Must fit in u32 after
    /// header + key + padding are added. Recommended: 128MB or less.
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
            enabled: false,               // Disabled by default for backward compatibility
            min_value_size: 1024,         // 1KB threshold
            max_value_size: 128 << 20,    // 128MB max per value (well under u32 overflow)
            max_vlog_file_size: 64 << 20, // 64MB per vLog file
            gc_threshold_ratio: 0.5,      // GC when 50% stale
            max_open_vlog_files: 64,
        }
    }
}
```

### 5. Modified SSTableBuilder

```rust
pub struct SsTableBuilder {
    builder: BlockBuilder,
    first_key: KeyVec,
    last_key: KeyVec,
    data: Vec<u8>,
    pub(crate) meta: Vec<BlockMeta>,
    block_size: usize,
    key_hashes: Vec<u32>,
    
    // NEW: Value log components
    // `vlog_builder` is a per-flush writer that allocates its own vLog file ID
    // from `ValueLog::next_file_id()`. This avoids contention on the shared
    // `active_writer` during flush. Each concurrent flush gets its own file.
    vlog_options: Option<ValueSeparationOptions>,
    vlog_builder: Option<ValueLogBuilder>,
    referenced_vlogs: HashSet<u32>,
}

impl SsTableBuilder {
    /// Add a key-value pair to the builder.
    /// `input_kind`: pass `Some(KvKind)` during compaction (from source SST metadata),
    /// or `None` for new writes (memtable flush) — the builder decides automatically.
    ///
    /// When `input_kind` is `None` (flush path), this method writes large values
    /// to the vLog and stores only the `ValuePointer` in the SST. The vLog is
    /// fsynced before the SST entry is written, so every pointer is durable.
    /// Backward-compatible wrapper: defaults to `input_kind = None` (flush path).
    pub fn add(&mut self, key: KeySlice, value: &[u8]) -> Result<()> {
        self.add_with_kind(key, value, None)
    }

    /// Add a key-value pair to the builder with an explicit KvKind.
    /// `input_kind`: pass `Some(KvKind)` during compaction (from source SST metadata),
    /// or `None` for new writes (memtable flush) — the builder decides automatically.
    ///
    /// When `input_kind` is `None` (flush path), this method writes large values
    /// to the vLog and stores only the `ValuePointer` in the SST. The vLog is
    /// fsynced before the SST file is written, so every pointer is durable.
    pub fn add_with_kind(&mut self, key: KeySlice, value: &[u8], input_kind: Option<KvKind>) -> Result<()> {
        if self.first_key.is_empty() {
            self.first_key.set_from_slice(key);
        }

        self.key_hashes.push(farmhash::fingerprint32(key.raw_ref()));

        // Local buffer for encoded ValuePointer — declared here so its slice
        // reference can be used in the `value_to_store` binding below without
        // borrow checker conflicts with self.finish_block().
        let mut local_buf = Vec::new();

        // Determine value-to-store and KvKind.
        //
        // During compaction the caller passes `input_kind` from the source SST's
        // block metadata, so classification is authoritative — no payload sniffing.
        // During flush `input_kind` is None and we decide here: large values are
        // written to the vLog (with fsync), and only the ValuePointer is stored
        // in the SST.
        let (value_to_store, kind) = if let Some(k) = input_kind {
            // Compaction path: trust the caller's authoritative metadata.
            if k == KvKind::ValuePointer {
                let vptr = ValuePointer::decode(value)
                    .expect("failed to decode ValuePointer during compaction — metadata is authoritative");
                self.referenced_vlogs.insert(vptr.file_id);
            }
            (value, k)
        } else if self.should_separate_value(key, value) {
            let vptr = self.write_to_vlog(key, value)?;
            // Encode into a local Vec — avoids borrow checker conflict with
            // self.finish_block() below. The buffer is small (17 bytes) and
            // only allocated on the separation path, so the cost is negligible.
            local_buf.reserve(ValuePointer::encoded_size());
            vptr.encode(&mut local_buf);
            (local_buf.as_slice(), KvKind::ValuePointer)
        } else {
            (value, KvKind::Inline)
        };

        // Each block entry now carries (key, value, KvKind) so that the
        // reader can classify the value without guessing from the payload.
        if self.builder.add_with_kind(key, value_to_store, kind) {
            self.last_key.set_from_slice(key);
            return Ok(());
        }

        self.finish_block();
        self.first_key.set_from_slice(key);
        assert!(self.builder.add_with_kind(key, value_to_store, kind));
        self.last_key.set_from_slice(key);
        Ok(())
    }

    /// Write a key-value pair to the active vLog builder and return a pointer.
    fn write_to_vlog(&mut self, key: KeySlice, value: &[u8]) -> Result<ValuePointer> {
        let ptr = self.vlog_builder.as_mut().unwrap().add(key.raw_ref(), value)?;
        self.referenced_vlogs.insert(ptr.file_id);
        Ok(ptr)
    }

    fn should_separate_value(&self, key: KeySlice, value: &[u8]) -> bool {
        match &self.vlog_options {
            Some(opts) if opts.enabled => {
                // Keys stored in vLog must fit in the u16 key_len field
                value.len() >= opts.min_value_size
                    && value.len() <= opts.max_value_size
                    && key.raw_ref().len() <= u16::MAX as usize
            }
            _ => false,
        }
    }
}
```

### 6. ValueLog Implementation

```rust
/// Pending deletion entry: a vLog file that has been retired by GC but whose
/// on-disk deletion is deferred until it is safe.
pub struct PendingDeletion {
    file_id: u32,
    /// The engine timestamp / epoch at the moment GC retired this file.
    obsolete_at_ts: u64,
}

/// Bidirectional SST ↔ vLog reference tracking.
/// Both maps under a single lock to prevent deadlocks.
pub struct VlogReferences {
    /// SST ID → set of vLog files it references
    sst_to_vlogs: HashMap<usize, HashSet<u32>>,
    /// vLog file ID → set of SSTs referencing it (reverse index)
    vlog_to_ssts: HashMap<u32, HashSet<usize>>,
}

/// RAII guard for a cached vLog reader. Automatically decrements the
/// per-file atomic refcount when dropped, preventing retired vLog files
/// from being unlinked while an iterator or snapshot still reads them.
pub struct ValueLogReaderHandle {
    reader: Arc<ValueLogReader>,
    vlog: Arc<ValueLog>,
    file_id: u32,
    counter: Arc<AtomicUsize>,  // Per-file atomic — no global lock on increment/decrement
}

impl std::ops::Deref for ValueLogReaderHandle {
    type Target = ValueLogReader;
    fn deref(&self) -> &Self::Target { &self.reader }
}

impl Drop for ValueLogReaderHandle {
    fn drop(&mut self) {
        self.vlog.release_reader(self.file_id, &self.counter);
    }
}

/// Manages value log files for the storage engine.
pub struct ValueLog {
    /// Path to the vLog directory
    path: PathBuf,

    /// Currently active vLog file for writing
    active_writer: Mutex<ValueLogWriter>,

    /// Read cache for vLog files (file_id -> Arc<ValueLogReader>)
    readers: moka::sync::Cache<u32, Arc<ValueLogReader>>,

    /// Next vLog file ID
    next_file_id: AtomicU32,

    /// Configuration options
    options: ValueSeparationOptions,

    /// SST ↔ vLog bidirectional reference tracking.
    /// Both maps live under a single lock to prevent deadlocks from
    /// inconsistent acquisition order.
    vlog_refs: RwLock<VlogReferences>,

    /// Monotonic clock / timestamp provider (shared with the LSM engine).
    /// Used by `schedule_deletion` to stamp each retired file with the
    /// current MVCC epoch so the deferred-reclamation pass can compare it
    /// against the MVCC watermark.
    lsm_clock: Arc<dyn Clock>,

    /// vLog files that have been retired by GC but not yet unlinked.
    /// Protected by a mutex; drained by `reclaim_pending_deletions`.
    pending_deletions: Mutex<Vec<PendingDeletion>>,

    /// Per-file open-reader reference count. Incremented when a
    /// `ValueLogReader` is fetched from the cache; decremented on drop.
    /// Used by `reclaim_pending_deletions` to ensure a file is not deleted
    /// while an iterator or snapshot still holds an open handle.
    reader_refcounts: RwLock<HashMap<u32, Arc<AtomicUsize>>>,

    /// Reference to the manifest for writing NewVlogFile/DeleteVlogFile
    /// records during file rotation and deletion.
    manifest: Arc<Manifest>,
}

impl ValueLog {
    /// Write a key-value pair to the active vLog file.
    /// Returns a ValuePointer that can be stored in the LSM tree.
    ///
    /// This is the primary write path for GC rewrites and any direct vLog
    /// writes. The flush path uses `ValueLogBuilder` (owned by `SsTableBuilder`)
    /// which writes to its own per-flush file; this method writes to the shared
    /// `active_writer` and serializes via `active_writer.lock()`.
    ///
    /// Delegates to ValueLogWriter::append which applies the same 8-byte
    /// alignment padding as ValueLogBuilder::add.
    pub fn write(&self, key: &[u8], value: &[u8]) -> Result<ValuePointer> {
        // VlogEntryHeader.key_len is u16 — reject keys that would overflow.
        anyhow::ensure!(
            key.len() <= u16::MAX as usize,
            "key length {} exceeds vLog header u16 capacity",
            key.len()
        );
        // Enforce max_value_size to prevent u32 overflow in ValuePointer::size
        // and to keep GC scan times reasonable.
        anyhow::ensure!(
            value.len() <= self.options.max_value_size,
            "value length {} exceeds max_value_size {}",
            value.len(),
            self.options.max_value_size
        );

        let mut writer = self.active_writer.lock();

        // Rotate to new file if current is full.
        // NOTE: rotate_vlog_file must NOT write to the manifest directly,
        // because ValueLog::write does not hold the state_lock and acquiring
        // it here would create an AB-BA deadlock (active_writer → state_lock
        // vs. state_lock → active_writer in flush/compaction paths). Instead,
        // rotation just creates the new file; the manifest NewVlogFile record
        // is written by the caller (flush or compaction) which already holds
        // the state_lock.
        if writer.size() >= self.options.max_vlog_file_size {
            self.rotate_vlog_file(&mut writer)?;
        }

        let offset = writer.offset();
        let total = writer.append(key, value)?;
        Ok(ValuePointer {
            file_id: writer.file_id(),
            offset,
            size: total as u32,
        })
    }

    /// Register SST -> vLog references when an SST is finalized.
    /// Updates both forward and reverse indexes atomically under one lock.
    pub fn register_sst_references(&self, sst_id: usize, vlog_ids: HashSet<u32>) {
        let mut refs = self.vlog_refs.write();
        // Unregister any existing references for this SST first to prevent
        // stale entries in the reverse index from leaking.
        if let Some(old_vlogs) = refs.sst_to_vlogs.remove(&sst_id) {
            for vlog_id in old_vlogs {
                if let Some(ssts) = refs.vlog_to_ssts.get_mut(&vlog_id) {
                    ssts.remove(&sst_id);
                    if ssts.is_empty() {
                        refs.vlog_to_ssts.remove(&vlog_id);
                    }
                }
            }
        }
        for vlog_id in &vlog_ids {
            refs.vlog_to_ssts.entry(*vlog_id).or_default().insert(sst_id);
        }
        refs.sst_to_vlogs.insert(sst_id, vlog_ids);
    }

    /// Get all vLog files referenced by a given SST.
    pub fn get_sst_references(&self, sst_id: usize) -> Option<HashSet<u32>> {
        self.vlog_refs.read().sst_to_vlogs.get(&sst_id).cloned()
    }

    /// Get all SSTs that reference a given vLog file.
    /// Uses the reverse index for O(1) lookup.
    pub fn get_ssts_referencing(&self, vlog_id: u32) -> Vec<usize> {
        self.vlog_refs
            .read()
            .vlog_to_ssts
            .get(&vlog_id)
            .map(|ssts| ssts.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Remove all vLog references for a deleted SST.
    /// Updates both indexes atomically under one lock.
    pub fn unregister_sst_references(&self, sst_id: usize) {
        let mut refs = self.vlog_refs.write();
        if let Some(vlog_ids) = refs.sst_to_vlogs.remove(&sst_id) {
            for vlog_id in vlog_ids {
                if let Some(ssts) = refs.vlog_to_ssts.get_mut(&vlog_id) {
                    ssts.remove(&sst_id);
                    if ssts.is_empty() {
                        refs.vlog_to_ssts.remove(&vlog_id);
                    }
                }
            }
        }
    }

    /// Read a value using a ValuePointer. Returns only the value bytes
    /// (the caller never needs to see the vLog header or key).
    /// `expected_key` is validated against the stored key to detect stale or
    /// corrupted pointers that land on a different entry's offset.
    pub fn read(self: &Arc<ValueLog>, ptr: &ValuePointer, expected_key: &[u8]) -> Result<Bytes> {
        // Defensive validation: a corrupted ptr.size (up to u32::MAX) could
        // cause an OOM panic in read_entry. Reject implausible sizes early.
        let min_entry = HEADER_SIZE + expected_key.len();
        let max_entry = self.options.max_value_size + HEADER_SIZE + expected_key.len() + ALIGNMENT;
        anyhow::ensure!(
            ptr.size as usize >= min_entry && ptr.size as usize <= max_entry,
            "ValuePointer size {} is invalid (must be between {} and {})",
            ptr.size, min_entry, max_entry
        );
        let reader = self.get_reader(ptr.file_id)?;
        let entry = reader.read_entry(ptr.offset, ptr.size)?;
        if entry.key != expected_key {
            anyhow::bail!("vLog key mismatch at offset {}: expected {:?}, found {:?}",
                ptr.offset, expected_key, entry.key);
        }
        Ok(Bytes::from(entry.value))
    }

    /// Get a cached reader for the specified vLog file.
    /// Returns a RAII guard that decrements the refcount on drop.
    /// Uses per-file AtomicUsize to avoid global write-lock contention.
    fn get_reader(self: &Arc<ValueLog>, file_id: u32) -> Result<ValueLogReaderHandle> {
        let reader = self.readers.try_get_with(file_id, || {
            ValueLogReader::open(self.path_of_file(file_id)).map(Arc::new)
        }).map_err(|e| anyhow!("Failed to open vlog {}: {}", file_id, e))?;
        // Get or create per-file atomic counter. Read lock first to avoid
        // contention on the common path (counter already exists).
        // Block expression drops the read guard before entering the else branch,
        // preventing deadlock when upgrading to a write lock.
        let counter = if let Some(c) = { self.reader_refcounts.read().get(&file_id).cloned() } {
            c
        } else {
            self.reader_refcounts
                .write()
                .entry(file_id)
                .or_insert_with(|| Arc::new(AtomicUsize::new(0)))
                .clone()
        };
        counter.fetch_add(1, Ordering::AcqRel);
        Ok(ValueLogReaderHandle { reader, vlog: self.clone(), file_id, counter })
    }

    /// Decrements the reference count for a vLog file reader.
    /// Called when a `ValueLogReaderHandle` is dropped.
    /// Counter map entries are NOT removed here to avoid write-lock contention
    /// on the hot path. Stale entries are cleaned up periodically by
    /// `cleanup_stale_counters` or left in place (they are small).
    fn release_reader(&self, _file_id: u32, counter: &AtomicUsize) {
        // Release ordering ensures all prior reads from the vLog file complete
        // before the count is decremented. This prevents another thread from
        // seeing count == 0 and deleting the file while reads are still in flight.
        counter.fetch_sub(1, Ordering::Release);
    }

    /// Returns the current open-reader reference count for a vLog file.
    /// Used by `reclaim_pending_deletions` to ensure a file is not deleted
    /// while iterators or snapshots still hold an open handle.
    pub fn reader_refcount(&self, file_id: u32) -> usize {
        self.reader_refcounts
            .read()
            .get(&file_id)
            .map(|c| c.load(Ordering::Acquire))
            .unwrap_or(0)
    }

    /// Return the current configuration options.
    pub fn options(&self) -> &ValueSeparationOptions {
        &self.options
    }

    /// Allocate and return the next vLog file ID.
    pub fn next_file_id(&self) -> u32 {
        self.next_file_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    /// Return the filesystem path for a given vLog file ID.
    fn path_of_file(&self, file_id: u32) -> PathBuf {
        self.path.join(format!("{:05}.vlog", file_id))
    }

    /// Remove a vLog file from disk and invalidate the cache entry.
    /// Only call this when no active snapshots or iterators reference the file.
    /// Cache is invalidated BEFORE unlink to prevent new readers from opening
    /// a file that's about to be deleted.
    pub fn remove_file(&self, file_id: u32) -> Result<()> {
        self.readers.invalidate(&file_id);
        let path = self.path_of_file(file_id);
        std::fs::remove_file(&path)?;
        Ok(())
    }

    /// Mark a vLog file as obsolete and queue it for deletion. The file is
    /// **not** unlinked here — that would race with active snapshots,
    /// iterators, and any in-flight reads through stale pointers in older
    /// SSTs. Instead the file is parked on a pending-deletion queue and a
    /// background task reclaims it once it is safe.
    ///
    /// Safety conditions (all required):
    /// - the file's reader/iterator refcount has dropped to zero, **and**
    /// - the MVCC watermark has advanced past the timestamp at which the file
    ///   was retired (so no snapshot can still hold a pointer into it), **and**
    /// - all SSTs that referenced this `file_id` have been compacted away
    ///   (so no `get()` can produce a stale pointer to it).
    ///
    /// `obsolete_at_ts` is the engine's current commit timestamp / epoch at
    /// the moment GC retired the file, used by the watermark check.
    pub fn schedule_deletion(&self, file_id: u32) -> Result<()> {
        let obsolete_at_ts = self.lsm_clock.now();
        self.pending_deletions
            .lock()
            .push(PendingDeletion { file_id, obsolete_at_ts });
        Ok(())
    }

    /// Background reclamation pass. Walks the pending queue and unlinks any
    /// file that has cleared all of the deferred-deletion conditions above.
    /// Run on a timer or at the tail of every successful compaction.
    pub fn reclaim_pending_deletions(&self, watermark_ts: u64) -> Result<()> {
        let mut pending = self.pending_deletions.lock();
        pending.retain(|p| {
            let safe = p.obsolete_at_ts <= watermark_ts
                && self.reader_refcount(p.file_id) == 0
                && self.get_ssts_referencing(p.file_id).is_empty();
            if safe {
                // Keep the entry in the queue if unlink fails so it can be retried.
                self.remove_file(p.file_id).is_err()
            } else {
                true // keep, retry later
            }
        });
        Ok(())
    }
}
```

### 7. Garbage Collection

Garbage collection is triggered during compaction when the ratio of stale data exceeds a threshold.

**Important design choice**: Instead of rewriting SSTs to update value pointers (which would add massive write amplification and break SST immutability), we use the standard WiscKey approach:

1. Scan the target vLog file and identify live entries
2. Rewrite live entries to a new vLog file
3. Re-insert each live key with its new `ValuePointer` into the LSM tree via the normal write path
4. Old SSTs still contain stale pointers, but they are shadowed by the newer entries in the memtable and upper LSM levels
5. Eventually, normal compaction removes old SSTs containing stale pointers

```rust
/// A single entry read from a vLog file.
pub struct VlogEntry {
    pub ptr: ValuePointer,
    pub key: Vec<u8>,
    pub value: Vec<u8>,
    pub size: usize,
}

/// Analysis result for a single vLog file.
/// Lightweight reference to a live vLog entry — stores only the pointer
/// and key (not the value) to avoid holding large values in memory during analysis.
pub struct LiveEntryRef {
    pub ptr: ValuePointer,
    pub key: Vec<u8>,
}

pub struct GcAnalysis {
    pub file_id: u32,
    pub stale_ratio: f64,
    pub live_entries: Vec<LiveEntryRef>,
    pub dead_bytes: usize,
}

/// Garbage collector for reclaiming space in value logs.
pub struct GarbageCollector {
    vlog: Arc<ValueLog>,
    lsm: Arc<MiniLsm>,
    threshold: f64,
}

impl GarbageCollector {
    /// Create a new garbage collector.
    pub fn new(vlog: Arc<ValueLog>, lsm: Arc<MiniLsm>, threshold: f64) -> Self {
        Self { vlog, lsm, threshold }
    }

    /// Analyze a vLog file and determine which entries are still live.
    /// Returns the ratio of stale (dead) data.
    ///
    /// Uses a header-only iterator that reads only the header + key (skipping
    /// the value payload) to avoid unnecessary I/O for large dead values.
    /// The value is only read during compact_file for entries confirmed live.
    pub fn analyze_file(&self, file_id: u32) -> Result<GcAnalysis> {
        let reader = self.vlog.get_reader(file_id)?;
        let mut live_entries = Vec::new();
        let mut dead_bytes = 0;
        let mut live_bytes = 0;

        for meta in reader.iter_headers() {
            let entry_size = meta.size;
            if self.check_liveness(&meta.key, &meta.ptr)? {
                live_entries.push(LiveEntryRef { ptr: meta.ptr, key: meta.key });
                live_bytes += entry_size;
            } else {
                dead_bytes += entry_size;
            }
        }

        let total = live_bytes + dead_bytes;
        let stale_ratio = if total > 0 { dead_bytes as f64 / total as f64 } else { 0.0 };

        Ok(GcAnalysis {
            file_id,
            stale_ratio,
            live_entries,
            dead_bytes,
        })
    }

    /// Rewrite live entries to a new vLog file and update the LSM index.
    /// Old SSTs are NOT rewritten; stale pointers are shadowed by new LSM writes.
    ///
    /// **Race avoidance**: a user `put`/`delete` can land on a key between
    /// the `is_entry_live` check and the GC re-insert. To make sure GC never
    /// shadows fresher user writes, we re-validate the pointer atomically
    /// (under a per-key guard or via the LSM's MVCC sequence number) right
    /// before insertion, and only insert when the LSM still observes the
    /// *exact* old pointer for that key. If the key has been overwritten or
    /// deleted in the meantime, the new pointer is discarded — the new vLog
    /// entry becomes orphaned (unreferenced by any SST).
    ///
    /// **Orphan reclamation**: entries written to the new vLog before a failed
    /// CAS are unreferenced but occupy space. No special watermark or tombstone
    /// tracking is needed — the LSM tree is the authoritative source of truth
    /// for liveness.  During the next GC pass of the new vLog file,
    /// `check_liveness` will return `false` for any orphaned entries (they are
    /// not pointed to by any SST), so they are naturally reclaimed by the
    /// standard GC mechanism.
    pub fn compact_file(&self, analysis: &GcAnalysis) -> Result<()> {
        if analysis.stale_ratio < self.threshold {
            return Ok(());
        }

        // Create new vLog file with live entries
        let new_file_id = self.vlog.next_file_id();
        let mut writer = ValueLogWriter::create(self.vlog.path_of_file(new_file_id))?;

        // Phase 1: Rewrite all live entries to the new vLog file.
        // We collect the (key, old_ptr, new_ptr) tuples first, then fsync,
        // then CAS — so every pointer we bind into the LSM already references
        // durable vLog data.
        let mut rewrites: Vec<(Vec<u8>, ValuePointer, ValuePointer)> = Vec::new();
        for entry_ref in &analysis.live_entries {
            let value = self.vlog.read(&entry_ref.ptr, &entry_ref.key)?;
            let offset = writer.offset();
            let total = writer.append(&entry_ref.key, &value)?;
            let new_ptr = ValuePointer {
                file_id: new_file_id,
                offset,
                size: total as u32,
            };
            rewrites.push((entry_ref.key.clone(), entry_ref.ptr, new_ptr));
        }

        // Fsync the new vLog BEFORE binding any pointers into the LSM tree.
        // This prevents dangling pointers on crash: every ValuePointer we
        // CAS into the memtable is already durable on disk.
        writer.close()?;

        // Phase 2: CAS each live key to point at the new vLog location.
        for (key, old_ptr, new_ptr) in &rewrites {
            let mut buf = Vec::with_capacity(ValuePointer::encoded_size());
            new_ptr.encode(&mut buf);

            // Atomic rebind: only swap the pointer if the key still resolves
            // to `old_ptr`. `compare_and_set` performs the get + put under
            // the same MVCC sequence so a concurrent user write cannot be
            // overwritten. Implementations without explicit CAS can serialize
            // GC writes with the write batch lock and re-check `is_entry_live`
            // inside the critical section.
            let mut expected_buf = Vec::with_capacity(ValuePointer::encoded_size());
            old_ptr.encode(&mut expected_buf);
            // Kind-aware CAS: ensures we don't overwrite an inline user value
            // that happens to be byte-identical to the old pointer encoding.
            if !self.lsm.compare_and_set_with_kind(
                key,
                &expected_buf, KvKind::ValuePointer,
                &buf, KvKind::ValuePointer,
            )? {
                continue; // A concurrent user write changed the value — skip this entry
            }
        }

        // Ensure LSM writes are durable before scheduling the old file for reclamation.
        self.lsm.sync()?;

        // Defer deletion until all active snapshots/iterators referencing the
        // old file have been released. See `ValueLog::schedule_deletion` and
        // section 7.1 for the watermark/refcount-based reclamation contract.
        self.vlog.schedule_deletion(analysis.file_id)?;

        Ok(())
    }

    /// Check if a vLog entry is still referenced by the LSM tree.
    /// Uses KvKind (authoritative SST block metadata) to classify the current
    /// value, avoiding ambiguous payload-based pointer detection.
    fn is_entry_live(&self, entry: &VlogEntry) -> Result<bool> {
        self.check_liveness(&entry.key, &entry.ptr)
    }

    /// Liveness check using only key + pointer (no value needed).
    /// Used by analyze_file's header-only iterator to avoid reading values.
    fn check_liveness(&self, key: &[u8], ptr: &ValuePointer) -> Result<bool> {
        match self.lsm.get_with_kind(key)? {
            Some((value, KvKind::ValuePointer)) => {
                let current_ptr = ValuePointer::decode(&value)?;
                Ok(current_ptr.file_id == ptr.file_id && current_ptr.offset == ptr.offset)
            }
            _ => Ok(false), // Key deleted, or value is now inline — entry is stale
        }
    }
}

### 7.1 Stale Pointer Handling

Because SSTs are immutable, old SSTs continue to contain pointers to the old vLog file even after GC moves values to a new file. This is handled naturally by the LSM tree's tiered structure:

- New GC writes go to the **memtable** first
- `get()` searches memtable → immutable memtables → L0 → L1 → ...
- The new pointer in the memtable (or a recently flushed SST) shadows the old pointer
- Range scans may encounter both old and new pointers; merge iterators deduplicate by key
- Eventually, compaction removes old SSTs containing stale pointers entirely

If a `get()` reads a stale pointer from an old SST after the old vLog file has been deleted, it will get an I/O error. To prevent this, GC must only delete old vLog files after:
1. All live entries are rewritten to the new vLog file
2. The new pointers are durably written to the LSM tree (via `sync()`)
3. No active snapshots, iterators, or open SSTables are referencing the old file. This can be achieved by having each `SsTable` instance hold a shared reference/handle to the vLog files it references, keeping the files open and preventing physical deletion until the `SsTable` is dropped.

**Deferred Deletion Strategy:**

Production systems use one of the following approaches to safely reclaim old vLog files:

- **Reference Counting**: Track open readers per vLog file. Delete when count reaches zero.
- **Watermark-Based Reclamation**: Record the current MVCC watermark (minimum active snapshot timestamp) before GC. Only delete files after all snapshots older than that watermark have been released. This integrates naturally with Mini-LSM's Week 3 MVCC design.
- **Epoch-Based Reclamation**: Similar to watermark, but using monotonic epoch counters for non-MVCC systems.
```

### 8. Integration with Compaction

```rust
pub struct CompactionController {
    /// Bounded thread pool for background GC work.
    /// Prevents unbounded thread creation under sustained write load.
    gc_pool: rayon::ThreadPool,
}

impl CompactionController {
    /// After compaction, schedule garbage collection for affected vLog files.
    /// GC runs asynchronously on a background thread to avoid blocking the
    /// compaction pipeline with vLog scanning and LSM lookups.
    pub fn post_compaction_gc(
        &self,
        input_ssts: &[usize],
        output_ssts: &[usize],
        vlog: &Arc<ValueLog>,
        lsm: &Arc<MiniLsm>,
    ) -> Result<()> {
        // Collect all vLog files referenced by input SSTs
        let mut affected_vlogs: HashSet<u32> = HashSet::new();
        
        for sst_id in input_ssts {
            if let Some(vlogs) = vlog.get_sst_references(*sst_id) {
                affected_vlogs.extend(vlogs);
            }
        }

        // Schedule GC on a bounded background worker to avoid blocking compaction.
        // A single GC executor (or small thread pool) prevents unbounded thread
        // creation under sustained write load where compactions outpace GC scans.
        let vlog = vlog.clone();
        let lsm = lsm.clone();
        self.gc_pool.spawn(move || {
            let gc = GarbageCollector::new(vlog.clone(), lsm, vlog.options().gc_threshold_ratio);
            for file_id in affected_vlogs {
                // Skip if another GC task is already processing this file.
                if !gc.try_acquire_gc_lock(file_id) {
                    continue;
                }
                if let Ok(analysis) = gc.analyze_file(file_id) {
                    if analysis.stale_ratio >= vlog.options().gc_threshold_ratio {
                        let _ = gc.compact_file(&analysis);
                    }
                }
                gc.release_gc_lock(file_id);
            }
        });

        // Register vLog references for output SSTs BEFORE removing input refs.
        // SsTableBuilder is a low-level component without access to ValueLog.
        // Registration is handled by the storage engine (LsmStorageInner) when
        // it receives the finalized SST: the builder exposes its referenced_vlogs
        // set via a getter, and the engine calls vlog.register_sst_references().
        //
        // CRITICAL: output registration must precede input unregistration.
        // Otherwise reclaim_pending_deletions() can observe an empty
        // get_ssts_referencing(file_id) for a still-live vLog file and unlink
        // it, causing read failures or data loss.
        for sst_id in output_ssts {
            // Engine registers: builder.get_referenced_vlogs() → vlog.register_sst_references(sst_id, vlogs)
        }

        // Clean up vLog references for input SSTs that are being replaced.
        // DEFERRED: Do NOT unregister immediately here. Active snapshots or
        // long-running iterators may still hold Arc<SsTable> references to
        // these input SSTs. If we unregister now and GC runs, it could see
        // get_ssts_referencing().is_empty() and delete the vLog file while
        // those iterators still need it.
        //
        // Instead, defer unregistration to SsTable::drop (or a drop token/
        // callback). This guarantees a vLog file is never physically deleted
        // while any snapshot or iterator holds a reference to an SST that
        // points to it.
        //
        // for sst_id in input_ssts {
        //     vlog.unregister_sst_references(*sst_id);
        // }

        Ok(())
    }
}
```

**Important**: `unregister_sst_references(sst_id)` removes the SST's entry from the
`sst_to_vlogs` mapping. This must be called whenever an SST is deleted (compaction,
manual removal) to prevent leaked references that block vLog space reclamation.

## Implementation Plan

### Phase 1: Core Infrastructure (Week 1)

1. **ValuePointer and Encoding**
   - Implement `ValuePointer` struct with serialization
   - Add configuration options to `LsmStorageOptions`
   - Create constants and shared types

2. **ValueLog File Format**
   - Implement vLog entry format with CRC32 checksums
   - Create `VlogEntryHeader` and encoding/decoding
   - Add alignment and padding logic

3. **ValueLogWriter**
   - Sequential write API for building vLog files
   - File rotation when size limit reached
   - Sync/flushing semantics

4. **ValueLogReader**
   - Random read API using file ID + offset
   - Iterator interface for garbage collection
   - Validation with checksums

### Phase 2: SSTable Integration (Week 2)

1. **Modified MemTable and WAL**
   - Store `(value, KvKind)` entries so GC pointer rewrites are unambiguous
   - Encode `KvKind` in WAL records for crash recovery
   - Preserve normal user writes as `KvKind::Inline` full values

2. **Modified SSTableBuilder**
   - Add `ValueLogBuilder` integration
   - Threshold-based value separation
   - Track which vLog files are referenced via `referenced_vlogs: HashSet<u32>`
   - Register SST -> vLog mapping via `register_sst_references()` when SST is finalized

3. **Modified SsTable and SsTableIterator**
   - Detect and decode `ValuePointer` values
   - Transparent value fetching from vLog
   - Iterator support for separated values

4. **ValueLog Manager**
   - Lifecycle management of vLog files
   - Reference tracking from SSTs
   - File caching and cleanup

### Phase 3: Garbage Collection (Week 3)

1. **GC Analysis**
   - Scan vLog files to find live/dead entries
   - Calculate space reclamation statistics
   - Trigger policies

2. **GC Execution**
   - Rewrite live entries to new vLog files
   - Re-insert updated pointers into LSM tree via normal writes
   - Defer old file deletion until snapshots are quiesced

3. **Background GC Thread**
   - Optional background GC processing
   - Rate limiting and I/O scheduling
   - Progress tracking and metrics

### Phase 4: Testing and Optimization (Week 4)

1. **Unit Tests**
   - Value pointer encoding/decoding
   - vLog file format correctness
   - GC correctness with various workloads

2. **Integration Tests**
   - End-to-end workflows
   - Crash recovery testing
   - Concurrent read/write scenarios

3. **Performance Benchmarks**
   - Compare with/without key-value separation
   - Measure write amplification reduction
   - Analyze read latency impact

## API Changes

### New Public Types

```rust
pub mod vlog {
    pub struct ValuePointer { ... }
    pub struct ValueSeparationOptions { ... }
    pub struct ValueLogStats { ... }
}
```

### Modified Types

```rust
pub struct LsmStorageOptions {
    // ... existing fields ...
    
    /// Options for key-value separation
    pub value_separation: ValueSeparationOptions,
}
```

### New Storage Methods

```rust
impl MiniLsm {
    /// Get statistics about value log usage
    pub fn vlog_stats(&self) -> ValueLogStats;

    /// Get a value along with its authoritative KvKind metadata.
    /// Returns None if the key is deleted. Used by GC's is_entry_live() to avoid
    /// ambiguous payload-based pointer detection.
    pub fn get_with_kind(&self, key: &[u8]) -> Result<Option<(Bytes, KvKind)>>;

    /// Trigger manual garbage collection
    pub fn trigger_gc(&self) -> Result<()>;

    /// Atomically replace `key` only if the current value equals `old`.
    /// Returns true if the swap succeeded, false if the value changed.
    /// Used by GC to avoid overwriting fresher user writes during re-insertion.
    pub fn compare_and_set(&self, key: &[u8], old: &[u8], new: &[u8]) -> Result<bool>;

    /// Kind-aware CAS: checks both value bytes AND KvKind.
    /// Prevents GC from overwriting an inline user value that happens to be
    /// byte-identical to an encoded ValuePointer (17 bytes starting with 0xFF).
    pub fn compare_and_set_with_kind(
        &self, key: &[u8], old: &[u8], old_kind: KvKind, new: &[u8], new_kind: KvKind,
    ) -> Result<bool>;
}
```

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_value_pointer_encoding() {
    let ptr = ValuePointer {
        file_id: 42,
        offset: 1024,
        size: 256,
    };
    let mut buf = Vec::new();
    ptr.encode(&mut buf);
    let decoded = ValuePointer::decode(&buf);
    assert_eq!(ptr, decoded);
}

#[test]
fn test_vlog_write_read() {
    let temp_dir = tempfile::tempdir().unwrap();
    let vlog = ValueLog::open(temp_dir.path(), Default::default()).unwrap();
    
    let key = b"test_key";
    let value = vec![0u8; 4096]; // Large value
    
    let ptr = vlog.write(key, &value).unwrap();
    let read_value = vlog.read(&ptr, key).unwrap();
    
    assert_eq!(value, read_value.as_ref());
}
```

### Integration Tests

```rust
#[test]
fn test_key_value_separation_workflow() {
    let dir = tempfile::tempdir().unwrap();
    let options = LsmStorageOptions {
        value_separation: ValueSeparationOptions {
            enabled: true,                // Enable for this test
            min_value_size: 100,
            ..Default::default()
        },
        ..Default::default()
    };
    
    let storage = MiniLsm::open(&dir, options).unwrap();
    
    // Write small value (inline)
    storage.put(b"small", b"tiny").unwrap();
    
    // Write large value — stored in WAL + memtable, separated to vLog on flush
    let large_value = vec![0u8; 10000];
    storage.put(b"large", &large_value).unwrap();

    // Force flush — this is where vLog write + separation happens
    storage.force_flush().unwrap();
    
    // Verify both values can be read
    assert_eq!(storage.get(b"small").unwrap().unwrap(), b"tiny");
    assert_eq!(storage.get(b"large").unwrap().unwrap(), large_value);
}
```

## Compatibility and Migration

### Forward Compatibility

- Disabled by default in existing configurations
- Can be enabled on existing databases (new writes use separation)
- Existing inline values remain unchanged

### Format Versioning

```rust
/// Database format version
const FORMAT_VERSION: u32 = 2; // Increment from 1

/// SSTable footer extension for vLog metadata
pub struct SsTableFooter {
    pub format_version: u32,
    pub has_vlog_references: bool,
    pub vlog_file_ids: Vec<u32>,
}
```

### Manifest Changes

Manifest records are extended to carry the SST → vLog reference set so that
recovery can rebuild `sst_to_vlogs` directly from the manifest log instead of
re-opening every SST footer. This keeps startup O(manifest size) rather than
O(total SST count) once vLog adoption grows.

```rust
#[derive(Serialize, Deserialize)]
pub enum ManifestRecord {
    /// Flush of a memtable to L0. Original variant kept for backward compat.
    Flush(usize),

    /// Flush with vLog references. Named fields so serde_json can deserialize
    /// even when the `vlogs` key is absent from old manifests.
    FlushV2 { sst_id: usize, #[serde(default)] vlogs: Vec<u32> },

    NewMemtable(usize),

    /// Compaction output. For each output SST, record the set of vLog files
    /// it references so the SST → vLog map is reconstructable from the
    /// manifest alone.
    // NOTE: The manifest uses serde_json, and #[serde(default)] on a tuple
    // variant field does NOT work with serde_json (it cannot deserialize a
    // single-element JSON array into a two-element Rust tuple). To preserve
    // backward compatibility with existing manifests, keep the original
    // variants unchanged and introduce new V2 variants with named fields.
    // The recovery code should accept both old and new variants.
    Compaction(CompactionTask, Vec<usize>),
    CompactionV2 { task: CompactionTask, output_ssts: Vec<(usize, Vec<u32>)> },

    /// vLog file lifecycle. `NewVlogFile` records that a file ID was allocated
    /// so future rotations do not reuse it; it is not a liveness root by itself.
    NewVlogFile(u32),
    DeleteVlogFile(u32),
}
```

Recovery walks the manifest as before; for every `Flush` / `FlushV2` /
`Compaction` / `CompactionV2` record it calls `register_sst_references(sst_id,
vlog_ids)` to populate **both** `sst_to_vlogs` and `vlog_to_ssts` indexes.
Additionally, for compaction records, it extracts the input SST IDs from the
`CompactionTask` and calls `unregister_sst_references` for each, preventing
reference leaks that would block vLog garbage collection.
Old `Flush(usize)` and `Compaction(usize, Vec<usize>)` records are treated as
having an empty vLog set. SSTable footers still embed the vLog reference list
as a redundant copy, used by `fsck`-style consistency checks and by older
snapshots whose manifest record predates this format.

`NewVlogFile` records participate in ID allocation and crash recovery, but not
data liveness. A vLog file is live only if it is referenced by an SST reference
set or by a WAL-recovered memtable entry with `KvKind::ValuePointer`.

## Crash Recovery

The WAL stores full values for user writes and stores kind-tagged pointer
entries for GC CAS rewrites. Recovery proceeds in three phases:

1. **WAL replay**: rebuilds the memtable with `(value, KvKind)` entries. User writes replay as `KvKind::Inline` full values; GC CAS rewrites replay as `KvKind::ValuePointer` entries.
2. **Manifest replay**: for every `Flush` / `FlushV2` / `Compaction` / `CompactionV2` record, call `register_sst_references(sst_id, vlog_ids)` to populate both `sst_to_vlogs` and `vlog_to_ssts` indexes.
3. **Orphan vLog cleanup**: scan the data directory for `.vlog` files. Any file not referenced by any SST's vLog reference list **and** not referenced by the WAL-recovered memtable is orphaned and deleted. `NewVlogFile` alone is not enough to keep a file: a crash after file allocation but before `FlushV2`/GC CAS would otherwise leak a fully unreferenced file.

**Flush-time crash safety**: vLog writes are fsynced (batch, once per flush) before the SST is written to disk. If a crash occurs:
- **Before SST is committed to manifest**: the flush is incomplete — the memtable (rebuilt from WAL) still contains the full values. The partially written vLog and SST files are orphaned.
- **After SST is committed to manifest**: all vLog pointers in the SST are guaranteed valid (vLog was fsynced first).
- **Partial vLog write**: detected by CRC32 mismatch and skipped during reads.

**GC crash safety**: GC's `compact_file` fsyncs the new vLog BEFORE performing any CAS operations (Phase 1 → Phase 2 ordering). Each successful `compare_and_set_with_kind` appends a kind-tagged WAL entry before exposing the pointer in the memtable. On crash:
- **Before CAS**: WAL replay restores the old ValuePointer (pre-CAS state). The new vLog file is orphaned but harmless — it will be cleaned up on the next GC pass or startup orphan sweep.
- **After CAS, before SST flush**: WAL replay restores the NEW ValuePointer (post-CAS state). This is safe because the new vLog was fsynced before the CAS (Phase 1 ordering), so the pointer is always valid.
- **After CAS is flushed to SST**: the new ValuePointer is durable in the SST. The new vLog file is referenced by the SST and will not be deleted.

### WAL Interaction

The WAL stores `(key, value, KvKind)` for every write. Normal user writes store
the full value with `KvKind::Inline`, preserving the same latency profile as a
non-separated LSM tree. GC CAS rewrites store the encoded `ValuePointer` with
`KvKind::ValuePointer` so recovery can reconstruct the post-GC memtable state
without payload sniffing. Value separation for normal user writes still happens
**during flush** (memtable → SST), not during the put path:

- **Write path (put)**: value + `KvKind::Inline` → WAL → **WAL fsync** → memtable
- **GC CAS path**: encoded `ValuePointer` + `KvKind::ValuePointer` → WAL → memtable
- **Flush path**: scan memtable → for each entry with `value.len() >= min_value_size`:
  1. append value to vLog buffer (in-memory)
  2. add `ValuePointer` to in-memory SST block
  3. after all entries are processed: **vLog fsync** (once, batch)
  4. write SST file to disk
- **Small values** (< `min_value_size`): written inline to SST as before, no vLog involvement

This design avoids the double-fsync problem entirely. The write path has exactly one fsync (WAL), identical to a non-separated engine. The vLog fsync cost is paid once per flush (not per entry), which is a background operation and does not add latency to the put path.

**Tradeoff**: user writes still store full values in the WAL, increasing WAL
size and recovery time for workloads with many large values. GC rewrites are the
exception: they store compact pointer entries because the referenced vLog entry
has already been fsynced.

**Crash recovery**:
- **Crash before flush**: WAL replay rebuilds the memtable with full values — no vLog pointers to validate.
- **Crash during flush**: the flush is atomic — either the SST (with valid vLog pointers) is committed to the manifest, or it is not. Partial vLog writes are detected by CRC32 mismatch and skipped.
- **No dangling pointers**: because vLog writes are fsynced (batch, once per flush) before the SST is written, every `ValuePointer` in every SST is guaranteed to reference durable data.
- **Orphan cleanup**: on startup, any `.vlog` file not referenced by any SST reference set or by the WAL-recovered memtable is deleted. `NewVlogFile` records alone do not keep files live.

## Performance Considerations

### Write Path

| Operation | Latency Impact | Notes |
|-----------|---------------|-------|
| Small value (< threshold) | None | Stored inline, same as non-separated |
| Large value | None | Full value goes to WAL + memtable, no vLog on write path |
| Flush (background) | +1 fsync per flush (batch) | vLog fsync once before SST write; does not block put path |

### Read Path

| Scenario | Latency Impact | Mitigation |
|----------|---------------|------------|
| Point get (large value) | +1 seek | vLog reader cache |
| Range scan (keys only) | **Improved** | No value scanning |
| Range scan (full) | Similar | Prefetching for sequential reads |

### Compaction

| Metric | Improvement |
|--------|-------------|
| Write amplification | 5-10x reduction for large values |
| I/O throughput | ~10x improvement (less data moved) |
| CPU usage | Reduced (smaller sorting) |

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| vLog file corruption | Data loss | CRC32 checksums + validation |
| GC overhead | Latency spikes | Background GC + rate limiting |
| Space amplification | Temporary bloat | Configurable GC threshold |
| Recovery complexity | Longer startup | vLog index + incremental recovery |

## Future Work

1. **Compression**: Compress values in vLog to reduce space
2. **Hot/Cold Separation**: Tiered storage for frequently accessed values
3. **Parallel GC**: Concurrent garbage collection across multiple files
4. **vLog Index**: In-memory index for faster lookups
5. **Value Caching**: Dedicated cache for hot values
6. **Batch GC CAS**: Batch `compare_and_set_with_kind` calls for live entries during GC to reduce atomic operation overhead and contention with user writes
7. **Lock-free vLog writer**: Replace the `active_writer` Mutex with a lock-free append buffer, dedicated writer thread with request channel, or multiple active vLog files to improve write concurrency
8. **Pre-created vLog rotation**: Prepare the next vLog file in the background so rotation doesn't block the writer with synchronous file creation
9. **Remove VALUE_POINTER_TAG**: If the 1-byte tag overhead becomes a bottleneck, remove it and rely solely on KvKind for classification (encoded size drops from 17 to 16 bytes)

## References

1. [WiscKey: Separating Keys from Values in SSD-conscious Storage](https://www.usenix.org/system/files/conference/fast16/fast16-papers-lu.pdf) - Lu et al., FAST 2016
2. [BadgerDB Documentation](https://dgraph.io/docs/badger/design/) - Dgraph Labs
3. [RocksDB BlobDB](https://github.com/facebook/rocksdb/wiki/BlobDB) - Facebook
4. [Titan: A RocksDB Plugin for Large Values](https://pingcap.com/blog/titan-storage-engine-design-and-implementation) - PingCAP
5. [Pebble Value Separation](https://www.cockroachlabs.com/blog/pebble-key-value-separation/) - Cockroach Labs

---

## Appendix A: File Layout

```
data/
├── MANIFEST
├── 00001.sst
├── 00002.sst
├── ...
├── 00001.vlog      # NEW: Value log files
├── 00002.vlog
├── 00001.wal
└── vlog_index/     # NEW: Optional vLog index
    └── 00001.idx
```

## Appendix B: Configuration Examples

### Development (low memory)

```rust
ValueSeparationOptions {
    enabled: true,
    min_value_size: 512,
    max_value_size: 128 << 20,     // 128MB
    max_vlog_file_size: 16 << 20,  // 16MB
    gc_threshold_ratio: 0.3,       // Aggressive GC
    max_open_vlog_files: 16,
}
```

### Production (large values)

```rust
ValueSeparationOptions {
    enabled: true,
    min_value_size: 4096,          // 4KB
    max_value_size: 128 << 20,     // 128MB
    max_vlog_file_size: 256 << 20, // 256MB
    gc_threshold_ratio: 0.5,
    max_open_vlog_files: 128,
}
```

### Disabled (backward compatible)

```rust
ValueSeparationOptions {
    enabled: false,
    ..Default::default()
}
```
