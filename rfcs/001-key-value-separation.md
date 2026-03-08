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
    /// Size of the encoded value entry (for validation)
    pub size: u32,
}

impl ValuePointer {
    /// Encode to bytes for storage in LSM tree
    pub fn encode(&self, buf: &mut Vec<u8>) {
        buf.put_u32(self.file_id);
        buf.put_u64(self.offset);
        buf.put_u32(self.size);
    }

    /// Decode from bytes
    pub fn decode(mut buf: &[u8]) -> Self {
        Self {
            file_id: buf.get_u32(),
            offset: buf.get_u64(),
            size: buf.get_u32(),
        }
    }

    /// Total encoded size: 16 bytes
    pub const fn encoded_size() -> usize {
        4 + 8 + 4 // 16 bytes
    }
}
```

### 2. Value Log File Format

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Value Log Entry Format                           │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌───────────┬─────────┬───────────┬───────────┬───────────┐       │
│  │ Header    │ Key     │ Value     │ Trailer   │ Padding   │       │
│  │ (16 bytes)│ (var)   │ (var)     │ (4 bytes) │ (0-7 bytes)│       │
│  └───────────┴─────────┴───────────┴───────────┴───────────┘       │
│                                                                     │
│  Header Format:                                                     │
│  ┌─────────────┬───────────────┬─────────────┬──────────────┐      │
│  │ crc32       │ key_length    │ value_length│ meta_flags   │      │
│  │ (4 bytes)   │ (2 bytes)     │ (4 bytes)   │ (6 bytes)    │      │
│  └─────────────┴───────────────┴─────────────┴──────────────┘      │
│                                                                     │
│  Trailer: CRC32 checksum of the entire entry (for validation)       │
│                                                                     │
│  Alignment: Entries are 8-byte aligned for efficient disk access    │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

```rust
/// Magic number for vLog file header
const VLOG_MAGIC: u32 = 0x564C4F47; // "VLOG"

/// Value log file header (first 16 bytes of each vLog file)
#[derive(Clone, Debug)]
pub struct VlogFileHeader {
    pub magic: u32,
    pub version: u16,
    pub reserved: u10,
}

/// Entry header (precedes each key-value pair)
#[repr(C, packed)]
pub struct VlogEntryHeader {
    pub crc32: u32,           // CRC32 of key + value
    pub key_len: u16,         // Key length (max 64KB)
    pub value_len: u32,       // Value length (max 4GB)
    pub flags: u16,           // Flags (tombstone, etc.)
}

const HEADER_SIZE: usize = std::mem::size_of::<VlogEntryHeader>();
const TRAILER_SIZE: usize = 4; // CRC32 checksum
const ALIGNMENT: usize = 8;
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
            enabled: true,
            min_value_size: 1024,        // 1KB threshold
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
    vlog_options: Option<ValueSeparationOptions>,
    vlog_builder: Option<ValueLogBuilder>,
    current_vlog_id: u32,
    vlog_entries: Vec<(ValuePointer, Bytes)>, // Pending entries
}

impl SsTableBuilder {
    pub fn add(&mut self, key: KeySlice, value: &[u8]) {
        if self.first_key.is_empty() {
            self.first_key.set_from_slice(key);
        }

        self.key_hashes.push(farmhash::fingerprint32(key.raw_ref()));

        // NEW: Check if value should be separated
        let value_to_store = if self.should_separate_value(value) {
            let vptr = self.write_to_vlog(key, value);
            // Store the encoded pointer instead of the value
            vptr.encode(&mut self.vlog_buffer);
            &self.vlog_buffer[..ValuePointer::encoded_size()]
        } else {
            value
        };

        if self.builder.add(key, value_to_store) {
            self.last_key.set_from_slice(key);
            return;
        }

        self.finish_block();
        assert!(self.builder.add(key, value_to_store));
        self.last_key.set_from_slice(key);
    }

    fn should_separate_value(&self, value: &[u8]) -> bool {
        match &self.vlog_options {
            Some(opts) if opts.enabled => value.len() >= opts.min_value_size,
            _ => false,
        }
    }
}
```

### 6. ValueLog Implementation

```rust
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
    
    /// Tracks which SSTs reference which vLog entries
    /// Used for garbage collection
    sst_to_vlogs: RwLock<HashMap<usize, HashSet<u32>>>,
}

impl ValueLog {
    /// Write a key-value pair to the active vLog file.
    /// Returns a ValuePointer that can be stored in the LSM tree.
    pub fn write(&self, key: &[u8], value: &[u8]) -> Result<ValuePointer> {
        let mut writer = self.active_writer.lock();
        
        // Rotate to new file if current is full
        if writer.size() >= self.options.max_vlog_file_size {
            writer = self.rotate_vlog_file(writer)?;
        }
        
        writer.append(key, value)
    }

    /// Read a value using a ValuePointer.
    pub fn read(&self, ptr: &ValuePointer) -> Result<Bytes> {
        let reader = self.get_reader(ptr.file_id)?;
        reader.read_at(ptr.offset, ptr.size)
    }

    /// Get a cached reader for the specified vLog file.
    fn get_reader(&self, file_id: u32) -> Result<Arc<ValueLogReader>> {
        self.readers.try_get_with(file_id, || {
            ValueLogReader::open(self.path_of_file(file_id))
        }).map_err(|e| anyhow!("Failed to open vlog {}: {}", file_id, e))
    }
}
```

### 7. Garbage Collection

Garbage collection is triggered during compaction when the ratio of stale data exceeds a threshold.

```rust
/// Garbage collector for reclaiming space in value logs.
pub struct GarbageCollector {
    vlog: Arc<ValueLog>,
    threshold: f64,
}

