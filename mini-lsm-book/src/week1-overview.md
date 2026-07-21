<!--
  mini-lsm-book © 2022-2025 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Week 1 Overview: Mini-LSM

![Chapter Overview](./lsm-tutorial/week1-overview.svg)

In the first week, you will build the storage engine's core formats, read path, and write path. By the end of its seven chapters, you will have a working LSM-based key-value store.

* [Day 1: Memtable](./week1-01-memtable.md). Implement the system's in-memory read and write paths.
* [Day 2: Merge Iterator](./week1-02-merge-iterator.md). Extend what you built on Day 1 and implement the system's `scan` interface.
* [Day 3: Block Encoding](./week1-03-block.md). Take the first step toward an on-disk representation by implementing block encoding and decoding.
* [Day 4: SST Encoding](./week1-04-sst.md). Compose blocks into SSTs to create the basic building blocks of the LSM tree's on-disk structure.
* [Day 5: Read Path](./week1-05-read-path.md). Combine the in-memory and on-disk structures into a complete read path.
* [Day 6: Write Path](./week1-06-write-path.md). Take over the SST creation that the Day 5 test harness performed for you. Flush memtables to level-0 SSTs to complete the storage engine.
* [Day 7: SST Optimizations](./week1-07-sst-optimizations.md). Implement several SST-format optimizations to improve system performance.

At the end of the week, your storage engine should be able to handle `get`, `scan`, and `put` requests. The remaining work is to persist the LSM state across restarts and organize SSTs on disk more efficiently. You will have a working **Mini-LSM** storage engine.

{{#include copyright.md}}
