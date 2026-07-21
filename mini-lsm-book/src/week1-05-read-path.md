<!--
  mini-lsm-book © 2022-2025 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Read Path

![Chapter Overview](./lsm-tutorial/week1-05-overview.svg)

In this chapter, you will:

* Integrate SST into the LSM read path.
* Implement LSM read path `get` with SSTs.
* Implement LSM read path `scan` with SSTs.

To copy the test cases into the starter code and run them:

```
cargo x copy-test --week 1 --day 5
cargo x scheck
```

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

## Test Your Understanding

* Suppose a user creates an iterator over the entire 1 TB storage engine, and the scan takes about an hour. What problems could this cause? We will revisit this question at several points in the course.
* Some LSM-tree storage engines provide a multi-get, or vectored-get, interface. The caller supplies a list of keys and receives a value for each one; for example, `multi_get(vec!["a", "b", "c", "d"]) -> a=1,b=2,c=3,d=4`. The simplest implementation performs one `get` per key. How would you implement multi-get, and what could you optimize? Hint: some work in the get path needs to be performed only once for the entire batch. You can also consider an improved disk-I/O interface designed for multi-get.

We do not provide reference answers to these questions. Feel free to discuss them in the Discord community.

## Bonus Tasks

* **The Cost of Dynamic Dispatch.** Implement a `Box<dyn StorageIterator>` version of merge iterators and benchmark to see the performance differences.
* **Parallel Seek.** Creating a merge iterator requires loading the first relevant block from every underlying SST when you create each `SsTableIterator`. Consider creating these iterators in parallel.

{{#include copyright.md}}
