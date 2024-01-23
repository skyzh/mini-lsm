# Tiered Compaction Strategy

![Chapter Overview](./lsm-tutorial/week2-01-overview.svg)

In this chapter, you will:

* Implement a tiered compaction strategy and simulate it on the compaction simulator.
* Incorporate tiered compaction strategy into the system.

The tiered compaction we talk about in this chapter is the same as RocksDB's universal compaction. We will use these two terminologies interchangeably.

## Task 1: Universal Compaction

### Task 1.1: Triggered by Space Amplification Ratio

### Task 1.2: Triggered by Size Ratio

### Task 1.3: Reduce Sorted Runs

## Task 2: Integrate with the Read Path

As tiered compaction does not use the L0 level of the LSM state, you should directly flush your memtables to a new tier instead of as an L0 SST. You can use `self.compaction_controller.flush_to_l0()` to know whether to flush to L0. You may use the first output SST id as the level/tier id for your new sorted run.

## Test Your Understanding

* What are the pros/cons of universal compaction compared with simple leveled/tiered compaction?
* How much storage space is it required (compared with user data size) to run universal compaction without using up the storage device space?
* The log-on-log problem.

We do not provide reference answers to the questions, and feel free to discuss about them in the Discord community.

{{#include copyright.md}}
