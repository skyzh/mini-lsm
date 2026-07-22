<!--
  mini-lsm-book © 2022-2026 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Tiered Compaction Strategy

![Chapter Overview](./lsm-tutorial/week2-00-tiered.svg)

By the end of this chapter, you will be able to:

* Implement a tiered compaction strategy and simulate it on the compaction simulator.
* Incorporate tiered compaction into the engine's flush, compaction, and read paths.
* Explain how universal compaction's three triggers trade write, read, and space amplification.
* Preserve newest-to-oldest tier order and determine whether a task includes the bottom tier.

This chapter's tiered policy is RocksDB's universal compaction. The chapter uses the two terms interchangeably.

To copy the test cases into the starter code and run them:

```
cargo x copy-test --week 2 --day 3
cargo x scheck
```

<div class="warning">

Read the [Week 2 overview](./week2-overview.md) before beginning this chapter for an introduction to compaction and amplification.

</div>

## Before You Begin

Unlike leveled compaction, tiered compaction treats each flush as a new sorted run. The `levels` vector is repurposed to store tiers from newest to oldest; the numeric ID names a tier rather than describing a fixed depth.

Keep these invariants in mind:

1. `levels[0]` is the newest tier, and every tier is internally sorted and non-overlapping.
2. A tiered task selects a contiguous prefix of tiers. The merge iterator must use the same newest-to-oldest order so that newer versions win.
3. The controller considers triggers in order: minimum tier count, space amplification, size ratio, then reduction of sorted runs.
4. `bottom_tier_included` is true only if the selected prefix contains the oldest tier. Tombstones must remain when an older tier is left out.
5. A flush creates a one-SST tier at the front of `levels`; it does not add an L0 file.

> **Predict before coding:** Suppose the tiers from newest to oldest have sizes `[1, 1, 3, 20]`, and the reduce-sorted-runs trigger is capped at `max_merge_width = 2`. Which tiers are selected? Does the task include the bottom tier, and may it discard a tombstone whose older value could be in the 20-file tier?

## Task 1: Universal Compaction

You will implement a simplified form of RocksDB's universal compaction. As in simple leveled compaction, the controller uses file counts rather than byte sizes. A task always includes each selected tier in full.

### Task 1.0: Precondition

In this task, you will need to modify:

```
src/compact/tiered.rs
```

In universal compaction, the LSM state does not use L0. Instead, each memtable flush creates a single-SST sorted run, or tier. The `levels` vector stores all tiers, with **the newest tier at the lowest index**. Each element is a tuple containing a tier ID and the SST IDs in that tier. Place every newly flushed SST in a tier at the front of the vector. The compaction simulator uses the first output SST ID as the tier ID; your implementation should do the same.

Universal compaction will only trigger tasks when the number of tiers (sorted runs) reaches `num_tiers`. Otherwise, it does not trigger any compaction.

### Task 1.1: Triggered by Space Amplification Ratio

The first trigger of universal compaction is by space amplification ratio. As we discussed in the overview chapter, space amplification can be estimated by `engine_size / last_level_size`. In our implementation, we compute the space amplification ratio by `all levels except last level size / last level size`, so that the ratio can be scaled to `[0, +inf)` instead of `[1, +inf]`. This is also consistent with the RocksDB implementation.

This estimate models a fixed logical dataset—for example, 100 GB—that receives repeated updates. The bottom tier approximates the logical dataset, while upper tiers contain newer changes that have not yet reached the bottom. The implementation expresses excess space as `upper_tier_size / bottom_tier_size`, producing a range from zero upward.

Trigger full compaction when `upper_tier_size / bottom_tier_size` is at least `max_size_amplification_percent / 100`. For example:

```
Tier 3: 1
Tier 2: 1 ; all levels except last level size = 2
Tier 1: 1 ; last level size = 1, 2/1=2
```

Assume `max_size_amplification_percent` = 200, we should trigger a full compaction now.

After you implement this trigger, you can run the compaction simulator. You will see:

```shell
cargo run --bin compaction-simulator tiered --iterations 10
```

```
=== Iteration 2 ===
--- After Flush ---
L3 (1): [3]
L2 (1): [2]
L1 (1): [1]
--- Compaction Task ---
compaction triggered by space amplification ratio: 200
L3 [3] L2 [2] L1 [1] -> [4, 5, 6]
--- After Compaction ---
L4 (3): [3, 2, 1]
```

With only this trigger implemented, the end of the simulation includes states like:

```bash
cargo run --bin compaction-simulator tiered
```

