<!--
  mini-lsm-book © 2022-2026 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Write-Ahead Log (WAL)

![Chapter Overview](./lsm-tutorial/week2-06-overview.svg)

By the end of this chapter, you will be able to:

* Encode memtable updates in a write-ahead log and synchronize its buffered data.
* Record memtable lifetimes in the manifest and recover them from WALs after a restart.
* Restore newest-to-oldest memtable order and a collision-free next SST ID.
* Explain the exact durability guarantee provided by `sync` and `close`.

To copy the test cases into the starter code and run them:

```
cargo x copy-test --week 2 --day 6
cargo x scheck
```

## Before You Begin

The manifest makes the SST layout recoverable, but it cannot restore writes that were still in memory when the process crashed. A WAL persists the contents of each memtable until that memtable is safely represented by an SST.

Keep these invariants in mind:

1. Every WAL record belongs to exactly one memtable, and the WAL ID becomes the SST ID if that memtable is flushed.
2. The manifest records each new memtable so recovery knows which WAL files are live. A flush record retires the corresponding WAL logically before the file is removed physically.
3. Writes may remain in `BufWriter` and the operating-system page cache. They become durable only after `flush()` followed by `sync_all()`.
4. Recovered immutable memtables are ordered newest to oldest, just like memtables created during normal execution.
5. `next_sst_id` is one greater than the maximum ID observed in either SST state or live WAL state.

> **Predict before coding:** The manifest contains `NewMemtable(7)` but no `Flush(7)`, and `00007.wal` contains `k -> v`. What should recovery construct? If `Flush(7)` is durable but the old WAL file was not deleted before the crash, should recovery replay it?

## Task 1: WAL Encoding

In this task, you will need to modify:

```
src/wal.rs
```

The previous chapter persisted the LSM structure and flushed every memtable during a clean close. A crash can bypass that close path. To recover unflushed data, log each memtable update to a write-ahead log. WALs are enabled only when `self.options.enable_wal` is true.

The WAL encoding is a list of key-value pairs.

```
| key_len | key | value_len | value |
```

Implement `recover` to replay the WAL into a memtable and reopen the file for appending.

The WAL uses a `BufWriter` to reduce the number of system calls on the write path. Updating a key does not by itself guarantee that the record has reached durable storage. The engine makes that guarantee when `sync` succeeds. Implement `sync` by first calling `flush()` to move bytes from `BufWriter` to the file and then calling `get_mut().sync_all()` to synchronize the file. You do not need to call `sync_all()` for every individual write.

## Task 2: Integrate WALs

In this task, you will need to modify:

```
src/mem_table.rs
src/wal.rs
src/lsm_storage.rs
```

`MemTable` has an optional WAL. When it is present, append every update to that WAL. If `enable_wal` is true, create each memtable with `create_with_wal` and append `ManifestRecord::NewMemtable` before making the new WAL-backed memtable available for writes.

Store each WAL as `<memtable_id>.wal` in the database directory. If the memtable is later flushed, reuse that ID for its SST.

## Task 3: Recover from the WALs

In this task, you will need to modify:

```
src/lsm_storage.rs
```

If WAL is enabled, recover live memtables from their WALs when opening the database. Also implement the database's `sync` method. After `sync` returns successfully, writes completed before that synchronization point must be recoverable after restart. In this design, synchronizing the current memtable's WAL provides that guarantee because frozen memtables are synchronized when they are replaced.

```
cargo run --bin mini-lsm-cli -- --enable-wal
```

Restore `next_sst_id` as `max{memtable ID, SST ID} + 1`. When WALs are enabled, `close` synchronizes them instead of flushing every memtable to an SST. Stop and join the compaction and flush threads before returning.

## Chapter Checkpoint

With WALs enabled, synchronized writes should survive a restart even when their memtables were never flushed. Recovery should ignore stale WAL files retired by manifest flush records, rebuild live immutable memtables in newest-to-oldest order, create a fresh mutable memtable with a new WAL, and choose an unused ID.

Test more than a clean close: create several memtables, synchronize, reopen, and trace the manifest records that identify each live WAL. Explain which writes are guaranteed to survive if the process stops before `sync`, after `sync`, and after a flush record becomes durable but before its WAL file is removed.

## Test Your Understanding

### Recovery and Durability

* When should you call `fsync` in your engine? What happens if you call `fsync` too often (i.e., on every put key request)?
* How costly is the `fsync` operation in general on an SSD (solid state drive)?
* When can you tell the user that their modifications (put/delete) have been persisted?
* Why must a new memtable be recorded in the manifest before a synchronized write to its WAL can be considered recoverable?
* Why should a flushed memtable's WAL be deleted only after the manifest's flush record is durable?
* Given WAL IDs 4 and 9 plus live SST IDs 3, 7, and 12, what ID should the next memtable use?
* How can you handle corrupted data in WAL?

### Performance and Design

* Is it possible to design an LSM engine without WAL (i.e., use L0 as WAL)? What will be the implications of this design?

We do not provide reference answers to these questions, so feel free to discuss them in the Discord community.

{{#include copyright.md}}
