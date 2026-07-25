<!--
  mini-lsm-book © 2022-2026 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Day 1 - Build the Storage Engine

This is the first day of [Mini-LSM with Coding Agents](./agent-fast-forward-overview.md). You will use a coding agent to build, test, and explain one working storage engine from the original Mini-LSM material:

```text
put/delete -> mutable memtable -> immutable memtables -> L0 SSTs
                     \____________ read + merge ____________/
```

You decide behavior that affects correctness or the design; the agent handles mechanical implementation. A short request begins a guided dialogue, not a complete implementation in one turn.

## What You Will Finish

At the end of this path:

- `put`, `delete`, `get`, and bounded `scan` work across memory and disk;
- a frozen memtable can be flushed into an L0 SST;
- Bloom filters and prefix encoding improve the implementation without changing its results;
- the complete supplied test suite passes; and
- you can trace a key through the engine, explain why the newest value wins, and design a test for a plausible bug.

The tests are evidence, not the specification. Generated code remains untrusted until you can connect it to a decision and an invariant and try to falsify it.

## Copy the Complete Test Suite

Complete the repository and agent preparation in the [track overview](./agent-fast-forward-overview.md#prepare-the-repository-and-the-agent). Leave the agent at the instruction-handshake stop, then run these commands from the repository root:

```shell
cargo x copy-test --week 1
cargo x scheck
```

The initial check should fail because the starter contains unfinished code. Record the first failure as a reproducible baseline. Do not ask the agent to make it disappear by changing the tests.

## Start Day 1

With the agent running from `mini-lsm-starter`, the instruction handshake complete, and the tests copied, send:

> Build Day 1 with me, starting with ordered in-memory state. Follow the student-owned design protocol in `AGENTS.md` and never access `../mini-lsm`. Before coding, ask one short question at a time using a concrete example. Use plain English and introduce technical terms after I answer. Mark each question **Course rule** or **Your choice**. I may reply `simpler`, `example`, `hint`, or `choose for me`. Do not edit until my answers specify one small, coherent slice. After each slice, show me one important line and ask what it does and what would break if it changed.

The first useful response is a concrete question about ordered state, not an architecture essay or a patch. Starting with Checkpoint 1 gives you one predictable path through the day. You may reorder checkpoints if you already understand their dependencies.

The tables below are audit guides. Do not paste a whole table back as a ready-made specification. Let the agent elicit one topic at a time, and use the table afterward to check whether the dialogue covered the important ground.

## Checkpoint 1: Ordered State

Ask:

> Implement ordered in-memory state.

This checkpoint covers the memtable and iterator layers. Use the [Memtable](./week1-01-memtable.md) and [Merge Iterator](./week1-02-merge-iterator.md) chapters when a question needs more context.

The agent should use concrete examples to help you work out at least these course rules:

| Behavior to work out | Small case that exposes it |
| --- | --- |
| Memtable ordering | Insert keys out of bytewise order, then scan. |
| Duplicate precedence inside a memtable | Put two values for one key before reading it. |
| Tombstone representation | Delete a key that still has a value in older state. |
| Equal-key precedence across merge inputs | Reverse two inputs containing the same key. |
| Iterator behavior after exhaustion or error | Call `next` again and check that iteration cannot resume. |
| Write/freeze synchronization | Interleave insertion with a concurrent memtable freeze. |

One critical outcome is that an ordinary write retains the `state` read guard until insertion into the mutable memtable completes. Writing through a cloned snapshot after releasing the guard permits a concurrent freeze to make that memtable immutable before the write occurs.

Once the representation and ordering rules are settled, authorize the smallest coherent slice. After its focused test passes, have the agent point to the exact comparison that resolves duplicate keys. Explain in your own words what that line is trying to do and why changing `>` to `>=`, or reversing input order, changes the visible value.

## Checkpoint 2: Durable Representation

Ask:

> Implement the durable block and SST representation.

This checkpoint covers blocks, block iterators, SST builders, SST readers, SST iterators, Bloom filters, and final prefix compression. Consult [Block](./week1-03-block.md), [Sorted String Table](./week1-04-sst.md), and [SST Optimizations](./week1-07-sst-optimizations.md) when a question requires deeper context.

Because this path copies all seven test suites at the start, the accepted design should converge directly on the final Day 7 prefix-compressed block format. Do not first implement the Day 3 layout and silently replace it. The agent should explicitly ask which acceptance target applies and record that choice.

First use concrete byte examples to derive the parts fixed by the course:

| Behavior to work out | Small case that exposes it |
| --- | --- |
| Integer encoding and exact block layout | Independently decode a three-entry block. |
| Inclusive target-size accounting | Add an entry that makes the encoded block exactly the target size. |
| Oversized first entry | Try to add a single representable record larger than the target. |
| Prefix basis and reconstruction | Mix keys with a long shared prefix and no shared prefix. |
| Lower-bound seek semantics | Seek before, between, at, and after stored keys. |
| SST trailer placement | Locate metadata and Bloom sections from the end of the file. |
| Bloom-filter authority | Compare a definite negative with a possible positive. |

Then make the genuinely open implementation choices, such as scanning versus binary search when looking for the first key at or after a target, allocating versus reusing memory for reconstructed keys, and how much malformed-input checking to add without changing the provided API. Record required course rules separately from choices you made.

The high-level SST trailer grammar is:

```text
blocks | metadata | metadata_offset:u32 | bloom | bloom_offset:u32
```

The final four bytes contain `bloom_offset`; the metadata offset is the four-byte field immediately before the Bloom section, at `bloom_offset - 4`.

Treat the writer and reader as two parties implementing a protocol. A round-trip test alone can miss an encoder and decoder that agree with each other but disagree with the specified bytes. Ask for at least one independently calculated byte layout.

<details>
<summary>Worked example: one plain-English block-format stop</summary>

> **Student:** Implement block format.
>
> **Agent:** **Course rule — What must one entry depend on?**
>
> A block contains `mini-a`, `mini-b`, and `mini-c`. The iterator is allowed to jump straight to `mini-c`. If `mini-c` stores only what changed from `mini-b`, can the iterator reconstruct it without first reading `mini-b`? Should each compressed key therefore compare itself with the first key or the previous key?
>
> You can reply `simpler`, `example`, `hint`, or `choose for me`.
>
> **Student:** Compare with the first key. Then the iterator needs only the first key and the selected entry, rather than a chain of earlier entries.
>
> **Agent:** Correct. This is called the prefix basis. I will record “first key” as a course rule. One consequence is that an iterator can inspect any entry independently, which permits either a linear scan or binary search. That search method is a **Your choice** stop we can discuss next.

</details>

The example begins with an operation the student can picture and names the concept afterward. A real implementation stop must still preview the files and focused test, wait for authorization, report actual evidence, and stop before beginning the next slice. An unexpected test failure either reveals a mechanical bug or reopens one specific decision.

Before approving the completed checkpoint, decode three keys by hand: one with no shared prefix, one with a long shared prefix, and one with an empty value. Confirm that a seek returns the first key greater than or equal to its target and that a Bloom-filter negative can never hide a present key.

## Checkpoint 3: One Logical Engine

Ask:

> Connect the components into one logical engine.

This checkpoint covers the read path, write path, freezing, flushing, SST filtering, and iterator accounting. Use [Read Path](./week1-05-read-path.md) and [Write Path](./week1-06-write-path.md) when a decision needs more context.

The agent must lead contract derivations for source ordering, tombstone filtering, scan bounds, flush selection, and lifecycle behavior. It should separately ask about open implementation choices such as iterator composition, lock scope that still preserves the synchronization contract, and where to perform expensive SST construction.

The central source-priority rule should emerge from those choices:

```text
mutable memtable
  > immutable memtables, newest to oldest
  > L0 SSTs, newest to oldest
```

For a duplicate key, the first visible entry wins. If it is a tombstone, the key is absent; an older value must not be resurrected.

Review the flush transition with concrete state:

```text
before: imm = [newest, ..., oldest], L0 = [newest, ..., oldest]
flush:  build an SST from the oldest immutable memtable
after:  remove exactly that memtable and insert its SST at the newest side of L0
```

Expensive SST construction and I/O should happen outside the `state` read-write lock. Structural changes still need `state_lock` so two flushes cannot select and install the same memtable concurrently.

For Day 1, the lifecycle contract is fixed: `close` stops and joins the existing worker threads and is harmless when called again after their handles have already been taken. It does not implicitly flush the remaining mutable memtable. Treat a different contract as an explicit scope change, not an ordinary implementation preference.

Before approving this checkpoint, construct one key that appears in the mutable memtable, an immutable memtable, and an L0 SST. Predict `get` and `scan` when the newest entry is first a value and then a tombstone. Also exercise included, excluded, and unbounded scan endpoints.

## Audit the Finished Engine

Run the full project check from the repository root:

```shell
cargo x scheck
```

Then ask the agent for a final evidence report containing:

1. the combined decision ledgers and any delegated choices;
2. the commands it ran and their outcomes;
3. a data-flow trace for one `put`, one `get`, one bounded `scan`, and one flush;
4. the source-priority rule used by both point reads and scans;
5. one concurrency risk around freezing or flushing and the synchronization that prevents it;
6. one optimization that must not alter logical results; and
7. one weakness or boundary case not established by the supplied tests.

Review actual diffs and test output, not only the report. Search for removed assertions, changed tests, broad lint suppressions, and error paths replaced with `unwrap` without justification.

Finally, introduce one small, deliberate fault—for example, reverse equal-key precedence in a merge iterator or make an excluded upper bound inclusive. Predict which test should fail, run it, and revert the fault. This verifies that the tests can detect at least one mistake you understand.

## Day 1 Completion Checkpoint

You are ready for Day 2 when you can do these without delegating the answer back to the agent:

- draw the write, read, and flush paths from memory;
- explain the ordering rule that selects one value from duplicate keys;
- explain the byte layout of a block and how an SST locates one;
- identify where tombstones are created and where they are filtered;
- explain why a Bloom-filter negative is safe and a positive is inconclusive;
- describe an unsafe flush interleaving and how the implementation prevents it; and
- turn a suspected invariant violation into a minimal test.

If you cannot yet do one of these, use the corresponding Mini-LSM chapter and ask the agent to quiz you with concrete states or byte layouts. The measure of success is not whether you typed the implementation. It is whether you can specify, inspect, test, and change the system with confidence.

{{#include copyright.md}}
