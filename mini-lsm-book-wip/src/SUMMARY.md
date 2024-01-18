# LSM in a Week

[Overview](./00-overview.md)
[Get Started](./00-get-started.md)

- [Week 1: Mini-LSM](./week1-overview.md)
  - [Memtables](./week1-01-memtable.md)
  - [Blocks](./week1-02-block.md)
  - [Sorted String Table (SST)](./week1-03-sst.md)
  - [Merge Iterators](./week1-04-merge-iterator.md)
  - [Read Path](./week1-05-read-path.md)
  - [Write Path](./week1-06-write-path.md)
  - [Snack Time: SST Optimizations](./week1-07-sst-optimizations.md)

- [Week 2: Compaction and Persistence](./week2-overview.md)
  - [Compaction Implementation](./week2-01-compaction.md)
  - [Simple Compaction Strategy](./week2-02-simple.md)
  - [Tiered Compaction Strategy](./week2-03-tiered.md)
  - [Leveled Compaction Strategy](./week2-04-leveled.md)
  - [Manifest](./week2-05-manifest.md)
  - [Write-Ahead Log (WAL)](./week2-06-wal.md)
  - [Snack Time: Batch Write](./week2-07-batch-write.md)

- [Week 3: MVCC](./week3-overview.md)

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
