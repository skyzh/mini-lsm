# Week 3 Overview: Multi-Version Concurrency Control

In this part, you will implement MVCC over the LSM engine that you have built in the previous two weeks. We will add timestamp encoding in the keys to maintain multiple versions of a key, and change some part of the engine to ensure old data are either retained or garbage-collected based on whether there are users reading an old version.

1. Use the new key module
2. Refactor until no compile error
3. Use correct key ranges, add timestamp for engine
4. Memtable refactor
5. LsmIterator use read_ts
6. Compaction no delete

{{#include copyright.md}}
