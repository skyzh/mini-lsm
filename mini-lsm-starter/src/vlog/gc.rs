use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Ok, Result};

use crate::lsm_storage::LsmStorageInner;
use crate::vlog::builder::ValueLogWriter;
use crate::vlog::{KvKind, ValueLog, ValuePointer};

/// Lightweight reference to a live vLog entry (no value payload).
pub struct LiveEntryRef {
    pub ptr: ValuePointer,
    pub key: Vec<u8>,
}

/// Result of analyzing a vLog file for GC.
pub struct GcAnalysis {
    pub file_id: u32,
    pub stale_ratio: f64,
    pub live_entries: Vec<LiveEntryRef>,
    pub dead_bytes: usize,
    pub total_bytes: usize,
}

/// Result of a GC compaction operation.
pub struct GcResult {
    pub old_file_id: u32,
    pub new_file_id: u32,
    pub keys_rewritten: usize,
}

/// Garbage collector for vLog files.
///
/// Scans vLog files to find live vs. dead entries, and rewrites live entries
/// to new vLog files when the stale ratio exceeds the configured threshold.
pub struct GarbageCollector<'a> {
    vlog: &'a Arc<ValueLog>,
    inner: &'a LsmStorageInner,
    threshold: f64,
}

impl<'a> GarbageCollector<'a> {
    pub(crate) fn new(vlog: &'a Arc<ValueLog>, inner: &'a LsmStorageInner, threshold: f64) -> Self {
        Self {
            vlog,
            inner,
            threshold,
        }
    }

    /// Analyze a vLog file to determine live vs. dead entries.
    /// Uses header-only iteration (skips value payloads) for efficiency.
    pub fn analyze_file(&self, file_id: u32) -> Result<GcAnalysis> {
        let reader = self.vlog.get_reader(file_id)?;
        let header_iter = reader.iter_headers()?;

        let mut live_entries = Vec::new();
        let mut dead_bytes = 0usize;
        let mut total_bytes = 0usize;

        for meta_result in header_iter {
            let meta = meta_result?;
            total_bytes += meta.entry_size;

            let is_live = self.check_liveness(&meta.key, &meta.ptr)?;
            if is_live {
                live_entries.push(LiveEntryRef {
                    ptr: meta.ptr,
                    key: meta.key,
                });
            } else {
                dead_bytes += meta.entry_size;
            }
        }

        let stale_ratio = if total_bytes > 0 {
            dead_bytes as f64 / total_bytes as f64
        } else {
            0.0
        };

        Ok(GcAnalysis {
            file_id,
            stale_ratio,
            live_entries,
            dead_bytes,
            total_bytes,
        })
    }

