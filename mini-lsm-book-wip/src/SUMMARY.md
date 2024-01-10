# LSM in a Week

[Overview](./00-overview.md)
[Get Started](./00-get-started.md)

---

# Week 1: Storage Format

- [Blocks](./01-block.md)
- [Sorted String Table (SST)](./02-sst.md)
- [Merge Iterators](./03-memtable.md)
- [Storage Engine](./04-engine.md)

# Week 2: Compaction and Persistence

- [Compaction Task](./05-compaction.md)
- [Compaction Strategy](./06-compaction-strategy.md)
- [Write-Ahead Log (WAL) and Manifest](./07-recovery.md)
- [Bloom Filter](./08-bloom-filter.md)
- [Key Compression](./09-key-compression.md)

# Week 3: MVCC

- [Encode the Timestamp](./10-ts.md)
- [Prefix Bloom Filter](./11-prefix-bloom-filter.md)
- [Read with Timestamp](./12-mvcc-read.md)
- [Snapshots and Watermark](./13-watermark.md)
- [Garbage Collection](./14-garbage-collection.md)

# The Rest of Your Life

- [I/O Optimization](./15-io-optimization.md)
- [Block Compression](./16-compression.md)
- [Async Engine](./17-async.md)
- [Serializable Snapshot Isolation](./18-serializable.md)
- [SQL over Mini LSM](./19-sql.md)
