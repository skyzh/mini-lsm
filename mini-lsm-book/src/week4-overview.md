<!--
  mini-lsm-book © 2022-2026 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# The Rest of Your Life

Congratulations! You have built a storage engine from the ground up.

Mini-LSM began with an ordered in-memory map. It now has its own on-disk formats, a unified read path, background compaction, crash recovery, multi-version concurrency control, and transactions. More importantly, you have practiced the kind of reasoning that makes storage systems trustworthy: defining an invariant, finding the event that could violate it, and designing evidence that would expose the mistake.

## Look Back at What You Built

In [Week 1](./week1-overview.md), you assembled the basic data path. Writes entered a memtable, frozen memtables became SSTs, and reads merged several physical sources into one logical view. Blocks, iterators, Bloom filters, prefix encoding, and the block cache were not isolated exercises: together they determined how bytes moved from an API call to memory and disk, and back again.

In [Week 2](./week2-overview.md), you turned that working engine into a persistent one. You compared leveled and tiered compaction, balanced read, write, and space amplification, and preserved newer data while background work reorganized files. The manifest and write-ahead log made recovery possible, while checksums and careful write ordering forced you to ask exactly what survives each crash point.

In [Week 3](./week3-overview.md), one value per key became a history of timestamped versions. You added stable snapshots, watermarks, atomic write batches, transaction-local writes, and commit-time validation. You also learned to state the boundary of a design honestly: Mini-LSM protects tracked point-key dependencies, but it does not track range gaps and therefore does not provide full serializability for scan-heavy workloads.

Across all three weeks, the recurring lesson is that a storage engine is a collection of agreements between layers. An ordering rule in the key type constrains blocks, SSTs, iterators, compaction, and recovery. A durability claim depends on both an encoded record and the order of filesystem operations. A transaction guarantee depends on when state becomes visible, not only on which lock is present. When you can trace those agreements end to end, you are doing systems engineering rather than merely completing functions.

## Choose Your Next Experiment

There is no single correct Week 4. Start with a workload or guarantee you care about, measure the current engine, and make one change whose effect you can explain.

### Measure and tune the engine

Build a benchmark harness for point reads, range scans, writes, and mixed workloads. Record latency distributions as well as throughput, then attribute time and bytes to memtables, cache lookups, table reads, compaction, and synchronization. Use those measurements to tune level sizes, compaction triggers, Bloom-filter parameters, and cache capacity. A good first project is to explain one performance result that initially surprised you.

### Explore new physical formats

Add block compression, compare alternative block encodings, or implement the skip list and block cache yourself. Each change should preserve compatibility and corruption handling, so version the format and test truncated, malformed, and mixed-version files. Key-value separation is a larger experiment: move large values into a value log and study how garbage collection changes write and space amplification.

### Make compaction and I/O more capable

Implement trivial moves, parallel subcompactions, rate limiting, prefetching, or a more workload-aware compaction picker. Then try an asynchronous storage path or an `io_uring`-based backend. These projects are most useful when you can show which stalls or amplification costs they reduce—and which new scheduling, cancellation, and recovery states they introduce.

### Expand the data model

Column families let several logical key spaces share one engine while keeping separate options and lifecycles. Sharding asks a different set of questions: how are ranges assigned, moved, and recovered, and what happens to a transaction that crosses a boundary? Managed timestamps and stronger range-conflict tracking can extend the transaction model toward workloads that Mini-LSM's current point-key validation does not cover.

### Build a database on top

Add a SQL layer, indexes, or a small application that needs ordered scans and transactions. This exposes the storage engine to real access patterns and makes abstract tradeoffs concrete. You will quickly discover which APIs are awkward, which metrics are missing, and which guarantees the layer above actually relies on.

## Keep the Habit

Whatever you build next, keep the workflow that carried you through Mini-LSM:

1. State the guarantee in observable terms.
2. Draw the smallest state, crash point, or interleaving that could break it.
3. Measure or test that case before optimizing the implementation.
4. Change one layer at a time, then recheck every layer that shares its invariant.
5. Document both what the system guarantees and what it deliberately does not.

You now have more than a toy key-value store. You have a compact laboratory for storage formats, concurrency, recovery, and performance—and the foundation to read a production engine with much sharper questions. Keep experimenting, keep measuring, and congratulations again on finishing Mini-LSM.

{{#include copyright.md}}
