<!--
  mini-lsm-book © 2022-2025 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Read Path

![Chapter Overview](./lsm-tutorial/week1-05-overview.svg)

By the end of this chapter, you will be able to:

* Integrate SST into the LSM read path.
* Implement LSM read path `get` with SSTs.
* Implement LSM read path `scan` with SSTs.
* Trace how recency, tombstones, range bounds, and state snapshots determine the logical result across memory and disk.

To copy the test cases into the starter code and run them:

```
cargo x copy-test --week 1 --day 5
cargo x scheck
```

## Before You Begin

Memtables and SSTs can each be queried independently. The storage engine must now combine them into one logical view in which implementation details—how many structures contain a key or where they reside—are invisible to the user.

Preserve these read-path invariants:

1. Mutable and immutable memtables take precedence over L0 SSTs, and newer sources take precedence over older sources within each group.
2. A tombstone in a newer source hides every older value for the same key.
3. `scan` returns sorted, unique, live keys within the exact requested bounds.
4. A seek may land on the next greater key, so `get` returns a value only after checking for exact key equality.
5. Creating and seeking SST iterators may perform I/O and therefore happens after releasing the `state` lock, using a consistent cloned snapshot.

> **Predict before coding:** The mutable memtable contains `b -> delete` and `d -> 4`; an immutable memtable contains `a -> 1` and `b -> 2`; the newest L0 SST contains `a -> 0`, `c -> 3`, and `d -> 3`. What should `get(a)`, `get(b)`, and a scan with both `a` and `d` included return? For each result, identify the source that wins.

## Task 1: Two Merge Iterator

In this task, you will need to modify:

```
src/iterators/two_merge_iterator.rs
```

You have already implemented a merge iterator for iterators of the same type, such as memtable iterators. Now that the SST format is implemented, the engine has both in-memory memtables and on-disk SSTs. A scan must merge memtable and SST iterators into a single stream. For this purpose, implement `TwoMergeIterator<X, Y>`, which can merge two different iterator types.

Because there are only two iterators, `TwoMergeIterator` does not need a binary heap. A flag can indicate which iterator currently has precedence. As in `MergeIterator`, when both iterators contain the same key, the first iterator takes precedence.

## Task 2: Read Path - Scan

In this task, you will need to modify:

```
src/lsm_iterator.rs
src/lsm_storage.rs
```

After implementing `TwoMergeIterator`, we can change the `LsmIteratorInner` to have the following type:

```rust,no_run
type LsmIteratorInner =
    TwoMergeIterator<MergeIterator<MemTableIterator>, MergeIterator<SsTableIterator>>;
```

This type combines data from memtables and SSTs into the storage engine's internal iterator.

The SST iterator does not support an end bound for scans. Enforce that bound in `LsmIterator` by updating its constructor to accept an `end_bound`:

```rust,no_run
pub(crate) fn new(iter: LsmIteratorInner, end_bound: Bound<Bytes>) -> Result<Self> {}
```

Then update the iteration logic to stop according to the bound's semantics: before an excluded end key, or after an included end key.

The tests create memtables and SSTs referenced by `l0_sstables`; your scan must return their combined contents correctly. You do not need to implement flushing until the next chapter. For now, update `LsmStorageInner::scan` to create a merge iterator over all memtables and L0 SSTs, completing the engine's scan path.

Creating and initially seeking an `SsTableIterator` may perform I/O, so do not do it while holding the `state` lock. First acquire the read lock and clone the `Arc` containing the state snapshot. Release the lock, then create an iterator for each L0 SST and merge the resulting streams.

```rust,no_run
fn scan(&self) {
    let snapshot = {
        let guard = self.state.read();
        Arc::clone(&guard)
    };
    // create iterators and seek them
}
```

The `l0_sstables` vector stores only SST IDs. Retrieve the corresponding `SsTable` objects from the `sstables` map.

## Task 3: Read Path - Get

In this task, you will need to modify:

```
src/lsm_storage.rs
```

Process a `get` as direct lookups in the memtables followed, if necessary, by a seek over a merge iterator of the SSTs. A seek may land on the requested key or on the next greater key, so return a value only if the iterator's key exactly matches the requested key. As in the scan path, minimize the `state` lock's critical section. Preserve newest-to-oldest precedence, and treat an empty value as a tombstone rather than continuing to older data.

## Chapter Checkpoint

The engine should now present one consistent read view across all mutable, immutable, and on-disk structures. Verify both point reads and scans for a key that appears in several sources, a key deleted by a newer source, and a key absent from every source.

Exercise all four combinations of included and excluded scan bounds, plus unbounded ranges. For each test, predict the first and last returned keys before running it. Finally, inspect the lifetime of the `state` read guard and confirm that no SST iterator is created while that guard is held.

## Test Your Understanding

### Correctness

* In the prediction example above, how would each result change if the two inputs to `TwoMergeIterator` were reversed?
* Construct the smallest state in which continuing to search after finding a tombstone resurrects a deleted key.
* A seek for `b` lands on `c`. Which explicit comparison prevents `get(b)` from returning `c`'s value?
* Where are included and excluded upper bounds enforced? Write a boundary test that would fail if the implementation used `<` for both variants.

### Resource Lifetime and Performance

* Suppose a user creates an iterator over the entire 1 TB storage engine, and the scan takes about an hour. What problems could this cause? We will revisit this question at several points in the course.
* Some LSM-tree storage engines provide a multi-get, or vectored-get, interface. The caller supplies a list of keys and receives a value for each one; for example, `multi_get(vec!["a", "b", "c", "d"]) -> a=1,b=2,c=3,d=4`. The simplest implementation performs one `get` per key. How would you implement multi-get, and what could you optimize? Hint: some work in the get path needs to be performed only once for the entire batch. You can also consider an improved disk-I/O interface designed for multi-get.

We do not provide reference answers to these questions. Feel free to discuss them in the Discord community.

## Bonus Tasks

* **The Cost of Dynamic Dispatch.** Implement a `Box<dyn StorageIterator>` version of merge iterators and benchmark to see the performance differences.
* **Parallel Seek.** Creating a merge iterator requires loading the first relevant block from every underlying SST when you create each `SsTableIterator`. Consider creating these iterators in parallel.

{{#include copyright.md}}
