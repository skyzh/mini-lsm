<!--
  mini-lsm-book © 2022-2026 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Transaction and Optimistic Concurrency Control

By the end of this chapter, you will be able to:

* Maintain a private transaction workspace with read-your-writes semantics.
* Merge local updates and tombstones with the transaction's stable engine snapshot.
* Commit one transaction as one atomic memtable and WAL batch.

To copy and run the test cases:

```
cargo x copy-test --week 3 --day 5
cargo x scheck
```

This chapter prepares for optimistic concurrency control; serializable conflict validation is added on Day 6.

## Before You Begin

A transaction now has two sources: its private workspace and the engine snapshot at `read_ts`. The local source is newer and must win on duplicate user keys.

Keep these invariants in mind:

1. `get` and `scan` provide read-your-writes, including local deletions.
2. Local entries shadow the engine snapshot but remain invisible to every other transaction until commit.
3. Commit marks the transaction unusable before publishing any writes.
4. A non-empty transaction uses one commit timestamp and one WAL batch, even when it exceeds the memtable size target.
5. The complete WAL record is appended before any record from that batch becomes visible in the memtable. Recovery validates the whole batch before applying any of it.
6. A read-only transaction publishes no batch and does not advance the commit timestamp.

> **Predict before coding:** The snapshot contains `a=old,b=old`; the local workspace contains `a=new,b=del,c=new`. What should an unbounded scan return? If the process crashes after writing only half of the WAL batch, how many of those local updates may recovery expose?

## Task 1: Local Workspace + Put and Delete

In this task, you will need to modify:

```
src/mvcc/txn.rs
```

You can now implement `put` and `delete` by inserting the corresponding key/value to the `local_storage`, which is a skiplist memtable without key timestamp. Note that for deletes, you will still need to implement it as inserting an empty value, instead of removing a value from the skiplist.

## Task 2: Get and Scan

In this task, you will need to modify:

```
src/mvcc/txn.rs
```

For `get`, probe local storage first and interpret an empty value as a deletion. For `scan`, implement `TxnLocalIterator` over the timestamp-free skiplist and merge it ahead of `FusedIterator<LsmIterator>`. Because the merge iterator retains tombstones, `TxnIterator` must suppress them without exposing the older engine value they shadow.

## Task 3: Commit

In this task, you will need to modify:

```
src/mvcc/txn.rs
```

We assume that a transaction is used from one thread. As it enters commit, atomically mark it committed so later `put`, `delete`, `scan`, `get`, or repeated `commit` calls fail.

Your commit implementation should simply collect all key-value pairs from the local storage and submit a write batch to the storage engine.

## Task 4: Atomic WAL

In this task, you will need to modify:

```
src/wal.rs
src/mem_table.rs
src/lsm_storage.rs
```

Note that `commit` involves producing a write batch, and for now, the write batch does not guarantee atomicity. You will need to change the WAL implementation to produce a header and a footer for the write batch.

The new WAL encoding is as follows:

```
|   HEADER   |                          BODY                                      |  FOOTER  |
|     u32    |   u16   | var | u64 |    u16    |  var  |           ...            |    u32   |
| batch_size | key_len | key | ts  | value_len | value | more key-value pairs ... | checksum |
```

`batch_size` is the size of the `BODY` section. `checksum` is the checksum for the `BODY` section.

Add focused validation for the encoded batch boundary and checksum. Recovery must reject a truncated or corrupt batch without applying a prefix of its records.

Implement `Wal::put_batch` and `MemTable::put_batch`. The original `put` function should treat one key-value pair as a one-record batch and call `put_batch`.

Append the complete batch to the WAL before inserting its entries into the skiplist. Otherwise, an I/O error can leave values visible in memory even though the durable log rejected their commit.

A batch should be handled in the same mem table and the same WAL, even if it exceeds the mem table size limit.

## Chapter Checkpoint

Transaction-local reads should now behave like one overlay, and a successful commit should create one indivisible durable batch.

Verify these cases explicitly:

1. Overwrite and delete keys from the snapshot, then compare local `get` with included, excluded, and unbounded scans.
2. Confirm that another transaction cannot observe the workspace before commit and that an older transaction cannot observe it afterward.
3. Commit a batch larger than the memtable threshold and confirm that it is not split across memtables or WALs.
4. Truncate a WAL batch at the header, body, and checksum; each recovery attempt should return an error and expose none of that batch.
5. Commit a read-only transaction and confirm that the latest commit timestamp does not change.

## Test Your Understanding

* With all the things we have implemented up to this point, does the system satisfy snapshot isolation? If not, what else do we need to do to support snapshot isolation? (Note: snapshot isolation is different from serializable snapshot isolation we will talk about in the next chapter)
* What if the user wants to batch import data (i.e., 1TB?) If they use the transaction API to do that, will you give them some advice? Is there any opportunity to optimize for this case?
* What is optimistic concurrency control? What would the system be like if we implement pessimistic concurrency control instead in Mini-LSM?
* What happens if your system crashes and leaves a corrupted WAL on disk? How do you distinguish a complete batch from a prefix?
* When you commit the transaction, is it necessary to put everything into the memtable as a batch, or can you insert it key by key? Which interleavings would expose a partial commit?
* Why must WAL append happen before memtable publication? What should the caller observe if either step fails?
* Should an empty transaction allocate a commit timestamp? What downstream state changes would that cause?

## Bonus Tasks

* **Spill to Disk.** If the private workspace of a transaction gets too large, you may flush some of the data to the disk.

{{#include copyright.md}}
