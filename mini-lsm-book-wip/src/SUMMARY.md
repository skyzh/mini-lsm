# LSM in a Week

[Overview](./00-overview.md)
[Get Started](./00-get-started.md)

# Week 1: Mini-LSM

- [Overview](./week1-overview.md)
  - [Blocks](./week1-01-block.md)
  - [Sorted String Table (SST)](./week1-02-sst.md)
  - [Memtables](./week1-03-memtable.md)
  - [Merge Iterators](./week1-04-merge-iterator.md)
  - [Read Path](./week1-05-read-path.md)
  - [Write Path](./week1-06-write-path.md)

# Week 2: Compaction and Persistence

- [Overview](./week2-overview.md)
  - [Simple Compaction](./week2-01-compaction.md)
  - [Tiered Compaction](./week2-02-tiered.md)
  - [Leveled Compaction](./week2-03-leveled.md)
  - [Manifest](./week2-04-manifest.md)
  - [Write-Ahead Log (WAL)](./week2-05-wal.md)
  - [SST Optimizations](./week2-06-sst-optimizations.md)

# Week 3: MVCC

- [Overview](./week3-overview.md)

# The Rest of Your Life (TBD)

---

# Mini-LSM v1

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
