# Merge Iterator

![Chapter Overview](./lsm-tutorial/week1-02-overview.svg)

In this chapter, you will:

* Implement memtable iterator.
* Implement merge iterator.
* Implement LSM read path `scan` for memtables.

## Task 1: Memtable Iterator

self-referential struct

## Task 2: Merge Iterator

error handling, order requirement

## Task 3: LSM Iterator

## Task 4: Read Path - Scan

## Test Your Understanding

* Why do we need a self-referential structure for memtable iterator?
* If we want to get rid of self-referential structure and have a lifetime on the memtable iterator (i.e., `MemtableIterator<'a>`, where `'a` = memtable or `LsmStorageInner` lifetime), is it still possible to implement the `scan` functionality?
* What happens if (1) we create an iterator on the skiplist memtable (2) someone inserts new keys into the memtable (3) will the iterator see the new key?
* Why do we need to ensure the merge iterator returns data in the iterator construction order?
* Is it possible to implement a Rust-style iterator (i.e., `next(&self) -> (Key, Value)`) for LSM iterators? What are the pros/cons?
* The scan interface is like `fn scan(&self, lower: Bound<&[u8]>, upper: Bound<&[u8]>)`. How to make this API compatible with Rust-style range (i.e., `key_a..key_b`)? If you implement this, try to pass a full range `..` to the interface and see what will happen.

## Bonus Task

* **Foreground Iterator.** In this tutorial we assumed that all operations are short, so that we can hold reference to mem-table in the iterator. If an iterator is held by users for a long time, the whole mem-table (which might be 256MB) will stay in the memory even if it has been flushed to disk. To solve this, we can provide a `ForegroundIterator` / `LongIterator` to our user. The iterator will periodically create new underlying storage iterator so as to allow garbage collection of the resources.

{{#include copyright.md}}
