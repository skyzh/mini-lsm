<!--
  mini-lsm-book © 2022-2026 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Day 2 - Compaction and Recovery

This is the second day of [Mini-LSM with Coding Agents](./agent-fast-forward-overview.md). You will turn the Day 1 engine into a storage engine that reorganizes files in the background and reconstructs its state after a restart:

```text
memtables + WALs -> SSTs -> compaction -> sorted runs
          \________ manifest records file ownership ________/
```

The difficult part is not producing more SSTs. It is preserving the newest value, retaining concurrent work, and making every durable record tell the truth after a crash.

## What You Will Finish

At the end of this path:

- full, simple leveled, tiered, and dynamic leveled compaction work;
- `get` and `scan` preserve newest-value priority across every layout;
- compaction can run without losing an L0 flush or tier created concurrently;
- a manifest reconstructs the live SST layout;
- WAL-backed memtables restore synchronized writes;
- batch writes preserve the existing `put` and `delete` behavior;
- checksums protect SST blocks and metadata, Bloom filters, WAL records, and manifest records; and
- you can predict a compaction task, trace a crash boundary, and identify the bytes protected by each checksum.

Tests and simulator output are evidence, not the specification. A plausible file layout can still return stale data, resurrect a deletion, or reference a file that was never made durable.

## Prepare Day 2

Begin from a Day 1 implementation that passes its complete suite. From the repository root, copy all available Week 2 tests and record the first failure:

```shell
cargo x copy-test --week 2
cargo x scheck
```

Week 2 has supplied tests for its original Days 1 through 6. The checksum work from original Day 7 has no dedicated supplied tests, so you will need your own corruption cases.

With the agent running from `mini-lsm-starter`, send:

> Build Day 2 with me, starting with one safe full compaction. Follow the student-owned design protocol in `AGENTS.md` and never access `../mini-lsm`. Ask one short question at a time using a concrete set of files, keys, or crash points. Mark each question **Course rule** or **Your choice**. I may reply `simpler`, `example`, `hint`, or `choose for me`. Do not edit until my answers specify one small, coherent slice. After each slice, show me one important line and ask what it does and what would break if it changed.

Use the checkpoints in order unless you can explain why a different dependency order is safe. The tables are audit guides, not questionnaires to paste into the agent.

## Checkpoint 1: One Safe Compaction

Ask:

> Implement one full compaction and read from its output.

This checkpoint covers full compaction, the concat iterator, and the two-level read path. Use [Compaction Implementation](./week2-01-compaction.md) when a question needs more context.

The agent should help you work out these course rules from small file states:

| Behavior to work out | Small case that exposes it |
| --- | --- |
| Duplicate-key priority | Put a newer value in L0 and an older value in L1. |
| Tombstone removal | Compare a full bottom-level compaction with a task that leaves an older level untouched. |
| Concurrent result installation | Flush file 6 after a task captures L0 files 5 and 4. |
| Empty output | Compact inputs whose newest entries are all tombstones. |
| Output splitting | Cross the target SST size with one more entry. |
| Concat-iterator precondition | Swap two non-overlapping SSTs or make their ranges overlap. |
| Reader lifetime | Unlink an input file while an older state snapshot still owns its open handle. |

The central installation case is:

```text
task captured:  L0 = [5, 4], L1 = [1, 2]
while building: L0 file 6 is flushed
install result: L0 = [6],    L1 = [new sorted outputs]
```

Merge and write the outputs outside `state_lock`. When the outputs are ready, acquire `state_lock`, apply the task to the latest state, remove exactly the captured input IDs, and retain file 6. Do not replace the current L0 with the task's old snapshot.

Inputs must be presented to the merge iterator from newest to oldest. A tombstone can be discarded only when the task reaches the bottom and therefore includes every possible older version of the key. If every surviving entry is discarded, return no output SST instead of building an empty one.

L1 is one sorted run: its SST ranges do not overlap and are ordered by first key. Its concat iterator should open only the active SST rather than eagerly loading one block from every file.

Before approving the checkpoint, use one key that appears in both L0 and L1. Predict the result when L0 contains a value and when it contains a tombstone. After the focused checks pass, explain the line that removes captured L0 IDs without removing a later flush.

## Checkpoint 2: Decide What to Compact

Ask:

> Implement the compaction schedulers, one policy at a time.

Use [Simple Leveled Compaction](./week2-02-simple.md), [Tiered Compaction](./week2-03-tiered.md), [Leveled Compaction](./week2-04-leveled.md), and the [Week 2 overview](./week2-overview.md) as references.

Follow this default sequence for each policy:

