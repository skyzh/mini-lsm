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

## Test Your Understanding

* What are the pros/cons of universal compaction compared with simple leveled/tiered compaction?
* How much storage space is it required (compared with user data size) to run universal compaction without using up the storage device space?
* The log-on-log problem.

We do not provide reference answers to the questions, and feel free to discuss about them in the Discord community.

{{#include copyright.md}}
