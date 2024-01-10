# LSM in a Week

[![CI (main)](https://github.com/skyzh/mini-lsm/actions/workflows/main.yml/badge.svg)](https://github.com/skyzh/mini-lsm/actions/workflows/main.yml)

Build a simple key-value storage engine in a week!

## Tutorial

The tutorial is available at [https://skyzh.github.io/mini-lsm](https://skyzh.github.io/mini-lsm). You can use the provided starter
code to kick off your project, and follow the tutorial to implement the LSM tree.

## Development

```
cargo x install-tools
cargo x check
cargo x book
```

If you changed public API in the reference solution, you might also need to synchronize it to the starter crate.
To do this, use `cargo x sync`.

## Progress

We are working on a new version of the mini-lsm tutorial that is split into 3 weeks.

* Week 1: Storage Format + Engine Skeleton
* Week 2: Compaction and Persistence
* Week 3: Multi-Version Concurrency Control
* The Extra Week / Rest of Your Life: Optimizations  (unlikely to be available in 2024...)

| Week + Chapter | Topic                                    | Solution | Starter Code | Writeup |
| -------------- | ---------------------------------------- | -------- | ------------ | ------- |
| 1.1            | Block Format                             | ✅        | ✅            | ✅       |
| 1.2            | Table Format                             | ✅        | ✅            | ✅       |
| 1.3            | Memtables                                | ✅        | ✅            | ✅       |
| 1.4            | Merge Iterators                          | ✅        | ✅            | ✅       |
| 1.5            | Storage Engine - Read Path               | ✅        | ✅            | ✅       |
| 1.6            | Storage Engine - Write Path              | ✅        | ✅            | ✅       |
| 2.1            | Compaction Framework                     | ✅        | 🚧            | 🚧       |
| 2.2            | Compaction Strategy                      | 🚧        |              |         |
| 2.3            | Manifest                                 |          |              |         |
| 2.4            | Write-Ahead Log                          |          |              |         |
| 2.5            | Bloom Filter and Key Compression         |          |              |         |
| 2.6            | Benchmarking                             |          |              |         |
| 3.1            | Timestamp Encoding + Prefix Bloom Filter |          |              |         |
| 3.2            | Snapshot Read                            |          |              |         |
| 3.3            | Watermark and Garbage Collection         |          |              |         |
| 3.4            | Transactions                             |          |              |         |
| 3.5            | Serializable Snapshot Isolation          |          |              |         |
| 3.6            | What's Next...                           |          |              |         |
| 4.1            | Block Compression                        |          |              |         |
| 4.2            | Rate Limiter and I/O Optimizations       |          |              |         |
| 4.3            | Build Your Own Block Cache               |          |              |         |
| 4.4            | Async Engine                             |          |              |         |
| 4.5            | Key-Value Separation                     |          |              |         |
| 4.6            | Column Families                          |          |              |         |
| 4.7            | SQL over Mini-LSM                        |          |              |         |