1. implement task selection and result application against the simulator state;
2. run a short simulator trace and explain every selected task;
3. add the policy to compaction dispatch, flushing, and reads; and
4. stop for review before beginning the next policy.

Do not treat “which policies should exist?” as a student preference: the completed Week 2 track implements all three. The policy algorithms are course rules; helper structure, allocation, and equivalent search methods may be genuine choices.

### Simple leveled

```text
L0: overlapping files, newest to oldest
L1..Ln: one non-overlapping sorted run per level
```

L0 reaches its file-count trigger before it compacts into L1. Below L0, let the upper and lower levels contain `U` and `L` files. Trigger when `L / U * 100` is less than `size_ratio_percent`; a task consumes both complete levels. Handle an empty upper level without dividing by zero. The controller must eventually return no task so the simulator and background worker converge. The upper level is newer and wins equal keys. Preserve tombstones unless the lower level is the bottom.

### Tiered

```text
levels[0] = newest tier
levels[last] = oldest tier
```

Tiered compaction does not use L0: every flush creates a one-SST tier at the front. Do not schedule until the number of tiers reaches `num_tiers`. Then consider triggers in this fixed order:

1. compact every tier when `sum(all tiers except bottom) / bottom_size` reaches `max_size_amplification_percent / 100`;
2. scanning from newest to oldest, compact the newer prefix before the first tier whose size divided by that prefix's total is greater than `(100 + size_ratio) / 100`, provided the prefix has at least `min_merge_width` tiers; and
3. if neither ratio selects work, merge the first `max_merge_width` tiers—or all tiers when it is unset—to reduce the number of sorted runs.

A task selects a contiguous prefix of newest tiers. If `max_merge_width` leaves an older tier behind, `bottom_tier_included` is false and tombstones must remain. Insert the output after any newer tier flushed while the task was running.

### Dynamic leveled

Compute target sizes from the bottom level upward. The first level with a positive target is the base level; an eligible L0 task goes directly there and takes priority over size-based tasks. For lower levels, compute `current_size / target_size`, choose the highest score greater than 1, select the oldest upper-level SST, and include every lower SST whose inclusive key range overlaps it.

Applying a result removes exactly the selected files and restores first-key ordering among the untouched and output SSTs. During manifest replay the SST objects are not open yet, so defer this sorting until their key ranges are available.

After each controller works in the simulator, integrate it with the engine. The background worker asks for a task every 50 ms, returns successfully when there is no work, compacts outside `state_lock`, and applies the result to the latest state. Extend `get` and `scan` across every lower-level sorted run using one concat iterator per run. Tiered mode instead reads tiers newest to oldest and flushes new SSTs directly into new tiers.

Use these simulator commands without consulting the reference implementation:

```shell
cargo run --bin compaction-simulator simple
cargo run --bin compaction-simulator tiered
cargo run --bin compaction-simulator leveled
```

For at least one trace per policy, annotate the selected inputs, the reason the task fired, whether it reaches the bottom, the source that wins equal keys, and the files that remain afterward. Change one threshold and predict the next task before rerunning the simulator.

## Checkpoint 3: Recover the Live State

Ask:

> Add the manifest and WAL so synchronized writes survive restart.

This checkpoint covers [Manifest](./week2-05-manifest.md) and [Write-Ahead Log](./week2-06-wal.md). Implement the final framed, checksummed manifest and WAL records from [Batch Write and Checksums](./week2-07-snacks.md) directly; do not first build an unframed format and replace it in the same checkpoint.

First derive the required recovery behavior:

| Behavior to work out | Small case that exposes it |
| --- | --- |
| Manifest replay | Replay a flush followed by a compaction from an empty state. |
| New-file durability order | Crash after the SST is synced but before its manifest record. |
| Unsafe reference order | Crash after the manifest is synced but before the SST directory entry. |
| Obsolete-file deletion | Crash after the compaction record but before deleting its inputs. |
| Live WAL selection | Leave `NewMemtable(7)` without a matching `Flush(7)`. |
| WAL retirement | Leave an old WAL file after its flush record is durable. |
| Recovered ordering | Recover several memtables whose IDs were created over time. |
| ID allocation | Recover SST and WAL IDs with gaps and find the next unused ID. |
| Durability boundary | Stop before and after `sync` returns. |

Use final record framing so recovery can locate and verify one record at a time:

```text
manifest: len | JSON record | checksum | len | JSON record | checksum | ...
WAL:      key_len | key | value_len | value | checksum | ...
```

The checksum covers the encoded record bytes, not itself. Integer byte order is part of the format. Validate lengths before slicing and verify a checksum before exposing its payload.

