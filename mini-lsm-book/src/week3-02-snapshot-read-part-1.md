<!--
  mini-lsm-book © 2022-2026 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Snapshot Read - Memtables and Timestamps

By the end of this chapter, you will be able to:

* Refactor your memtable/WAL to store multiple versions of a key.
* Assign one unique commit timestamp to every write batch.
* Preserve every version through compaction while keeping all versions of one user key in one SST.
* Return the newest committed version of each user key from the latest snapshot.

During the refactor, you might need to change the signature of some functions from `&self` to `self: &Arc<Self>` as necessary.

To copy and run the test cases:

```
cargo x copy-test --week 3 --day 2
cargo x scheck
```

The supplied Day 2 test covers timestamped batches, raw version order, and latest-state reads. The compaction boundary case is exercised by the Day 4 suite, once transactions can pin a watermark and keep old versions live in the completed engine.

**Note:** You should also pass every checkpoint through Week 2 Day 4 after finishing this chapter.

## Before You Begin

Day 1 changed the internal representation but still wrote timestamp 0. This chapter turns timestamps into a commit order. Reads still use the latest timestamp; historical snapshots arrive on Day 3.

Keep these invariants in mind:

1. Every record in one write batch receives the same timestamp, and different committed batches receive different timestamps.
2. The write lock covers timestamp allocation, the memtable/WAL update, and publication of the new latest commit timestamp.
3. A scan over a user-key range must include every internal version that could determine the visible value. Included and excluded user-key bounds therefore map to different timestamp sentinels.
4. Compaction retains all versions in this chapter. It may split output between user keys, but never between two versions of the same user key.
5. The WAL checksum covers key length, user-key bytes, timestamp, value length, and value bytes in their encoded byte order.

> **Predict before coding:** A batch writes `a` and `b` at timestamp 7, while the memtable already contains `a@6`. What is their internal order? For the user range `(a, b]`, which timestamp sentinels exclude every version of `a` while including every version of `b`?

## Task 1: MemTable, Write-Ahead Log, and Read Path

In this task, you will need to modify:

```
src/wal.rs
src/mem_table.rs
src/lsm_storage.rs
```

We have already made most of the keys in the engine to be a `KeySlice`, which contains a bytes key and a timestamp. However, some part of our system still did not consider the timestamps. In our first task, you will need to modify your memtable and WAL implementation to take timestamps into account.

You will need to first change the type of the `SkipMap` stored in your memtable.

```rust,no_run
pub struct MemTable {
    // map: Arc<SkipMap<Bytes, Bytes>>,
    map: Arc<SkipMap<KeyBytes, Bytes>>, // Bytes -> KeyBytes
    // ...
}
```

After that, you can continue to fix all compiler errors so as to complete this task.

**MemTable::get**

We keep the get interface so that the test cases can still probe a specific version of a key in the memtable. This interface should not be used in your read path after finishing this task. Given that we store `KeyBytes`, which is `(Bytes, u64)` in the skiplist, while the user probe the `KeySlice`, which is `(&[u8], u64)`. We have to find a way to convert the latter to a reference of the former, so that we can retrieve the data in the skiplist.

To do this, you may temporarily cast the `&[u8]` to a `'static` slice and use `Bytes::from_static` to construct a lookup key without copying. This is sound only because the `Bytes` value is used for the synchronous lookup and cannot escape the original slice's lifetime. Storing or returning that value would make the cast unsound.

<details>

<summary>Spoilers: Convert u8 slice to Bytes</summary>

```rust,no_run
Bytes::from_static(unsafe { std::mem::transmute(key.key_ref()) })
```

</details>

The pre-MVCC map did not need this conversion because `Bytes` implements `Borrow<[u8]>`.

**MemTable::put**

The signature should be changed to `fn put(&self, key: KeySlice, value: &[u8])` and You will need to convert a key slice to a `KeyBytes` in your implementation.

**MemTable::scan**

The signature should be changed to `fn scan(&self, lower: Bound<KeySlice>, upper: Bound<KeySlice>) -> MemTableIterator`. You will need to convert `KeySlice` to `KeyBytes` and use these as `SkipMap::range` parameters.

**MemTable::flush**

Instead of using the default timestamp, you should now use the key timestamp when flushing the memtable to the SST.

**MemTableIterator**