    /// Check if a vLog entry is still live (the LSM tree still points to it).
    pub fn check_liveness(&self, key: &[u8], ptr: &ValuePointer) -> Result<bool> {
        let (current_val, current_kind) = self.inner.get_with_kind(key)?;

        match current_kind {
            KvKind::ValuePointer => {
                if let Some(ref val) = current_val
                    && let Some(current_ptr) = ValuePointer::try_decode(&val[1..])
                {
                    return Ok(current_ptr.file_id == ptr.file_id
                        && current_ptr.offset == ptr.offset
                        && current_ptr.size == ptr.size);
                }
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    /// Compact a vLog file: rewrite live entries to a new file and CAS each key.
    pub fn compact_file(&self, analysis: &GcAnalysis) -> Result<Option<GcResult>> {
        if analysis.stale_ratio < self.threshold {
            return Ok(None);
        }
        if analysis.live_entries.is_empty() {
            // All entries are dead — just schedule deletion
            self.vlog.schedule_deletion(analysis.file_id);
            return Ok(Some(GcResult {
                old_file_id: analysis.file_id,
                new_file_id: u32::MAX, // sentinel: no new file was created
                keys_rewritten: 0,
            }));
        }

        // Phase 1: Read live entries and write to a new vLog file
        let new_file_id = self.vlog.next_file_id();
        let new_path = self.vlog.path_of_file(new_file_id);

        // Wrap compaction in a closure so we can clean up the new file on error
        let compact_res = (|| -> Result<GcResult> {
            let mut writer = ValueLogWriter::create(new_path.clone(), new_file_id)?;

            let mut rewrites: Vec<(Vec<u8>, ValuePointer, ValuePointer)> = Vec::new();

            for live_ref in &analysis.live_entries {
                let (key, value) = self.vlog.read_entry(&live_ref.ptr)?;
                let bytes_written = writer.append(&key, &value)?;
                let new_ptr = ValuePointer {
                    file_id: new_file_id,
                    offset: writer.offset() - bytes_written as u64,
                    size: bytes_written as u32,
                };
                rewrites.push((key, live_ref.ptr, new_ptr));
            }

            // Fsync the new vLog before binding pointers into the LSM tree
            writer.close()?;
            // Sync the directory to ensure the new file's directory entry is durable
            if let std::result::Result::Ok(dir) = std::fs::File::open(&self.vlog.path) {
                let _ = dir.sync_all();
            }

            // Phase 2: CAS each key to point to the new location
            let mut cas_failures = 0usize;
            for (key, old_ptr, new_ptr) in &rewrites {
                let mut old_buf = Vec::with_capacity(1 + ValuePointer::encoded_size());
                old_buf.push(KvKind::ValuePointer as u8);
                old_ptr.encode(&mut old_buf);

                let mut new_buf = Vec::with_capacity(ValuePointer::encoded_size());
                new_ptr.encode(&mut new_buf);

                let swapped = self.inner.compare_and_set_with_kind(
                    key,
                    &old_buf,
                    KvKind::ValuePointer,
                    &new_buf,
                    KvKind::ValuePointer,
                )?;
                if !swapped {
                    cas_failures += 1;
                }
            }

            // Always schedule the old file for deletion. Concurrent writes during GC
            // go to the memtable (not the old vLog), so the old file has no live
            // entries after the CAS loop completes — even if some CAS operations
            // failed due to concurrent overwrites.
            self.vlog.schedule_deletion(analysis.file_id);
            if cas_failures == rewrites.len() {
                // All CAS operations failed — the new vLog file is entirely
                // unreferenced. Schedule it for immediate deletion to avoid leak.
                self.vlog.schedule_deletion(new_file_id);
            }

            Ok(GcResult {
                old_file_id: analysis.file_id,
                new_file_id,
                keys_rewritten: rewrites.len() - cas_failures,
            })
        })();

        match compact_res {
            std::result::Result::Ok(res) => Ok(Some(res)),
            std::result::Result::Err(e) => {
                // Clean up the orphaned new vLog file on error
                let _ = std::fs::remove_file(&new_path);
                Err(e)
            }
        }
    }

    /// Run GC on a specific vLog file: analyze and compact if above threshold.
    pub fn gc_file(&self, file_id: u32) -> Result<Option<GcResult>> {
        if !self.vlog.try_acquire_gc_lock(file_id) {
            return Ok(None);
        }

        let result = (|| -> Result<Option<GcResult>> {
            let analysis = self.analyze_file(file_id)?;
            self.compact_file(&analysis)
        })();

        self.vlog.release_gc_lock(file_id);
        result
    }

    /// Run GC on all vLog files referenced by the current SST set.
    pub fn gc_all(&self) -> Result<Vec<GcResult>> {
        let snapshot = self.inner.state.read().clone();
        let mut vlog_files: HashSet<u32> = HashSet::new();

        // Collect vLog file IDs from all SSTs
        for sst_id in snapshot.sstables.keys() {
            if let Some(refs) = self.vlog.get_sst_references(*sst_id) {
                vlog_files.extend(refs);
            }
        }

        let mut vlog_files: Vec<u32> = vlog_files.into_iter().collect();
        vlog_files.sort_unstable();

        let mut results = Vec::new();
        for file_id in vlog_files {
            if let Some(result) = self.gc_file(file_id)? {
                results.push(result);
            }
        }

        Ok(results)
    }
}
