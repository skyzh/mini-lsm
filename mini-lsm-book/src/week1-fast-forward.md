<!--
  mini-lsm-book © 2022-2025 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Day 1: Week 1 — Mini-LSM

This is the first day of [Agent Fast Forward in 3 Days](./agent-fast-forward-overview.md). You will use a coding agent to build and defend one working storage engine from the original Week 1 material:

```text
put/delete -> mutable memtable -> immutable memtables -> L0 SSTs
                     \____________ read + merge ____________/
```

Use the existing Week 1 chapters as a reference library when you need a deeper explanation. You do not need to follow them one chapter at a time.

## The Completion Contract

At the end of this path:

- `put`, `delete`, `get`, and bounded `scan` work across memory and disk;
- a frozen memtable can be flushed into an L0 SST;
- Bloom filters and prefix encoding improve the implementation without changing its results;
- the complete Week 1 test suite passes; and
- you can trace a key through the engine, explain why the newest value wins, and design a test for a plausible bug.

The tests are evidence, not the specification. Generated code remains untrusted until you can connect it to an invariant and try to falsify it.

## Start Day 1

Complete the repository and agent preparation in the [track overview](./agent-fast-forward-overview.md#prepare-the-repository-and-the-agent). With the agent running from `mini-lsm-starter` and the instruction handshake complete, send this kickoff prompt:

> We are completing Week 1 of Mini-LSM in this starter directory. Use the starter interfaces, copied Week 1 tests, and Week 1 book chapters, but never access `../mini-lsm`. Do not edit yet.
>
> Return:
>
> 1. a map of the write, read, and flush paths;
> 2. the ordering, ownership, and file-format invariants that connect their components;
> 3. an implementation plan divided into the three review gates on this page; and
> 4. any ambiguity you found between the prose, interfaces, and tests.
>
> Ask me to predict one important boundary case, then stop.

Answer the prediction before asking the agent to evaluate it. This turns the first exchange into a check of your current model rather than a generated summary to skim.

When its plan matches the three gates below, use the overview's implementation and challenge prompts for each gate in turn.

## Review Gate 1: Ordered State

Have the agent implement the memtable and iterator layers. Use the [Memtable](./week1-01-memtable.md) and [Merge Iterator](./week1-02-merge-iterator.md) chapters when the code or tests do not explain a decision.

The resulting implementation must preserve these properties:

| Invariant | How to challenge it |
| --- | --- |
| A memtable exposes keys in bytewise sorted order. | Insert keys out of order, then scan them. |
| Rewriting a key leaves only its newest value visible. | Put two values for one key before reading it. |
| An empty value is a tombstone, not a reason to search older state. | Delete a key that still has a value in an older memtable. |
| Merge inputs are ordered by recency, and the earlier input wins equal keys. | Reverse two inputs containing the same key and predict the changed result. |
| Iterators remain fused after exhaustion or error. | Call `next` again and verify that iteration does not resume. |

An ordinary write must retain the `state` read guard until insertion into the mutable memtable completes. Writing through a cloned snapshot after releasing the guard creates a race in which a concurrent freeze can make that memtable immutable before the write occurs.

Before approving this gate, ask the agent to point to the exact comparison that resolves duplicate keys. Then explain in your own words why changing `>` to `>=`, or reversing the input order, would affect correctness.

## Review Gate 2: Durable Representation

Have the agent implement blocks, block iterators, SST builders, SST readers, and SST iterators. Consult [Block](./week1-03-block.md), [Sorted String Table](./week1-04-sst.md), and [SST Optimizations](./week1-07-sst-optimizations.md) as needed.

Because this path copies all seven test suites at the start, implement the final Day 7 prefix-compressed block format directly. There is no learning value in first implementing Day 3's uncompressed key layout only to replace it during the same review gate.

Treat the file format as a protocol between the writer and reader. Check these properties:

- entries and offsets agree on exact byte boundaries;
- a seek returns the first key greater than or equal to its target;
- the first and last keys in SST metadata describe the actual key range;
- an oversized entry still produces a valid block rather than an empty table or loop;
- every prefix-compressed key decodes to its original bytes; and
- Bloom filters may return false positives but never false negatives.

Use this exact high-level SST trailer grammar so the writer and reader agree on where each variable-length section ends:

```text
blocks | metadata | metadata_offset:u32 | bloom | bloom_offset:u32
```

The final four bytes contain `bloom_offset`; the metadata offset is the four-byte field immediately before the Bloom section, at `bloom_offset - 4`.

Ask the agent to sketch one encoded block, including its entries and offset table, using three short keys. Decode the sketch manually. A convincing explanation of the format is more valuable than a summary of the Rust functions.

For an adversarial check, use keys with no shared prefix, a long shared prefix, and an empty value. Confirm that encoding and decoding preserve all three cases.

## Review Gate 3: One Logical Engine

Have the agent connect the read path, write path, freezing, flushing, SST filtering, Bloom-filter lookup, and iterator accounting. Use [Read Path](./week1-05-read-path.md) and [Write Path](./week1-06-write-path.md) when reviewing the integration.

The central rule is:

```text
mutable memtable
  > immutable memtables, newest to oldest
  > L0 SSTs, newest to oldest
```

For a duplicate key, the first visible entry wins. If that entry is a tombstone, the key is absent; an older value must not be resurrected.

Review the following state transition carefully:

```text
before: imm = [newest, ..., oldest], L0 = [newest, ..., oldest]
flush:  build an SST from the oldest immutable memtable
after:  remove exactly that memtable and insert its SST at the newest side of L0
```

Expensive SST construction and I/O should happen outside the `state` read-write lock. Structural changes still need `state_lock` so two flushes cannot select and install the same memtable concurrently.

For Day 1, `close` stops and joins the existing worker threads and is harmless when called again after their handles have already been taken. It does not implicitly flush the remaining mutable memtable. If you choose a different lifecycle contract, state it and add tests before changing the implementation.

Before approving this gate, construct one key that appears in the mutable memtable, an immutable memtable, and an L0 SST. Predict `get` and `scan` results when the newest entry is first a value and then a tombstone. Also exercise included, excluded, and unbounded scan endpoints.

## Audit the Finished Engine

Run the full project check from the repository root:

```shell
cargo x scheck
```

Then ask the agent for a final evidence report containing:

1. the commands it ran and their outcomes;
2. a data-flow trace for one `put`, one `get`, one bounded `scan`, and one flush;
3. the source-priority rule used by both point reads and scans;
4. one concurrency risk around freezing or flushing and the synchronization that prevents it;
5. one optimization that must not alter logical results; and
6. one weakness or boundary case not established by the supplied tests.

Review actual diffs and test output, not only the report. Search for removed assertions, changed tests, broad lint suppressions, and error paths replaced with `unwrap` without justification.

Finally, introduce one small, deliberate fault—for example, reverse equal-key precedence in a merge iterator or make an excluded upper bound inclusive. Predict which test should fail, run it, and revert the fault. This verifies that the tests can detect at least one mistake you understand.

## The Student Checkpoint

You are ready for Week 2 when you can do these without delegating the answer back to the agent:

- draw the write, read, and flush paths from memory;
- explain the ordering rule that selects one value from duplicate keys;
- explain the byte layout of a block and how an SST locates one;
- identify where tombstones are created and where they are filtered;
- explain why a Bloom-filter negative is safe and a positive is inconclusive;
- describe an unsafe flush interleaving and how the implementation prevents it; and
- turn a suspected invariant violation into a minimal test.

If you cannot yet do one of these, use the corresponding Week 1 chapter and ask the agent to quiz you with concrete states or byte layouts. The measure of success is not whether you typed the implementation. It is whether you can specify, inspect, test, and change the system with confidence.

{{#include copyright.md}}