```
=== Iteration 7 ===
--- After Flush ---
L8 (1): [8]
L7 (1): [7]
L6 (1): [6]
L5 (1): [5]
L4 (1): [4]
L3 (1): [3]
L2 (1): [2]
L1 (1): [1]
--- Compaction Task ---
--- Compaction Task ---
compaction triggered by space amplification ratio: 700
L8 [8] L7 [7] L6 [6] L5 [5] L4 [4] L3 [3] L2 [2] L1 [1] -> [9, 10, 11, 12, 13, 14, 15, 16]
--- After Compaction ---
L9 (8): [8, 7, 6, 5, 4, 3, 2, 1]
--- Compaction Task ---
1 compaction triggered in this iteration
--- Statistics ---
Write Amplification: 16/8=2.000x
Maximum Space Usage: 16/8=2.000x
Read Amplification: 1x

=== Iteration 49 ===
--- After Flush ---
L82 (1): [82]
L81 (1): [81]
L80 (1): [80]
L79 (1): [79]
L78 (1): [78]
L77 (1): [77]
L76 (1): [76]
L75 (1): [75]
L74 (1): [74]
L73 (1): [73]
L72 (1): [72]
L71 (1): [71]
L70 (1): [70]
L69 (1): [69]
L68 (1): [68]
L67 (1): [67]
L66 (1): [66]
L65 (1): [65]
L64 (1): [64]
L63 (1): [63]
L62 (1): [62]
L61 (1): [61]
L60 (1): [60]
L59 (1): [59]
L58 (1): [58]
L57 (1): [57]
L33 (24): [32, 31, 30, 29, 28, 27, 26, 25, 24, 23, 22, 21, 20, 19, 18, 17, 9, 10, 11, 12, 13, 14, 15, 16]
--- Compaction Task ---
--- Compaction Task ---
no compaction triggered
--- Statistics ---
Write Amplification: 82/50=1.640x
Maximum Space Usage: 50/50=1.000x
Read Amplification: 27x
```

The simulator sets `num_tiers` to 8, but the state can still grow far beyond eight tiers because the threshold only enables scheduling; the space trigger does not guarantee that it will choose a task. This causes high read amplification.

The current trigger controls space amplification only. The next two triggers limit read amplification.

### Task 1.2: Triggered by Size Ratio

The size-ratio trigger maintains geometric growth between tiers. Starting with the newest tier, compare each next tier's size with the total size of all newer tiers. At the first ratio greater than `(100 + size_ratio) / 100`, compact the newer prefix but exclude the tier that satisfied the ratio. Schedule the task only when the prefix contains at least `min_merge_width` tiers.

For example, with `size_ratio = 1` and `min_merge_width = 2`, compact when the ratio exceeds 101%:

```
Tier 3: 1
Tier 2: 1 ; 1 / 1 = 1
Tier 1: 1 ; 1 / (1 + 1) = 0.5, no compaction triggered
```

Example 2:

```
Tier 3: 1
Tier 2: 1 ; 1 / 1 = 1
Tier 1: 3 ; 3 / (1 + 1) = 1.5, compact tier 2+3
```

```
Tier 4: 2
Tier 1: 3
```

Example 3:

```
Tier 3: 1
Tier 2: 2 ; 2 / 1 = 2, however, it does not make sense to compact only one tier; also note that min_merge_width=2
Tier 1: 4 ; 4 / 3 = 1.33, compact tier 2+3
```

```
Tier 4: 3
Tier 1: 4
```

With this trigger, you will observe the following in the compaction simulator:

```bash
cargo run --bin compaction-simulator tiered
```

```
=== Iteration 49 ===
--- After Flush ---
L119 (1): [119]
L118 (1): [118]
L114 (4): [113, 112, 111, 110]
L105 (5): [104, 103, 102, 101, 100]
L94 (6): [93, 92, 91, 90, 89, 88]
L81 (7): [80, 79, 78, 77, 76, 75, 74]
L48 (26): [47, 46, 45, 44, 43, 37, 38, 39, 40, 41, 42, 24, 25, 26, 27, 28, 29, 30, 9, 10, 11, 12, 13, 14, 15, 16]
--- Compaction Task ---
--- Compaction Task ---
no compaction triggered
--- Statistics ---
Write Amplification: 119/50=2.380x
Maximum Space Usage: 52/50=1.040x
Read Amplification: 7x
```

```bash
cargo run --bin compaction-simulator tiered --iterations 200 --size-only
```

```
=== Iteration 199 ===
--- After Flush ---
Levels: 0 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 2 3 4 5 6 10 15 21 28 78
no compaction triggered
--- Statistics ---
Write Amplification: 537/200=2.685x
Maximum Space Usage: 200/200=1.000x
Read Amplification: 38x
```

