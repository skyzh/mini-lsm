<!--
  mini-lsm-book © 2022-2026 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Transaction and Optimistic Concurrency Control

By the end of this chapter, you will be able to:

* Maintain a private transaction workspace with read-your-writes semantics.
* Merge local updates and tombstones with the transaction's stable engine snapshot.
* Publish every update in a transaction with one commit timestamp and one crash-atomic WAL record.

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
4. Every record collected by one commit receives the same commit timestamp and remains in one memtable and WAL, even if the batch exceeds the size target.
5. Recovery validates the complete WAL batch before applying any record, so a truncated transaction exposes no prefix.
6. The WAL batch is appended before its entries become visible in the memtable. The latest commit timestamp is published after the batch is accepted and before fallible freeze maintenance.

> **Predict before coding:** The snapshot contains `a=old,b=old`; the local workspace contains `a=new,b=del,c=new`. What should an unbounded scan return? If the process stops after writing only part of the transaction's WAL record, how many of those updates may recovery expose?

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

One timestamp makes a completed transaction appear at once to new readers, but it does not make the transaction durable as a unit. Encode the entire transaction as one framed WAL record:

```
|   HEADER   |                          BODY                                      |  FOOTER  |
|     u32    |   u16   | var | u64 |    u16    |  var  |           ...            |    u32   |
| batch_size | key_len | key | ts  | value_len | value | more key-value pairs ... | checksum |
```

`batch_size` is the byte length of the body, and `checksum` covers the complete body. Reject keys, values, or batches that do not fit their encoded length fields rather than truncating their lengths.

Implement `Wal::put_batch` and make `Wal::put` call it with a one-record batch. Recovery must validate the header, every field boundary, and the footer before applying any record. A truncated or corrupt batch returns an error and exposes none of its prefix.

Implement `MemTable::put_batch` and make `MemTable::put` use it. Append the complete WAL record before inserting entries into the skiplist; otherwise an I/O error can make an update visible in memory without a durable commit.

Submit the transaction to one memtable and one WAL before checking whether to freeze. A batch larger than the configured memtable target is allowed to exceed the target; it must not be split across WALs. Once the batch is accepted, publish its commit timestamp before fallible freeze maintenance so the timestamp cannot be reused after a maintenance error.

## Chapter Checkpoint

Transaction-local reads should now behave like one overlay, and a successful commit should publish one indivisible logical and durable batch.

Verify these cases explicitly:

1. Overwrite and delete keys from the snapshot, then compare local `get` with included, excluded, and unbounded scans.
2. Confirm that another transaction cannot observe the workspace before commit and that an older transaction cannot observe it afterward.
3. Inspect all internal records created by one commit and confirm that they have the same timestamp.
4. Confirm that `get`, `scan`, `put`, `delete`, and a second `commit` reject use after the transaction has committed.
5. Commit a batch larger than the memtable target and confirm that it stays in one memtable and WAL.
6. Truncate the WAL batch in its header, body, and checksum; every recovery attempt must fail without exposing a prefix.

## Test Your Understanding

* With all the things we have implemented up to this point, does the system satisfy snapshot isolation? If not, what else do we need to do to support snapshot isolation? (Note: snapshot isolation is different from serializable snapshot isolation we will talk about in the next chapter)
* What if the user wants to batch import data (i.e., 1TB?) If they use the transaction API to do that, will you give them some advice? Is there any opportunity to optimize for this case?
* What is optimistic concurrency control? What would the system be like if we implement pessimistic concurrency control instead in Mini-LSM?
* Why are both a shared commit timestamp and WAL framing required for transaction atomicity?
* Why must WAL append happen before memtable publication? What should the caller observe if either step fails?
* When can the engine safely check the memtable size and freeze it without splitting the transaction?
* Should an empty transaction allocate a commit timestamp? What are the tradeoffs? This checkpoint follows the existing batch-write behavior.

## Bonus Tasks

* **Spill to Disk.** If the private workspace of a transaction gets too large, you may flush some of the data to the disk.

{{#include copyright.md}}
