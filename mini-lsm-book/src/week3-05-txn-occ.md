# Transaction and Optimistic Concurrency Control

In this chapter, you will implement all interfaces of `Transaction`. Your implementation will maintain a private workspace for modifications inside a transaction, and commit them in batch, so that all modifications within the transaction will only be visible to the transaction itself until commit.

To run test cases,

```
cargo x copy-test --week 3 --day 5
cargo x scheck
```

## Task 1: Local Workspace + Put and Delete

In this task, you will need to modify:

```
src/txn.rs
```

You can now implement `put` and `delete` by inserting the corresponding key/value to the `local_storage`, which is a skiplist memtable without key timestamp. Note that for deletes, you will still need to implement it as inserting an empty value, instead of removing a value from the skiplist.

## Task 2: Get and Scan

In this task, you will need to modify:

```
src/txn.rs
```

For `get`, you should first probe the local storage. If a value is found, return the value or `None` depending on whether it is a deletion marker. For `scan`, you will need to implement a `TxnLocalIterator` for the skiplist as in chapter 1.1 when you implement the iterator for a memtable without key timestamp. You will need to store a `TwoMergeIterator<TxnLocalIterator, FusedIterator<LsmIterator>>` in the `TxnLocalIterator`. And, lastly, given that the `TwoMergeIterator` will retain the deletion markers in the child iterators, you will need to modify your `TxnIterator` implementation to correctly handle deletions.

## Task 3: Commit

In this task, you will need to modify:

```
src/txn.rs
```

We assume that a transaction will only be used on a single thread. Once your transaction enters the commit phase, you should set `self.committed` to true, so that users cannot do any other operations on the transaction. You `put`, `delete`, `scan`, and `get` implementation should error if the transaction is already committed.

Your commit implementation should simply collect all key-value pairs from the local storage and submit a write batch to the storage engine.

## Test Your Understanding

* With all the things we have implemented up to this point, does the system satisfy snapshot isolation? If not, what else do we need to do to support snapshot isolation? (Note: snapshot isolation is different from serializable snapshot isolation we will talk about in the next chapter)

{{#include copyright.md}}
