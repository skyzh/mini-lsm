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

## Structure

* mini-lsm: the final solution code
* mini-lsm-starter: the starter code
* mini-lsm-book: the tutorial

We have another repo mini-lsm-solution-checkpoint at [https://github.com/skyzh/mini-lsm-solution-checkpoint](https://github.com/skyzh/mini-lsm-solution-checkpoint). In this repo, each commit corresponds to a chapter in the tutorial. We will not update the solution checkpoint very often.

## Demo

You can run the reference solution by yourself to gain an overview of the system before you start.

```
cargo run --bin mini-lsm-cli-ref
```

And we have a compaction simulator to experiment with your compaction algorithm implementation,

```
cargo run --bin compaction-simulator-ref
```


## Progress

We are working on chapter 3 and more test cases for all existing contents.

* Week 1: Storage Format + Engine Skeleton
* Week 2: Compaction and Persistence
* Week 3: Multi-Version Concurrency Control
* The Extra Week / Rest of Your Life: Optimizations  (unlikely to be available in 2024...)

✅: finished \
🚧: WIP and will likely be available soon

| Week + Chapter | Topic                                           | Solution | Starter Code | Writeup |
| -------------- | ----------------------------------------------- | -------- | ------------ | ------- |
| 2.7            | Batch Write + Checksum                          | 🚧        |              |         |
| 3.1            | Timestamp Key Encoding + New Block Format       |          |              |         |
| 3.2            | Prefix Bloom Filter                             |          |              |         |
| 3.3            | Snapshot Read                                   |          |              |         |
| 3.4            | Watermark and Garbage Collection                |          |              |         |
| 3.5            | Transactions and Optimistic Concurrency Control |          |              |         |
| 3.6            | Serializable Snapshot Isolation                 |          |              |         |
| 3.7            | TTL (Time-to-Live) Entries                      |          |              |         |
| 4.1            | Benchmarking                                    |          |              |         |
| 4.2            | Block Compression                               |          |              |         |
| 4.3            | Trivial Move and Parallel Compaction            |          |              |         |
| 4.4            | Alternative Block Encodings                     |          |              |         |
| 4.5            | Rate Limiter and I/O Optimizations              |          |              |         |
| 4.6            | Build Your Own Block Cache                      |          |              |         |
| 4.7            | Build Your Own SkipList                         |          |              |         |
| 4.8            | Async Engine                                    |          |              |         |
| 4.9            | Key-Value Separation                            |          |              |         |
| 4.10           | Column Families                                 |          |              |         |
| 4.11           | Sharding                                        |          |              |         |
| 4.12           | SQL over Mini-LSM                               |          |              |         |

## License

The Mini-LSM starter code and solution are under Apache 2.0 license. The author reserves the full copyright of the tutorial materials (markdown files and figures).