For a flush or compaction that creates SSTs, the safe durable order is:

```text
write and sync new SSTs
  -> sync the directory entries
  -> append and sync the manifest record
  -> delete obsolete inputs
  -> sync the directory again
```

Recovery may see the old logical state or the new logical state at some crash boundaries. It must never see a durable manifest record that requires a missing file. An unreferenced new file or an undeleted obsolete file is tolerable because neither belongs to the recovered logical state.

When WALs are enabled, create the WAL, sync its directory entry, and durably record `NewMemtable(id)` before making that memtable available for writes. `sync` must flush the `BufWriter` and then call `sync_all`. A durable flush record retires the WAL logically before its filename is removed. Recovery ignores such stale files, restores live immutable memtables newest to oldest, and sets `next_sst_id` to one greater than every live SST or WAL ID.

Without WALs, `close` flushes every non-empty memtable. With WALs, it synchronizes them instead. In both cases, it stops and joins the background threads and remains harmless when called again.

Before approving the checkpoint, write the event sequence for one flush and place a crash after every event. Then recover a manifest containing interleaved `NewMemtable`, `Flush`, and `Compaction` records by hand. After the focused tests pass, explain the line that prevents a manifest record from becoming durable before its new file.

## Checkpoint 4: Reject Corrupted Data

Ask:

> Add the final write-batch API and checksums to the remaining disk formats.

This checkpoint finishes [Batch Write and Checksums](./week2-07-snacks.md). The supplied suite has no dedicated checksum tests, so an agent report that only says `cargo x scheck` passed is incomplete.

`put` and `delete` should delegate to `write_batch`. At this stage a batch preserves the existing per-record behavior; it does not promise transaction-like atomic visibility or durability for the entire group.

For SST data, keep the earlier block-size contract: the configured target describes block content, and the four-byte checksum is appended after it. Verify the block bytes before decoding them.

For each remaining section, identify its first protected byte, last protected byte, stored checksum, and framing fields:

```text
data block | checksum
block metadata | checksum | metadata offset
Bloom filter | checksum | Bloom offset
```

`Bloom::encode` appends to a buffer that already contains other SST sections. Record the filter's starting offset and checksum only the bytes added for the filter. The encoder and decoder must agree on the exact range; a round-trip alone cannot detect two matching implementations that protect the wrong bytes.

For every persistent format—data block, block metadata, Bloom filter, WAL, and manifest—add or run a test that:

1. writes a valid value and reads it back;
2. flips one byte inside the protected payload;
3. predicts which decoder should reject it; and
4. checks a truncated length or checksum without silently accepting data.

After implementation, have the agent point to one checksum slice boundary and ask what would happen if it included preceding bytes or excluded the final payload byte.

## Audit the Finished Engine

Run the complete project check from the repository root:

```shell
cargo x scheck
```

Also rerun all three compaction simulators and the corruption cases. Then ask for a final evidence report containing:

1. the combined decision ledgers and delegated choices;
2. the exact commands and outcomes;
3. one duplicate key traced through each compaction layout;
4. one task installation interleaved with a concurrent flush;
5. one manifest/WAL recovery trace with crash points;
6. the next-ID calculation for that recovered state;
7. the exact protected byte range for every checksum; and
8. one weakness not established by the supplied tests.

Inspect the actual diff and outputs. Search for changed supplied tests, removed assertions, broad lint suppressions, unchecked corrupted lengths, and file deletion before the durable manifest transition.

Finally, introduce and immediately revert one deliberate fault you understand. Good examples are reversing upper/lower merge priority, marking a capped tiered task as bottom-reaching, deleting an SST before its compaction record is durable, or excluding one payload byte from a checksum. Predict the failing test or recovery trace before running it.

## Day 2 Completion Checkpoint

You are ready for Day 3 when you can do these without delegating the answer back to the agent:

- predict the next task for simple, tiered, and leveled compaction;
- explain why the newest version wins in every task layout;
- decide whether a particular compaction may discard tombstones;
- show how result installation retains a concurrent flush;
- compare the read, write, and space costs of leveled and tiered policies;
- replay the manifest and live WALs into one LSM state;
- place crashes in a flush sequence and distinguish safe leftovers from missing required files;
- state what `sync` and `close` guarantee with and without WALs; and
- mark the exact byte range protected by every checksum and design a corruption test.

If one item is unclear, return to its original Week 2 chapter and ask the agent for a smaller file state, crash timeline, or byte layout. The goal is not merely a persistent engine. It is an engine whose background transitions and recovery story you can defend.

{{#include copyright.md}}
