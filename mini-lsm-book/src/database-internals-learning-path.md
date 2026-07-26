<!--
  mini-lsm-book © 2022-2026 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# How to Learn Database Internals

Database internals become easier to learn when each concept has a place in one small working system. Instead of studying file formats, recovery, compaction, and transactions as disconnected topics, build a storage engine and follow each write from the user-facing API to memory, disk, recovery, and concurrent reads.

Mini-LSM covers the storage-engine half of that journey. It deliberately stops before SQL query processing and distributed coordination so that you can understand one layer end to end.

## A Practical Learning Order

### 1. Represent Ordered Data in Memory and on Disk

Begin with the physical structures that hold sorted key-value pairs. A memtable accepts writes in memory. Blocks group sorted entries, and sorted-string tables (SSTs) combine blocks with indexes and metadata on disk.

In [Week 1](./week1-overview.md), you will implement these structures and reason about their byte layouts, size boundaries, and ordering invariants.

### 2. Connect the Read and Write Paths

A storage engine rarely has one authoritative container. A recent value may live in the mutable memtable, an immutable memtable waiting to flush, or one of several SSTs. Reads must merge those sources in the correct priority order, while writes must freeze and flush memory without losing concurrent updates.

Week 1 ends with a working engine that supports point lookups, range scans, writes, deletes, and flushes. This is the first useful milestone: you can run the engine before adding the rest of the production-inspired machinery.

### 3. Control Background Maintenance

Immutable files make foreground writes simple, but they leave older values and overlapping key ranges behind. Compaction rewrites those files into a shape that bounds read cost and reclaims obsolete data.

[Week 2](./week2-overview.md) compares simple, tiered, and leveled compaction. The important lesson is not one universally best policy. It is how a workload turns read, write, and space amplification into an explicit engineering tradeoff.

### 4. Make State Recoverable

Correct results before a restart are not enough. The engine needs a write-ahead log for updates that have not reached an SST and a manifest for changes to the set of durable files. The ordering of file creation, synchronization, metadata installation, and deletion determines which crash states recovery can safely accept.

Week 2 adds the manifest, WAL, checksums, and restart recovery after the compaction foundation is in place.

### 5. Add Snapshots and Transactions

Concurrent readers need a stable view even while new writes commit. Multi-version concurrency control attaches timestamps to internal keys, selects the newest version visible to a snapshot, and delays garbage collection until no active reader needs an older version.

[Week 3](./week3-overview.md) builds from version ordering to snapshot reads, transactional writes, optimistic validation, and serializable validation for tracked keys. It also makes the limits explicit: key-based validation does not prevent every range phantom.

### 6. Continue into Query Processing and Distributed Systems

A complete database has layers that Mini-LSM intentionally does not implement. After finishing the storage engine, useful next subjects include:

* SQL parsing, planning, and query execution;
* indexes and cost-based optimization;
* replication and consensus;
* distributed transactions; and
* workload-aware observability and tuning.

At that point, the storage engine is no longer a black box. You can evaluate how those higher layers depend on durability, ordering, isolation, and the physical cost of reads and writes.

## Choose How You Build

Use the [three-week guided course](./00-overview.md) if you want to implement each subsystem directly. If you work with a coding agent, use the [coding-agent track](./agent-fast-forward-overview.md), which is in progress with Days 1–2 available. That track lets the agent write much of the code while requiring you to own consequential design decisions, predict adversarial cases, and explain what the implementation would break if an invariant changed.

Whichever path you choose, do not treat a passing test suite as the finish line. You should be able to trace the data flow, name the ordering and durability invariants, describe at least one plausible failure mode, and design a test that exposes it.

Continue with the [Mini-LSM course overview](./00-overview.md) or [set up the repository](./00-get-started.md).

{{#include copyright.md}}