impl GarbageCollector {
    /// Analyze a vLog file and determine which entries are still live.
    /// Returns the ratio of live data.
    pub fn analyze_file(&self, file_id: u32) -> Result<GcAnalysis> {
        let reader = self.vlog.get_reader(file_id)?;
        let mut live_entries = Vec::new();
        let mut dead_bytes = 0;
        let mut live_bytes = 0;

        for entry in reader.iter() {
            if self.is_entry_live(&entry)? {
                live_entries.push(entry.ptr);
                live_bytes += entry.size;
            } else {
                dead_bytes += entry.size;
            }
        }

        let total = live_bytes + dead_bytes;
        let live_ratio = if total > 0 { live_bytes as f64 / total as f64 } else { 1.0 };

        Ok(GcAnalysis {
            file_id,
            live_ratio,
            live_entries,
            dead_bytes,
        })
    }

    /// Rewrite live entries to a new vLog file and update pointers in SSTs.
    pub fn compact_file(&self, analysis: &GcAnalysis) -> Result<()> {
        if analysis.live_ratio > self.threshold {
            return Ok(()); // No need to compact
        }

        // Create new vLog file with live entries
        let new_file_id = self.vlog.next_file_id();
        let mut writer = ValueLogWriter::create(self.vlog.path_of_file(new_file_id))?;
        
        // Map: old_ptr -> new_ptr
        let mut pointer_map: HashMap<ValuePointer, ValuePointer> = HashMap::new();

        for old_ptr in &analysis.live_entries {
            let value = self.vlog.read(old_ptr)?;
            let new_ptr = writer.append_raw(&value)?;
            pointer_map.insert(*old_ptr, new_ptr);
        }

        writer.close()?;

        // Update SSTs that reference this vLog file
        self.update_sst_pointers(analysis.file_id, &pointer_map)?;

        // Remove old vLog file
        self.vlog.remove_file(analysis.file_id)?;

        Ok(())
    }

    /// Check if a vLog entry is still referenced by the LSM tree.
    fn is_entry_live(&self, entry: &VlogEntry) -> Result<bool> {
        // Query the LSM tree to see if this key still references this vLog entry
        let key = &entry.key;
        match self.lsm.get(key)? {
            Some(value) => {
                // Decode value pointer and check if it matches
                if value.len() == ValuePointer::encoded_size() {
                    let ptr = ValuePointer::decode(&value);
                    Ok(ptr.file_id == entry.ptr.file_id && ptr.offset == entry.ptr.offset)
                } else {
                    Ok(false) // Value is now inline
                }
            }
            None => Ok(false), // Key was deleted
        }
    }
}
```

### 8. Integration with Compaction

```rust
impl CompactionController {
    /// After compaction, trigger garbage collection for affected vLog files.
    pub fn post_compaction_gc(
        &self,
        input_ssts: &[usize],
        output_ssts: &[usize],
        vlog: &Arc<ValueLog>,
    ) -> Result<()> {
        // Collect all vLog files referenced by input SSTs
        let mut affected_vlogs: HashSet<u32> = HashSet::new();
        
        for sst_id in input_ssts {
            if let Some(vlogs) = vlog.get_sst_references(*sst_id) {
                affected_vlogs.extend(vlogs);
            }
        }

        // Run GC analysis on affected files
        let gc = GarbageCollector::new(vlog.clone());
        for file_id in affected_vlogs {
            let analysis = gc.analyze_file(file_id)?;
            if analysis.live_ratio < vlog.options().gc_threshold_ratio {
                gc.compact_file(&analysis)?;
            }
        }

        // Update references for output SSTs
        for sst_id in output_ssts {
            vlog.update_sst_references(*sst_id)?;
        }

        Ok(())
    }
}
```

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

1. **Modified SSTableBuilder**
   - Add `ValueLogBuilder` integration
   - Threshold-based value separation
   - Track which vLog files are referenced

2. **Modified SsTable and SsTableIterator**
   - Detect and decode `ValuePointer` values
   - Transparent value fetching from vLog
   - Iterator support for separated values

3. **ValueLog Manager**
   - Lifecycle management of vLog files
   - Reference tracking from SSTs
   - File caching and cleanup

### Phase 3: Garbage Collection (Week 3)

1. **GC Analysis**
   - Scan vLog files to find live/dead entries
   - Calculate space reclamation statistics
   - Trigger policies

2. **GC Execution**
   - Rewrite live entries to new files
   - Update SST value pointers
   - Atomic file replacement

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
    
    /// Trigger manual garbage collection
    pub fn trigger_gc(&self) -> Result<()>;
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
    let read_value = vlog.read(&ptr).unwrap();
    
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
            enabled: true,
            min_value_size: 100,
            ..Default::default()
        },
        ..Default::default()
    };
    
    let storage = MiniLsm::open(&dir, options).unwrap();
    
    // Write small value (inline)
    storage.put(b"small", b"tiny").unwrap();
    
    // Write large value (separated)
    let large_value = vec![0u8; 10000];
    storage.put(b"large", &large_value).unwrap();
    
    // Force flush to create SST
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

```rust
#[derive(Serialize, Deserialize)]
pub enum ManifestRecord {
    Flush(usize),
    NewMemtable(usize),
    Compaction(CompactionTask, Vec<usize>),
    // NEW: Track vLog file lifecycle
    NewVlogFile(u32),
    DeleteVlogFile(u32),
}
```

## Performance Considerations

### Write Path

| Operation | Latency Impact | Notes |
|-----------|---------------|-------|
| Small value (< threshold) | None | Stored inline as before |
| Large value | +1 disk write | Sequential write to vLog |
| Flush | Neutral | Sequential vLog writes are fast |

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
