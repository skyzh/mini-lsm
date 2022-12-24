# Overview

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

## Overview of LSM

An LSM storage engine generally contains 3 parts:

1. Write-ahead log to persist temporary data for recovery.
2. SSTs on the disk for maintaining a tree structure.
3. Mem-tables in memory for batching small writes.

The storage engine generally provides the following interfaces:

* `Put(key, value)`: store a key-value pair in the LSM tree.
* `Delete(key)`: remove a key and its corresponding value.
* `Get(key)`: get the value corresponding to a key.

To ensure persistence,

* `Sync()`: ensure all the operations before `sync` are persisted to the disk.

Some engines choose to combine `Put` and `Delete` into a single operation called `WriteBatch`, which accepts a batch
of key value pairs.

In this tutorial, we assume the LSM tree is using leveled compaction algorithm, which is commonly used in real-world
systems.

## Write Flow

![Write Flow](figures/lsm-tutorial/00-lsm-write-flow.svg)

The write flow of LSM contains 4 steps:

1. Write the key-value pair to write-ahead log, so that it can be recovered after the storage engine crashes.
2. Write the key-value pair to memtable. After (1) and (2) completes, we can notify the user that the write operation
   is completed.
3. When a memtable is full, we will flush it to the disk as an SST file in the background.
4. We will compact some files in some level into lower levels to maintain a good shape for the LSM tree, so that read
   amplification is low.

## Read Flow

![Read Flow](figures/lsm-tutorial/00-lsm-read-flow.svg)

When we want to read a key,

1. We will first probe all the memtables from latest to oldest.
2. If the key is not found, we will then search the entire LSM tree containing SSTs to find the data.

## Tutorial Overview

![Tutorial Overview](figures/lsm-tutorial/00-lsm-tutorial-overview.svg)

In this tutorial, we will build the LSM tree structure in 7 days:

* Day 1: Block encoding. SSTs are composed of multiple data blocks. We will implement the block encoding.
* Day 2: SST encoding.
* Day 3: Engine. In this day we will get a functional (but not persistent) key-value engine with `get`, `put`, `delete`
  API.
* Day 4: Block cache. To reduce disk I/O and maximize performance, we will use moka-rs to build a block cache for the
  LSM tree.
* Day 5: Compaction. Now it's time to maintain a leveled structure for SSTs.
* Day 6: Recovery. We will implement WAL and manifest so that the engine can recover after restart.
* Day 7: Bloom filter and key compression. They are widely-used optimizations in LSM tree structures.

## Development Guide

We provide you starter code (see `mini-lsm-starter` crate), where we simply replace all function body with
`unimplemented!()`. You can start your project based on this starter code. We provide test cases, but they are very
simple. We recommend you to think carefully about your implementation and write test cases by yourself.

You can use `cargo x scheck` to run all test cases and do style check in your codebase.
