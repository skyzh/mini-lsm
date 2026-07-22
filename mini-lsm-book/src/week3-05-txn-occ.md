<!--
  mini-lsm-book © 2022-2026 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Transaction and Optimistic Concurrency Control

By the end of this chapter, you will be able to:

* Maintain a private transaction workspace with read-your-writes semantics.
* Merge local updates and tombstones with the transaction's stable engine snapshot.
* Publish every update in a transaction with one commit timestamp.

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
4. Every record collected by one commit receives the same commit timestamp.
5. The latest commit timestamp advances only after the batch has been submitted, so a new transaction does not select a partially published snapshot.
6. This checkpoint does not make a transaction crash-atomic: the existing write path can append records individually and can freeze between them.

> **Predict before coding:** The snapshot contains `a=old,b=old`; the local workspace contains `a=new,b=del,c=new`. What should an unbounded scan return? Which transaction can observe those changes before commit, and which older transaction can observe them afterward?

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

## Chapter Checkpoint

Transaction-local reads should now behave like one overlay, and a successful commit should publish all local records with one timestamp.

Verify these cases explicitly:

1. Overwrite and delete keys from the snapshot, then compare local `get` with included, excluded, and unbounded scans.
2. Confirm that another transaction cannot observe the workspace before commit and that an older transaction cannot observe it afterward.
3. Inspect all internal records created by one commit and confirm that they have the same timestamp.
4. Confirm that `get`, `scan`, `put`, `delete`, and a second `commit` reject use after the transaction has committed.
5. Trace a multi-key commit through the existing write path and identify where a crash or memtable freeze could split it. That limitation is outside this checkpoint.

## Test Your Understanding

* With all the things we have implemented up to this point, does the system satisfy snapshot isolation? If not, what else do we need to do to support snapshot isolation? (Note: snapshot isolation is different from serializable snapshot isolation we will talk about in the next chapter)
* What if the user wants to batch import data (i.e., 1TB?) If they use the transaction API to do that, will you give them some advice? Is there any opportunity to optimize for this case?
* What is optimistic concurrency control? What would the system be like if we implement pessimistic concurrency control instead in Mini-LSM?
* The records share a timestamp, but are they crash-atomic? Which changes would be required in the WAL and memtable write path to make them so?
* When you commit the transaction, can you insert records into the memtable one by one without exposing a partial commit to a new transaction? Which state publication makes the completed timestamp visible?
* Should an empty transaction allocate a commit timestamp? What are the tradeoffs? This checkpoint follows the existing batch-write behavior.

## Bonus Tasks

* **Spill to Disk.** If the private workspace of a transaction gets too large, you may flush some of the data to the disk.
* **Crash-Atomic Transactions.** Frame the complete batch in the WAL with a length and checksum, recover it only after validating the full frame, keep it in one memtable, and append it durably before publishing its entries in memory.

{{#include copyright.md}}
