# Overview

<!-- toc -->

In this tutorial, you will learn how to build a simple LSM-Tree storage engine in the Rust programming language.

## What is LSM, and Why LSM?

Log-structured merge tree is a data structure to maintain key-value pairs. This data structure is widely used in
distributed database systems like [TiDB](https://www.pingcap.com) and [CockroachDB](https://www.cockroachlabs.com) as
their underlying storage engine. [RocksDB](http://rocksdb.org), based on [LevelDB](https://github.com/google/leveldb),
is an implementation of LSM-Tree storage engine. It provides a wide range of key-value access functionalities and is
used in a lot of production systems.

Generally speaking, LSM Tree is an append-friendly data structure. It is more intuitive to compare LSM to other
key-value data structure like RB-Tree and B-Tree. For RB-Tree and B-Tree, all data operations are in-place. That is to
say, when you update the value corresponding to the key, the value will be overwritten at its original memory or disk
space. But in an LSM Tree, all write operations, i.e., insertions, updates, deletions, are performed in somewhere else.
These operations will be batched into SST (sorted string table) files and be written to the disk. Once written to the
disk, the file will not be changed. These operations are applied lazily on disk with a special task called compaction.
The compaction job will merge multiple SST files and remove unused data.

This architectural design makes LSM tree easy to work with.

1. Data are immutable on persistent storage, which means that it is easier to offload the background tasks (compaction)
   to remote servers. It is also feasible to directly store and serve data from cloud-native storage systems like S3.
2. An LSM tree can balance between read, write and space amplification by changing the compaction algorithm. The data
   structure itself is super versatile and can be optimized for different workloads.

In this tutorial, we will learn how to build an LSM-Tree-based storage engine in the Rust programming language.

## Prerequisites of this Tutorial

* You should know the basics of the Rust programming language. Reading [the Rust book](https://doc.rust-lang.org/book/)
  is enough.
* You should know the basic concepts of key-value storage engines, i.e., why we need somehow complex design to achieve
  persistence. If you have no experience with database systems and storage systems before, you can implement Bitcask
  in [PingCAP Talent Plan](https://github.com/pingcap/talent-plan/tree/master/courses/rust/projects/project-2).
* Knowing the basics of an LSM tree is not a requirement but we recommend you to read something about it, e.g., the 
  overall idea of LevelDB. This would familiarize you with concepts like mutable and immutable mem-tables, SST,
  compaction, WAL, etc.

## Overview of LSM

An LSM storage engine generally contains 3 parts:

1. Write-ahead log to persist temporary data for recovery.
2. SSTs on the disk for maintaining a tree structure.
3. Mem-tables in memory for batching small writes.

The storage engine generally provides the following interfaces:

* `Put(key, value)`: store a key-value pair in the LSM tree.
* `Delete(key)`: remove a key and its corresponding value.
* `Get(key)`: get the value corresponding to a key.
* `Scan(range)`: get a range of key-value pairs.

To ensure persistence,

* `Sync()`: ensure all the operations before `sync` are persisted to the disk.

Some engines choose to combine `Put` and `Delete` into a single operation called `WriteBatch`, which accepts a batch
of key value pairs.

In this tutorial, we assume the LSM tree is using leveled compaction algorithm, which is commonly used in real-world
systems.

## Write Flow

![Write Flow](lsm-tutorial/00-lsm-write-flow.svg)

The write flow of LSM contains 4 steps:

1. Write the key-value pair to write-ahead log, so that it can be recovered after the storage engine crashes.
2. Write the key-value pair to memtable. After (1) and (2) completes, we can notify the user that the write operation
   is completed.
3. When a memtable is full, we will flush it to the disk as an SST file in the background.
4. We will compact some files in some level into lower levels to maintain a good shape for the LSM tree, so that read
   amplification is low.

## Read Flow

![Read Flow](lsm-tutorial/00-lsm-read-flow.svg)

When we want to read a key,

1. We will first probe all the memtables from latest to oldest.
2. If the key is not found, we will then search the entire LSM tree containing SSTs to find the data.

## Community

You may join skyzh's Discord server and study with the mini-lsm community.

[![Join skyzh's Discord Server](https://dcbadge.vercel.app/api/server/ZgXzxpua3H)](https://skyzh.dev/join/discord)

## About the Author

As of writing (at the end of 2022), Chi is a first-year master's student in Carnegie Mellon University. He has 5 years'
experience with the Rust programming language since 2018. He has been working on a variety of database systems including
[TiKV][db1], [AgateDB][db2], [TerarkDB][db3], [RisingLight][db4], and [RisingWave][db5]. In his first semester in CMU,
he worked as a teaching assistant for CMU's [15-445/645 Intro to Database Systems][15445-course] course, where he built
a new SQL processing layer for the [BusTub][bustub] educational database system, added more query optimization stuff into
the course, and made the course [more challenging than ever before][tweet]. Chi is interested in exploring how the Rust
programming language can fit in the database world. Check out his [previous tutorial][type-exercise] on building a
vectorized expression framework if you are also interested in that topic.

[db1]: https://github.com/tikv/tikv
[db2]: https://github.com/tikv/agatedb
[db3]: https://github.com/bytedance/terarkdb
[db4]: https://github.com/risinglightdb/risinglight
[db5]: https://github.com/risingwavelabs/risingwave
[15445-course]: https://15445.courses.cs.cmu.edu/fall2022/
[tweet]: https://twitter.com/andy_pavlo/status/1598137241016360961
[type-exercise]: https://github.com/skyzh/type-exercise-in-rust
[bustub]: https://github.com/cmu-db/bustub

<!--
## Structure

chapters + snacks, clear goal

implement, think, try by yourself

required tasks, check your understanding questions, bonus tasks

## Testing

exploring and understanding is more important than passing all the test cases

testing basic requirements, not the internal structure or something

## Solution

### Checkpoints

the final version, but many things can be simplified, read the docs

comments / tests / not up-to-date with the starter code

### How to use the solutions

## Feedbacks

join the Discord server, your feedback is important, thank GitHub users

## License

### Free forever?

### Video lectures + Review Service + Office Hour?

should have a separate preface (before you start) chapter? and what's new with v2?

## Target audience?

## What will you get after taking this course...
-->

{{#include copyright.md}}
