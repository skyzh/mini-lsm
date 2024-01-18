# LSM in a Week

[![CI (main)](https://github.com/skyzh/mini-lsm/actions/workflows/main.yml/badge.svg)](https://github.com/skyzh/mini-lsm/actions/workflows/main.yml)

Build a simple key-value storage engine in a week!

## Tutorial

The tutorial is available at [https://skyzh.github.io/mini-lsm](https://skyzh.github.io/mini-lsm). You can use the provided starter
code to kick off your project, and follow the tutorial to implement the LSM tree.

## Community

You may join skyzh's Discord server and study with the mini-lsm community.

[![Join skyzh's Discord Server](https://dcbadge.vercel.app/api/server/ZgXzxpua3H)](https://skyzh.dev/join/discord)

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

| Week + Chapter | Topic                                           | Solution | Starter Code | Writeup |
| -------------- | ----------------------------------------------- | -------- | ------------ | ------- |
| 1.1            | Memtables                                       | ✅        | ✅            | ✅       |
| 1.2            | Block Format                                    | ✅        | ✅            | ✅       |
| 1.3            | Table Format                                    | ✅        | ✅            | ✅       |
| 1.4            | Merge Iterators                                 | ✅        | ✅            | ✅       |
| 1.5            | Storage Engine - Read Path                      | ✅        | ✅            | ✅       |
| 1.6            | Storage Engine - Write Path                     | ✅        | ✅            | ✅       |
| 1.7            | Bloom Filter and Key Compression                |          |              |         |
| 2.1            | Compaction Implementation                       | ✅        | 🚧            | 🚧       |
| 2.2            | Compaction Strategy - Simple                    | ✅        | 🚧            | 🚧       |
| 2.3            | Compaction Strategy - Tiered                    | ✅        |              |         |
| 2.4            | Compaction Strategy - Leveled                   | ✅        |              |         |
| 2.5            | Manifest                                        |          |              |         |
| 2.6            | Write-Ahead Log                                 |          |              |         |
| 2.7            | Batch Write (and preparations for MVCC)         |          |              |         |
| 3.1            | Timestamp Encoding + Prefix Bloom Filter        |          |              |         |
| 3.2            | Snapshot Read                                   |          |              |         |
| 3.3            | Watermark and Garbage Collection                |          |              |         |
| 3.4            | Transactions and Optimistic Concurrency Control |          |              |         |
| 3.5            | Serializable Snapshot Isolation                 |          |              |         |
| 4.1            | Benchmarking                                    |          |              |         |
| 4.2            | Block Compression                               |          |              |         |
| 4.3            | Trivial Move and Parallel Compaction            |          |              |         |
| 4.4            | Alternative Block Encodings                     |          |              |         |
| 4.5            | Rate Limiter and I/O Optimizations              |          |              |         |
| 4.6            | Build Your Own Block Cache                      |          |              |         |
| 4.7            | Async Engine                                    |          |              |         |
| 4.8            | Key-Value Separation                            |          |              |         |
| 4.9            | Column Families                                 |          |              |         |
| 4.10           | SQL over Mini-LSM                               |          |              |         |