It should now store `(KeyBytes, Bytes)` and the return key type should be `KeySlice`.

**Wal::recover** and **Wal::put**

The write-ahead log should now accept a key slice instead of only user-key bytes. When serializing and deserializing a WAL record, include the timestamp and checksum the exact encoded bytes for every field.

The WAL format is as follows:

```
| key_len (exclude ts len) (u16) | key | ts (u64) | value_len (u16) | value | checksum (u32) |
```

**LsmStorageInner::get**

Previously, `get` could probe sources one at a time. Now a version's timestamp, not only its source, determines visibility. The simplest correct implementation creates the same merged stream as `scan` over memtables, immutable memtables, L0 SSTs, and lower levels, while using Bloom filters to avoid impossible SST probes.

**LsmStorageInner::scan**

Incorporate the new memtable APIs and map user-key bounds to internal-key bounds. An included lower bound starts at `TS_RANGE_BEGIN`; an excluded lower bound must skip every timestamp of that user key. An included upper bound ends at `TS_RANGE_END`; an excluded upper bound stops before `TS_RANGE_BEGIN` for that user key.

## Task 2: Write Path

In this task, you will need to modify:

```
src/lsm_storage.rs
```

We have an `mvcc` field in `LsmStorageInner` that includes all data structures we need to use for multi-version concurrency control in this week. When you open a directory and initialize the storage engine, you will need to create that structure.

In `write_batch`, allocate `latest_commit_ts() + 1` and use it for every record. Hold `self.mvcc().write_lock.lock()` across allocation, insertion, and publication so that concurrent batches cannot reuse a timestamp. After every record has been accepted by the memtable/WAL, publish the new latest commit timestamp.

## Task 3: MVCC Compaction

In this task, you will need to modify:

```
src/compact.rs
```

Previous compaction retained only the newest value and could remove a bottom-level tombstone. With MVCC, older versions may still serve a snapshot, so those rules are not yet safe.

In this chapter, you may simply remove the logic to remove the keys. You may ignore `compact_to_bottom_level` for now, and you should keep ALL versions of a key during the compaction.

Also, you will need to implement the compaction algorithm in a way that the same key with different timestamps are put in the same SST file, *even if* it exceeds the SST size limit. This ensures that if a key is found in an SST in a level, it will not be in other SST files in that level, and therefore simplifying the implementation of many parts of the system.

## Task 4: LSM Iterator

In this task, you will need to modify:

```
src/lsm_iterator.rs
```

In the previous chapter, we implemented the LSM iterator to act as viewing the same key with different timestamps as different keys. Now, we will need to refactor the LSM iterator to only return the latest version of a key if multiple versions of the keys are retrieved from the child iterator.

You will need to record `prev_key` in the iterator. If we already returned the latest version of a key to the user, we can skip all old versions and proceed to the next key.

At this point, you should pass all previous tests except the persistence tests from Week 2 Days 5 and 6.

## Chapter Checkpoint

Each write batch should now create one ordered group of versions, and latest-state reads should collapse that history back to one value per user key.

Verify these cases explicitly:

1. Write the same key in three batches and confirm that a raw internal iterator sees all three timestamps in descending order while `get` returns only the newest value.
2. Scan each combination of included and excluded bounds around a key that has several versions.
3. Force an SST-size boundary in the middle of a key's history and confirm that every version remains in one output SST.
4. Round-trip a WAL record and confirm its checksum covers the encoded timestamp as well as the key and value.

## Test Your Understanding

* What is the difference of `get` in the MVCC engine and the engine you built in week 2?
* In week 2, you stop at the first memtable/level where a key is found when `get`. Can you do the same in the MVCC version?
* How do you convert `KeySlice` into a temporary `KeyBytes` lookup key? Which lifetime condition makes the unsafe conversion sound?
* Why must the write lock cover both timestamp selection and batch insertion?
* Why does an excluded lower bound use the opposite timestamp sentinel from an included lower bound?
* What observable failure occurs if compaction splits two versions of one user key across SSTs in the same level?

We do not provide reference answers to these questions, so feel free to discuss them in the Discord community.

## Bonus Tasks

* **Early Stop for Memtable Gets**. Instead of creating a merge iterator over all memtables and SSTs, we can implement `get` as follows: If we find a version of a key in the memtable, we can stop searching. The same applies to SSTs.

{{#include copyright.md}}