This trigger produces fewer one-SST tiers and tends to keep tiers ordered from smaller to larger. The state can still exceed `num_tiers`, so one final trigger is required.

### Task 1.3: Reduce Sorted Runs

If none of the previous triggers produce a task, merge the first `max_merge_width` tiers into one tier to reduce the number of sorted runs. If `max_merge_width` is not set, select all tiers. Remember that a capped prefix does not include the bottom tier when older tiers remain.

With this compaction trigger enabled, you will see:

```bash
cargo run --bin compaction-simulator-ref tiered --iterations 200 --size-only
```

```
=== Iteration 199 ===
--- After Flush ---
Levels: 0 1 1 4 5 21 28 140
no compaction triggered
--- Statistics ---
Write Amplification: 742/200=3.710x
Maximum Space Usage: 280/200=1.400x
Read Amplification: 7x
```

You can also try tiered compaction with a larger tier limit:

```bash
cargo run --bin compaction-simulator tiered --iterations 200 --size-only --num-tiers 16
```

```
=== Iteration 199 ===
--- After Flush ---
Levels: 0 1 1 1 1 1 1 1 1 1 1 15 175
no compaction triggered
--- Statistics ---
Write Amplification: 607/200=3.035x
Maximum Space Usage: 350/200=1.750x
Read Amplification: 12x
```

**Note: we do not provide fine-grained unit tests for this part. You can run the compaction simulator and compare with the output of the reference solution to see if your implementation is correct.**

## Task 2: Integrate with the Read Path

In this task, you will need to modify:

```
src/compact.rs
src/lsm_storage.rs
```

Tiered compaction does not use L0, so flush each memtable directly to a new tier. Use `self.compaction_controller.flush_to_l0()` to choose between the leveled and tiered flush paths. Name a compacted tier with its first output SST ID, and construct the compaction merge iterators in newest-to-oldest tier order.

## Chapter Checkpoint

Your engine should now flush directly into newest-first tiers, schedule the first eligible universal-compaction trigger, and retain tombstones whenever a task leaves older tiers behind. A task that includes the bottom tier may legitimately produce no SSTs when all surviving entries are tombstones.

For several simulator iterations, annotate each task with the trigger that selected it, the tiers it consumes, and whether it reaches the bottom. Then use one duplicate key to verify merge priority across tiers. Finally, test a capped `max_merge_width` and confirm that the task does not claim to include an unselected bottom tier.

## Related Readings

[Universal Compaction - RocksDB Wiki](https://github.com/facebook/rocksdb/wiki/Universal-Compaction)

## Test Your Understanding

### Correctness and Scheduling

* Why must every task select adjacent tiers from the newest end of the state? Construct a counterexample for a task that merges two non-adjacent tiers and places the output incorrectly.
* When `max_merge_width` limits a task to only the newest tiers, why must `bottom_tier_included` be false?
* What should the new LSM state contain if a bottom-tier compaction produces no output because every surviving entry is a tombstone?
* Construct a tier-size sequence for which the space-amplification trigger wins, and another for which the size-ratio trigger wins.
* If a new tier is flushed while a task is running, where should the compacted output be inserted relative to that tier?

### Amplification and Design

* What is the estimated write amplification of tiered compaction? This is difficult in general; begin by ignoring the final *reduce sorted runs* trigger.
* What is the estimated read amplification of tiered compaction?
* What are the advantages and disadvantages of universal compaction compared with leveled compaction?
* How much free storage space does universal compaction require relative to the logical data size?
* What happens if compaction speed cannot keep up with the SST flushes for tiered compaction?
* What must the system consider before scheduling multiple compaction tasks in parallel?
* SSDs also write its own logs (basically it is a log-structured storage). If the SSD has a write amplification of 2x, what is the end-to-end write amplification of the whole system? Related: [ZNS: Avoiding the Block Interface Tax for Flash-based SSDs](https://www.usenix.org/conference/atc21/presentation/bjorling).
* Consider the case that the user chooses to keep a large number of sorted runs (i.e., 300) for tiered compaction. To make the read path faster, is it a good idea to keep some data structure that helps reduce the time complexity (i.e., to `O(log n)`) of finding SSTs to read in each layer for some key ranges? Note that normally, you will need to do a binary search in each sorted run to find the key ranges that you will need to read. (Check out Neon's [layer map](https://neon.tech/blog/persistent-structures-in-neons-wal-indexing) implementation!)

We do not provide reference answers to these questions, so feel free to discuss them in the Discord community.

{{#include copyright.md}}
