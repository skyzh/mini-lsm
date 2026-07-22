<!--
  mini-lsm-book © 2022-2026 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Snapshot Read - Engine Read Path and Transaction API

By the end of this chapter, you will be able to:

* Select the newest version whose timestamp is less than or equal to a transaction's read timestamp.
* Keep a snapshot alive for the lifetime of a point read or scan iterator.
* Recover the latest commit timestamp from durable SST and WAL state.

At the end of the day, your engine will be able to give the user a consistent view of the storage key space.

During the refactor, you might need to change the signature of some functions from `&self` to `self: &Arc<Self>` as necessary.

To copy and run the test cases:

```
cargo x copy-test --week 3 --day 3
cargo x scheck
```

**Note:** You should also pass the persistence tests from Week 2 Days 5 and 6 after finishing this chapter.

## Before You Begin

Day 2 stores a complete version history but always reads at the latest timestamp. A transaction now captures `read_ts = latest_commit_ts` when it begins and reuses that value for every read.

Keep these invariants in mind:

1. For each user key, skip versions with `ts > read_ts`, then choose the first remaining version.
2. If the chosen version is a tombstone, the user key is absent; older values must not be returned.
3. After deciding one user key, skip every remaining version of that key before returning the next result.
4. A `TxnIterator` holds the transaction alive so its read timestamp remains protected for the entire scan.
5. Recovery initializes the timestamp oracle to at least the largest timestamp in every live SST and recovered WAL-backed memtable.

> **Predict before coding:** For `a@7=del, a@5=v5, a@2=v2`, what should reads at timestamps 8, 6, 5, and 1 return? Which iterator transitions are needed to avoid resurrecting `a@5` at timestamp 8?

## Task 1: LSM Iterator with Read Timestamp

The goal of this chapter is to have something like:

```rust,no_run
let snapshot1 = engine.new_txn();
// write something to the engine
let snapshot2 = engine.new_txn();
// write something to the engine
snapshot1.get(/* ... */); // we can retrieve a consistent snapshot of a previous state of the engine
```

Record the latest committed timestamp when creating the transaction. A read over that transaction may inspect newer internal versions, but it must return only versions at or below the recorded timestamp.

In this task, you will need to modify:

```
src/lsm_iterator.rs
```

To do this, you will need to record a read timestamp in `LsmIterator`.

```rust,no_run
impl LsmIterator {
    pub(crate) fn new(
        iter: LsmIteratorInner,
        end_bound: Bound<Bytes>,
        read_ts: u64,
    ) -> Result<Self> {
        // ...
    }
}
```

Update `LsmIterator` initialization and `next` logic to skip future versions, suppress tombstones, and return at most one result per user key.

## Task 2: Multi-Version Scan and Get

In this task, you will need to modify:

```
src/mvcc.rs
src/mvcc/txn.rs
src/lsm_storage.rs
```

Now implement `scan` and `get` on `Transaction` so every operation uses the same `read_ts`.

We recommend you to create helper functions like `scan_with_ts(/* original parameters */, read_ts: u64)` and `get_with_ts` if necessary in your `LsmStorageInner` structure. The original get/scan on the storage engine should be implemented as creating a transaction (snapshot) and do a get/scan over that transaction. The call path would be like:

```
LsmStorageInner::scan -> new_txn and Transaction::scan -> LsmStorageInner::scan_with_ts
```

To create a transaction in `LsmStorageInner::scan`, we will need to provide a `Arc<LsmStorageInner>` to the transaction constructor. Therefore, we can change the signature of `scan` to take `self: &Arc<Self>` instead of simply `&self`, so that we can create a transaction with `let txn = self.mvcc().new_txn(self.clone(), /* ... */)`.

Change `scan` to return a `TxnIterator`. The snapshot must remain live while the user consumes the scan, so `TxnIterator` stores the transaction object. Inside it, store a `FusedIterator<LsmIterator>` for now; Day 5 will add the transaction-local stream.

You do not need to implement `Transaction::put/delete` for now, and all modifications will still go through the engine.

## Task 3: Store Largest Timestamp in SST

In this task, you will need to modify:

```
src/table.rs
src/table/builder.rs
```

Store the largest timestamp after the block metadata and recover it when opening the SST. This summary lets startup advance the timestamp oracle without scanning every entry.

## Task 4: Recover Commit Timestamp

Now that we have largest timestamp information in the SSTs and timestamp information in the WAL, we can obtain the largest timestamp committed before the engine starts, and use that timestamp as the latest committed timestamp when creating the `mvcc` object.

If WAL is not enabled, you can simply compute the latest committed timestamp by finding the largest timestamp among SSTs. If WAL is enabled, you should further iterate all recovered memtables and find the largest timestamp.

In this task, you will need to modify:

```
src/lsm_storage.rs
```

Test this path explicitly by closing and reopening a database whose largest timestamp appears first in an SST and then in an unflushed WAL-backed memtable. The first post-recovery write must receive a strictly larger timestamp.

## Chapter Checkpoint

Transactions should now retain a stable logical snapshot across later writes, flushes, compactions, and restart.

Verify these cases explicitly:

1. Keep three transactions at different timestamps and compare both `get` and `scan` after overwrites and deletes.
2. Flush and compact while the transactions remain alive, then repeat the same reads.
3. Start a scan, commit a newer value, and continue the scan; the iterator must stay on its original read timestamp.
4. Reopen with the maximum timestamp present in an SST and then in a WAL, and confirm that neither timestamp is reused.

## Test Your Understanding

* So far, we have assumed that our SST files use a monotonically increasing id as the file name. Is it okay to use `<level>_<begin_key>_<end_key>_<max_ts>.sst` as the SST file name? What might be the potential problems with that?
* Consider an alternative implementation of transaction/snapshot. In our implementation, we have `read_ts` in our iterators and transaction context, so that the user can always access a consistent view of one version of the database based on the timestamp. Is it viable to store the current LSM state directly in the transaction context in order to gain a consistent snapshot? (i.e., all SST ids, their level information, and all memtables + ts) What are the pros/cons with that? What if the engine does not have memtables? What if the engine is running on a distributed storage system like S3 object store?
* Consider that you are implementing a backup utility of the MVCC Mini-LSM engine. Is it enough to simply copy all SST files out without backing up the LSM state? Why or why not?
* Why does a tombstone selected at `read_ts` stop the search instead of allowing an older value to become visible?
* Which object owns the lifetime of a scan's read timestamp, and what could compaction reclaim if that object were dropped too early?
* Is the maximum timestamp among current SST entries always a durable history of every timestamp ever allocated? What additional metadata would be needed if timestamps must never be reused after all records at the maximum timestamp are garbage-collected?

We do not provide reference answers to these questions, so feel free to discuss them in the Discord community.

{{#include copyright.md}}
