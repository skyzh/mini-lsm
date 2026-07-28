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

## Separate the Guarantees

MVCC is a mechanism for storing and selecting versions; by itself, it does not define all transaction guarantees. Keep these questions separate as you work through the week:

1. **Version order:** Which versions exist, and how are they ordered in memtables and SSTs?
2. **Snapshot visibility:** Given `read_ts`, which committed version may a read observe? A stable snapshot prevents later commits from appearing halfway through a transaction.
3. **Atomic commit:** Do all writes in one transaction become visible together? Mini-LSM uses one commit timestamp for atomic visibility and one framed WAL batch for all-or-nothing recovery. These are related but distinct guarantees.
4. **Durability:** After which synchronization point must a successful commit survive a crash? A well-framed WAL record can still be lost if it was not made durable.
5. **Isolation:** Is the outcome of concurrent transactions equivalent to some serial order? Stable snapshots alone do not ensure this; write skew is a counterexample.

Classic **snapshot isolation** gives each transaction the committed snapshot from its start, includes its own writes, and prevents two concurrent transactions that update the same item from both committing. Even with that write-write rule, transactions that write different items can produce write skew. See the background and write-skew example in [Serializable Isolation for Snapshot Databases](https://www.cs.cornell.edu/~sowell/dbpapers/serializable_isolation.pdf).

Mini-LSM builds these properties incrementally. Days 1–4 implement version ordering, stable snapshots, and safe version reclamation. Day 5 adds a private workspace plus atomic visibility and crash-atomic WAL batches; it does not yet validate concurrent decisions. Day 6 adds conservative commit-time validation for point-key dependencies. Because scans record returned keys rather than the gaps in a range predicate, the completed exercise does **not** provide full serializability for scan-heavy workloads. This scope is part of the lesson, not an implementation detail to hide.

The first three chapters refactor internal formats and finish snapshot reads. The remaining chapters track active snapshots, add transactional writes and validation, and reclaim obsolete data.

| Chapter | Before | After |
| --- | --- | --- |
| [Day 1: Timestamp Key Refactor](./week3-01-ts-key-refactor.md) | Internal keys contain only user bytes. | Blocks, SST metadata, iterators, and memtables preserve descending timestamp order. |
| [Day 2: Memtables and Timestamps](./week3-02-snapshot-read-part-1.md) | Most data still uses timestamp 0. | Writes receive one commit timestamp per batch and all versions survive compaction. |
| [Day 3: Transaction API](./week3-03-snapshot-read-part-2.md) | Reads return only the newest global state. | Transactions select the newest visible version at a fixed read timestamp, including after recovery. |
| [Day 4: Watermark and Garbage Collection](./week3-04-watermark.md) | Compaction retains every historical version. | Compaction retains exactly the versions active snapshots can still observe. |
| [Day 5: Transaction Workspace and Atomic Commit](./week3-05-txn-occ.md) | Transaction writes are neither private nor crash-atomic. | A transaction reads its own workspace and commits one timestamped, framed WAL batch. |
| [Day 6: Serializable Validation](./week3-06-serializable.md) | Stable snapshots still permit write skew. | Commit-time validation rejects read/write conflicts for tracked keys. |
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
- why a shared commit timestamp provides atomic visibility while a framed, checksummed WAL batch provides crash atomicity;
- what anomaly commit-time validation prevents and which scan phantoms it does not prevent; and
- why a compaction filter cannot blindly remove versions newer than the watermark.

## End-of-Week Self-Check

For each scenario, name the relevant guarantee before answering. This is as important as computing the visible value.

### 1. Snapshot Visibility and Tombstones

The internal stream contains `k@9=delete, k@7=v7, k@3=v3`. What does a read return at timestamps 10, 8, 7, and 2?

<details>

<summary>Answer criteria</summary>

At 10, the newest visible version is the tombstone at 9, so the key is absent. At 8 and 7, `v7` is visible. At 2, no version is visible. A correct iterator skips versions newer than `read_ts`, chooses the first remaining version, and never continues past a chosen tombstone to resurrect an older value.

</details>

### 2. Watermark Garbage Collection

For the same versions and watermark 7, which versions must a non-bottom compaction retain? What may a bottom-level compaction do after the watermark advances to 9?

<details>

<summary>Answer criteria</summary>

At watermark 7, retain every version above it (`k@9`) and the newest version at or below it (`k@7`); `k@3` is obsolete. After the watermark reaches 9, a bottom-level compaction may remove the selected tombstone and all older versions because no active snapshot needs them and no older value can survive below the task.

</details>

### 3. Atomicity Is Not Durability or Serializability

One transaction writes `a` and `b`. Name what one shared commit timestamp guarantees, what one framed and checksummed WAL batch guarantees, and what `sync` adds. Do any of these alone prevent write skew?

<details>

<summary>Answer criteria</summary>

The shared timestamp makes the completed batch visible as one logical version. WAL framing and verification prevent recovery from exposing only a prefix of a torn or corrupt batch. Synchronization establishes the point after which the record must survive a crash. None of these validates dependencies between concurrent transactions, so none alone prevents write skew.

</details>

### 4. Validation Scope

T1 and T2 begin at timestamp 10. T1 reads `b` and writes `a`; T2 reads `a` and writes `b`. T1 commits first. Why should T2 abort? Why can the same key-only scheme miss an insert into an empty scan range?

<details>

<summary>Answer criteria</summary>

T1's committed write set contains `a`, which intersects T2's read set, so T2 must abort to break the write-skew execution. An empty range scan returns no keys to hash; an insertion into the gap therefore has no recorded key intersection. Preventing that phantom requires tracking the predicate or key range, not only returned keys.

</details>

{{#include copyright.md}}
