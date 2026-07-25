<!--
  mini-lsm-book © 2022-2026 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Mini-LSM Course Overview

## Course Structure

![Course Overview](lsm-tutorial/00-full-overview.svg)

This course has three parts, or weeks. In the first week, you will focus on the structure and storage format of an LSM storage engine. In the second week, you will explore compaction in depth and add persistence to the storage engine. In the third week, you will implement multiversion concurrency control (MVCC).

* [The First Week: Mini-LSM](./week1-overview.md)
* [The Second Week: Compaction and Persistence](./week2-overview.md)
* [The Third Week: Multi-Version Concurrency Control](./week3-overview.md)

Follow [Environment Setup](./00-get-started.md) to prepare your development environment.

## From Bitcask to Mini-LSM

If Bitcask is your starting point, you already know the central log-structured idea: do not overwrite an old record in place. Append a new record, let an index identify the newest value, and reclaim obsolete records later. Mini-LSM keeps that idea but changes the index and the shape of the files.

In a Bitcask-style engine, an in-memory hash index maps each key to its newest record on disk. That design is excellent for point lookups, but the hash index does not place keys in order. An ordered range scan therefore needs an additional ordered index or must examine and sort matching keys. The entire key directory must also fit in memory. See the original [Bitcask paper](https://riak.com/assets/bitcask-intro.pdf) for the design and its stated memory requirement.

Mini-LSM instead maintains **sorted** data at every stage:

1. A memtable is an ordered in-memory map. It absorbs small random writes without updating disk pages in place.
2. Freezing and flushing a memtable turns one sorted memory run into an immutable sorted-string table (SST). Because the input is already sorted, the engine can write the file sequentially.
3. Reads merge the newest memtables and SSTs into one logical sorted view. Source priority resolves several physical versions of the same key.
4. Compaction merges sorted runs in the background. It pays sequential rewrite work to bound how many runs a read must consult and to reclaim obsolete values and tombstones.
5. A write-ahead log protects the mutable part that has not reached an SST. The manifest separately records which immutable files form the current tree.

This is the design bargain behind the course: foreground writes become cheap appends and in-memory updates, while background compaction pays and schedules the work that an in-place index would perform on the write path. Sorted runs also make ordered scans and merge-based maintenance natural. The original [LSM-tree paper](https://www.cs.umb.edu/~poneil/lsmtree.pdf) motivates the structure as a way to maintain a disk index with lower insertion cost.

The three weeks follow the consequences of that bargain. Week 1 establishes one correct logical view over sorted memory and disk structures. Week 2 controls the delayed maintenance work and makes both the file layout and unflushed writes recoverable. Week 3 retains several timestamped versions so concurrent readers can keep stable snapshots while writes continue.

## Overview of LSM

An LSM storage engine generally has three components:

1. A write-ahead log that persists recent data for recovery.
2. SSTs on disk that form the LSM-tree structure.
3. Memtables in memory that batch small writes.

The storage engine generally provides the following interfaces:

* `Put(key, value)`: Stores a key-value pair in the LSM tree.
* `Delete(key)`: Removes a key and its corresponding value.
* `Get(key)`: Retrieves the value associated with a key.
* `Scan(range)`: Retrieves a range of key-value pairs.

It may also provide an operation that establishes a persistence boundary:

* `Sync()`: Ensures that all preceding operations have been persisted to disk.

Some engines combine `Put` and `Delete` into a single operation called `WriteBatch`, which accepts a batch of updates.

The overview diagrams assume a leveled compaction layout, which is common in production systems. In Week 2, you will implement and compare several compaction strategies.

### Write Path

![Write Path](lsm-tutorial/00-lsm-write-flow.svg)

The LSM write path has four steps:

1. Write the key-value pair to the write-ahead log so that it can be recovered after a crash.
2. Write the key-value pair to the mutable memtable. After steps 1 and 2 are complete, the engine can report that the write has completed.
3. In the background, freeze a full mutable memtable, making it immutable, and flush it to disk as an SST file.
4. Also in the background, compact files from one or more levels into lower levels. This maintains the shape of the LSM tree and limits read amplification.

### Read Path

![Read Path](lsm-tutorial/00-lsm-read-flow.svg)

To read a key, the engine:

1. Probes the memtables from newest to oldest.
2. If the memtables do not determine the result, searches the SSTs in the LSM tree.

There are two types of reads: lookups and scans. A lookup finds one key in the LSM tree, whereas a scan iterates over all keys within a range. The course covers both.

{{#include copyright.md}}
