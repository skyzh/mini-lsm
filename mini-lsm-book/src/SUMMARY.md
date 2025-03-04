<!--
  mini-lsm-book © 2022-2025 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# LSM in a Week

[Preface](./00-preface.md)
[Mini-LSM Overview](./00-overview.md)
[Environment Setup](./00-get-started.md)

- [Week 1 Overview: Mini-LSM](./week1-overview.md)
  - [Memtable](./week1-01-memtable.md)
  - [Merge Iterator](./week1-02-merge-iterator.md)
  - [Block](./week1-03-block.md)
  - [Sorted String Table (SST)](./week1-04-sst.md)
  - [Read Path](./week1-05-read-path.md)
  - [Write Path](./week1-06-write-path.md)
  - [Snack Time: SST Optimizations](./week1-07-sst-optimizations.md)

- [Week 2 Overview: Compaction + Persistence](./week2-overview.md)
  - [Compaction Implementation](./week2-01-compaction.md)
  - [Simple Compaction Strategy](./week2-02-simple.md)
  - [Tiered Compaction Strategy](./week2-03-tiered.md)
  - [Leveled Compaction Strategy](./week2-04-leveled.md)
  - [Manifest](./week2-05-manifest.md)
  - [Write-Ahead Log (WAL)](./week2-06-wal.md)
  - [Snack Time: Batch Write and Checksums](./week2-07-snacks.md)

- [Week 3 Overview: MVCC](./week3-overview.md)
  - [Timestamp Encoding + Refactor](./week3-01-ts-key-refactor.md)
  - [Snapshots - Memtables and Timestamps](./week3-02-snapshot-read-part-1.md)
  - [Snapshots - Transaction API](./week3-03-snapshot-read-part-2.md)
  - [Watermark and GC](./week3-04-watermark.md)
  - [Transaction and OCC](./week3-05-txn-occ.md)
  - [Serializable Snapshot Isolation](./week3-06-serializable.md)
  - [Snack Time: Compaction Filters](./week3-07-compaction-filter.md)
- [The Rest of Your Life (TBD)](./week4-overview.md)

---

# DEPRECATED Mini-LSM v1

- [Overview](./00-v1.md)
  - [Store key-value pairs in little blocks](./01-block.md)
  - [And make them into an SST](./02-sst.md)
  - [Now it's time to merge everything](./03-memtable.md)
  - [The engine is on fire](./04-engine.md)
  - [Let's do something in the background](./05-compaction.md)
  - [Be careful when the system crashes](./06-recovery.md)
  - [A good bloom filter makes life easier](./07-bloom-filter.md)
  - [Save some space, hopefully](./08-key-compression.md)
  - [What's next](./09-whats-next.md)
