<!--
  mini-lsm-book © 2022-2026 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Watermark and Garbage Collection

By the end of this chapter, you will be able to:

* Track the oldest read timestamp still held by a live transaction or iterator.
* Reclaim obsolete versions without changing any snapshot at or above the watermark.
* Explain when a bottom-level tombstone and every older value can be removed together.

To copy and run the test cases:

```
cargo x copy-test --week 3 --day 4
cargo x scheck
```

## Before You Begin

Retaining every version is correct but unbounded. The watermark is the smallest read timestamp still in use. Compaction may remove history that no transaction at or above that timestamp can distinguish.

Keep these invariants in mind:

1. The tracker counts readers, not only distinct timestamps; dropping one of two readers at the same timestamp must not advance the watermark.
2. Creating a transaction registers its timestamp before it can race with compaction.
3. A scan iterator keeps its transaction alive, so abandoning the original transaction handle does not release the watermark early.
4. Compaction keeps every version above the watermark and the newest version at or below it.
5. A tombstone at or below the watermark can be removed only when the compaction reaches the bottom level, where no older value can survive outside the task.

> **Predict before coding:** Readers exist at timestamps 3, 3, and 7. Which drops advance the watermark? For `k@8=v8, k@5=del, k@2=v2` with watermark 5, which versions survive a non-bottom compaction and a bottom-level compaction?

## Task 1: Implement Watermark

In this task, you will need to modify:

```
src/mvcc/watermark.rs
```

`Watermark` tracks the lowest `read_ts` in the system. A new transaction calls `add_reader`; the final owner of that transaction calls `remove_reader` when dropped. `watermark()` returns the lowest active timestamp, or `None` when no snapshot is live.

A `BTreeMap` can map each `read_ts` to its reader count. Remove entries when their count reaches zero.

## Task 2: Maintain Watermark in Transactions

In this task, you will need to modify:

```
src/mvcc/txn.rs
src/mvcc.rs
```

Register `read_ts` when a transaction starts and remove it in `Drop`. Because iterators own an `Arc<Transaction>`, the reader remains registered until every handle and iterator is gone.

## Task 3: Garbage Collection in Compaction

In this task, you will need to modify:

```
src/compact.rs
```

Now that we have a watermark for the system, we can clean up unused versions during the compaction process.

* If a version of a key is above watermark, keep it.
* For all versions of a key below or equal to the watermark, keep the latest version.

For example, if we have watermark=3 and the following data:

```
a@4=del <- above watermark
a@3=3   <- latest version below or equal to watermark
a@2=2   <- can be removed, no one will read it
a@1=1   <- can be removed, no one will read it
b@1=1   <- latest version below or equal to watermark
c@4=4   <- above watermark
d@3=del <- can be removed if compacting to bottom-most level
d@2=2   <- can be removed
```

If we do a compaction over these keys, we will get:

```
a@4=del
a@3=3
b@1=1
c@4=4
d@3=del (can be removed if compacting to bottom-most level)
```

Assume these are all keys in the engine. A scan at timestamp 3 returns `a=3,b=1`; `c@4` is still in the future. A scan at timestamp 4 returns `b=1,c=4` because `a@4` is a tombstone. Both results must be identical before and after compaction. Compaction must not affect transactions whose read timestamp is at or above the watermark.

## Chapter Checkpoint

Compaction should now reduce history as the oldest snapshot advances, while every still-live snapshot returns the same values.

Verify these cases explicitly:

1. Hold two transactions at the same oldest timestamp and confirm that dropping only one does not advance the watermark.
2. Compact the example above at each successive watermark and compare both raw internal versions and user-visible reads.
3. Keep only a scan iterator alive, compact, and confirm that the iterator's snapshot is still protected.
4. Compare a tombstone compacted into a middle level with the same tombstone compacted into the bottom level.

## Test Your Understanding

* In our implementation, we manage watermarks by ourselves with the lifecycle of `Transaction` (so-called un-managed mode). If the user intends to manage key timestamps and the watermarks by themselves (i.e., when they have their own timestamp generator), what do you need to do in the write_batch/get/scan API to validate their requests? Is there any architectural assumption we had that might be hard to maintain in this case?
* Why do we need to store an `Arc` of `Transaction` inside a transaction iterator?
* What is the condition to fully remove a key from the SST file?
* For now, we only remove a key when compacting to the bottom-most level. Is there any other prior time that we can remove the key? (Hint: you know the start/end key of each SST in all levels.)
* Consider the case that the user creates a long-running transaction and we could not garbage collect anything. The user keeps updating a single key. Eventually, there could be a key with thousands of versions in a single SST file. How would it affect performance, and how would you deal with it?
* Why must compaction keep one version at or below the watermark instead of deleting every version below it?
* What race appears if a transaction reads the latest timestamp before registering itself with the watermark?

## Bonus Tasks

* **O(1) Watermark.** You may implement an amortized O(1) watermark structure by using a hash map or a cyclic queue.

{{#include copyright.md}}
