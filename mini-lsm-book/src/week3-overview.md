<!--
  mini-lsm-book © 2022-2026 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Week 3 Overview: Multi-Version Concurrency Control

In this part, you will implement multi-version concurrency control (MVCC) over the LSM engine from the previous two weeks. Internal keys will carry timestamps so that the engine can retain several versions of one user key. Reads will select a version from a stable timestamp, while compaction will reclaim versions only after no active reader needs them.

The general approach of the MVCC part in this course is inspired and partially based on [BadgerDB](https://github.com/dgraph-io/badger).

The central representation is `user_key + timestamp (u64)`. Versions sort by user key and then by timestamp in descending order, so the newest version of one user key appears first. User-facing APIs hide this representation and expose a snapshot through a transaction.

Before MVCC, source priority determined which duplicate key was newest. With MVCC, the timestamp makes version order explicit. Source priority still matters when two sources contain the same internal key, but compaction must not collapse distinct timestamps merely because they share a user key. This course also preserves the invariant that newer sources remain above older sources when compaction does not include both.

An MVCC engine can assign timestamps itself or accept timestamps from a caller. Using BadgerDB's terminology, the mode that hides timestamps is **unmanaged mode**, while the mode that exposes timestamp control is **managed mode**.

**Managed mode APIs**
```
get(key, read_timestamp) -> (value, write_timestamp)
scan(key_range, read_timestamp) -> iterator<key, value, write_timestamp>
put(key, value, write_timestamp)
delete(key, write_timestamp)
write_batch(records, write_timestamp)
set_watermark(timestamp)
```

**Unmanaged mode APIs**
```
get(key) -> value
scan(key_range) -> iterator<key, value>
start_transaction() -> txn
txn.get/scan(key or range)
txn.put(key, value)
txn.delete(key)
txn.commit()
```

Managed mode requires the caller to provide timestamps. They might come from a centralized timestamp service or an upstream log such as PostgreSQL logical replication. The caller must also advance a watermark that tells the engine which historical versions are no longer needed.

In unmanaged mode, the engine chooses timestamps. A transaction records the latest committed timestamp when it begins. Later commits remain invisible to that transaction, so every read observes the same logical snapshot.

The first three chapters refactor internal formats and finish snapshot reads. The remaining chapters track active snapshots, add transactional writes and validation, and reclaim obsolete data.

| Chapter | Before | After |
| --- | --- | --- |
| [Day 1: Timestamp Key Refactor](./week3-01-ts-key-refactor.md) | Internal keys contain only user bytes. | Blocks, SST metadata, iterators, and memtables preserve descending timestamp order. |
| [Day 2: Memtables and Timestamps](./week3-02-snapshot-read-part-1.md) | Most data still uses timestamp 0. | Writes receive one commit timestamp per batch and all versions survive compaction. |
| [Day 3: Transaction API](./week3-03-snapshot-read-part-2.md) | Reads return only the newest global state. | Transactions select the newest visible version at a fixed read timestamp, including after recovery. |
| [Day 4: Watermark and Garbage Collection](./week3-04-watermark.md) | Compaction retains every historical version. | Compaction retains exactly the versions active snapshots can still observe. |
| [Day 5: Transactional Writes](./week3-05-txn-occ.md) | Transaction writes are not private. | A transaction reads its own workspace and publishes its updates with one commit timestamp. |
| [Day 6: Serializable Validation](./week3-06-serializable.md) | Snapshot isolation permits write skew. | Commit-time validation rejects read/write conflicts for tracked keys. |
| [Day 7: Compaction Filters](./week3-07-compaction-filter.md) | Garbage collection is based only on version age. | User-installed filters can reclaim a logical key prefix during compaction. |

## How to Use This Week

MVCC bugs often return a plausible value at the latest timestamp while breaking an older snapshot. For each chapter:

1. Write down the internal order for two user keys with several timestamps.
2. Trace one overwrite and one tombstone through a point read, a bounded scan, a flush, and a compaction.
3. Keep one old transaction alive while newer batches commit, then repeat the read after flushing and compacting.
4. For persistence changes, identify the durable source of the recovered commit timestamp and distinguish timestamp visibility from crash atomicity.
5. For serializable validation, draw the dependency that should make a transaction abort and state which read or write set records it.

Before finishing Week 3, check that you can explain:

- why timestamps sort in descending order within one user key;
- how included and excluded user-key bounds map to internal timestamp bounds;
- which version a read at timestamp `T` returns when newer versions and tombstones exist;
- why the watermark preserves one version at or below its value;
- why one commit timestamp makes a completed batch visible together, but per-record WAL writes do not provide crash atomicity;
- what anomaly commit-time validation prevents and which scan phantoms it does not prevent; and
- why a compaction filter cannot blindly remove versions newer than the watermark.

{{#include copyright.md}}
