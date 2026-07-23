<!--
  mini-lsm-book © 2022-2026 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Day 1 - Mini-LSM

This is the first day of [Agent Fast Forward in 3 Days](./agent-fast-forward-overview.md). You will use a coding agent to build and defend one working storage engine from the original Mini-LSM material:

```text
put/delete -> mutable memtable -> immutable memtables -> L0 SSTs
                     \____________ read + merge ____________/
```

You decide consequential behavior; the agent handles mechanical implementation. A short request begins a design interview; it must not produce a complete implementation in one turn.

## The Completion Contract

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

> Implement the Mini-LSM write, read, and flush paths. Follow the student-owned design protocol in `AGENTS.md`. Ask one design decision at a time, record my accepted choices, and do not edit until those choices specify one coherent slice. Never access `../mini-lsm`.

The first useful response is a question, not an architecture essay or a patch. It will usually ask you to choose the first checkpoint. Select one of the three below and explain why it is a useful boundary.

For each checkpoint, the tables below are an audit guide for you. Do not paste a whole table back as a ready-made specification. Make the agent elicit one topic at a time, label it as a fixed contract or an open choice, and use the table afterward to check whether the dialogue covered the important ground.

## Checkpoint 1: Ordered State

Ask:

> Implement ordered in-memory state.

This checkpoint covers the memtable and iterator layers. Use the [Memtable](./week1-01-memtable.md) and [Merge Iterator](./week1-02-merge-iterator.md) chapters when a question needs more context.

The agent should stop on at least these contract topics and ask you to derive the required behavior:

| Decision | Case that exposes it |
| --- | --- |
| Memtable ordering | Insert keys out of bytewise order, then scan. |
| Duplicate precedence inside a memtable | Put two values for one key before reading it. |
| Tombstone representation | Delete a key that still has a value in older state. |
| Equal-key precedence across merge inputs | Reverse two inputs containing the same key. |
| Iterator behavior after exhaustion or error | Call `next` again and check that iteration cannot resume. |
| Write/freeze synchronization | Interleave insertion with a concurrent memtable freeze. |

One critical outcome is that an ordinary write retains the `state` read guard until insertion into the mutable memtable completes. Writing through a cloned snapshot after releasing the guard permits a concurrent freeze to make that memtable immutable before the write occurs.

Once the representation and precedence choices are settled, authorize the smallest coherent slice. After its focused test passes, ask the agent to identify the exact comparison that resolves duplicate keys. Explain in your own words why changing `>` to `>=`, or reversing input order, changes the visible value.

## Checkpoint 2: Durable Representation

Ask:

> Implement the durable block and SST representation.

This checkpoint covers blocks, block iterators, SST builders, SST readers, SST iterators, Bloom filters, and final prefix compression. Consult [Block](./week1-03-block.md), [Sorted String Table](./week1-04-sst.md), and [SST Optimizations](./week1-07-sst-optimizations.md) when a question requires deeper context.

Because this path copies all seven test suites at the start, the accepted design should converge directly on the final Day 7 prefix-compressed block format. Do not first implement the Day 3 layout and silently replace it. The agent should explicitly ask which acceptance target applies and record that choice.

First derive the parts fixed by the course contract:

| Contract | Case that exposes it |
| --- | --- |
| Integer encoding and exact block layout | Independently decode a three-entry block. |
| Inclusive target-size accounting | Add an entry that makes the encoded block exactly the target size. |
| Oversized first entry | Try to add a single representable record larger than the target. |
| Prefix basis and reconstruction | Mix keys with a long shared prefix and no shared prefix. |
| Lower-bound seek semantics | Seek before, between, at, and after stored keys. |
| SST trailer placement | Locate metadata and Bloom sections from the end of the file. |
| Bloom-filter authority | Compare a definite negative with a possible positive. |

Then make the genuinely open implementation choices, such as linear versus binary lower-bound search when both satisfy the interface, allocation versus reuse for reconstructed keys, and how much malformed-input checking to add without changing the provided API. Record required protocol facts as constraints and open selections as choices.

The high-level SST trailer grammar is:

```text
blocks | metadata | metadata_offset:u32 | bloom | bloom_offset:u32
```

The final four bytes contain `bloom_offset`; the metadata offset is the four-byte field immediately before the Bloom section, at `bloom_offset - 4`.

Treat the writer and reader as two parties implementing a protocol. A round-trip test alone can miss an encoder and decoder that agree with each other but disagree with the specified bytes. Ask for at least one independently calculated byte layout.

### Illustrative Dialogue: “Implement Block Format”

This is a condensed example of the expected interaction, including representative command output rather than a transcript captured from your machine. It targets the final prefix-compressed format required by fast-forward Day 1.

> **Student:** Implement block format.
>
> **Agent:** This can be split into builder/encoding and iterator/seek slices. Which slice should establish the contract first, and why?
>
> **Student:** Builder and encoding first. The iterator consumes that representation, while encoding can be checked independently against exact bytes.
>
> **Agent:** Contract derivation: the full Week 1 suite is installed. Should this slice implement the earlier uncompressed Day 3 entry or the final Day 7 prefix-compressed entry? What evidence decides?
>
> **Student:** The final Day 7 format. Replacing an intermediate format inside the same checkpoint would be throwaway work, and the installed acceptance target includes prefix compression.
>
> **Agent:** Contract derivation: a compressed key can be relative to the previous key or the first key in the block. Which basis does the course require, and what does it permit an iterator to do?
>
> **Student:** Use the first key. Then any entry can be reconstructed independently once the first key is known; using the previous key would require decoding the chain before it.
>
> **Agent:** Derive the projected encoded size for a new entry with `rest_key_len` suffix bytes and decide whether an exact fit is admitted.
>
> **Student:** For `n` existing entries: `data.len() + 2 + 2 + rest_key_len + 2 + value.len() + 2 * (n + 1) + 2`. The fields are overlap length, suffix length, suffix, value length, value, all offsets after the add, and the one final count. Equality fits.
>
> **Agent:** Open choice: because first-key compression makes an arbitrary entry independently decodable, lower-bound seek can scan linearly or binary-search offsets. Which do you want?
>
> **Student:** Binary search. The entries are sorted, and independent reconstruction makes each comparison possible without decoding all preceding entries.
>
> **Agent:** Open choice: `decode` has no error return. Should this checkpoint trust encoder-produced bytes, add assertions that still panic, or request an interface change to return `Result`?
>
> **Student:** Trust encoder-produced bytes for this checkpoint and document the boundary. A production decoder should return `Result` and validate counts, offsets, and lengths, but changing the interface is outside scope.
>
> **Agent:** The slice is specified. I will change `src/block/builder.rs`, `src/block.rs`, and `src/block/iterator.rs`, then run `cargo test --lib tests::week1_day7::test_task3_block_key_compression`. Prediction first: with empty values, `mini-a` encodes as an overlap of 0 plus a six-byte suffix, and `mini-b` as an overlap of 5 plus a one-byte suffix. What are the two entry offsets and final encoded size?
>
> **Student:** The first entry is 12 bytes and starts at 0. The second is 7 bytes and starts at 12. Two offsets and the final count add 6 bytes, so the encoded block is 25 bytes. I authorize this slice.
>
> **Agent:** Implemented only the authorized block slice. `cargo test --lib tests::week1_day7::test_task3_block_key_compression` exited 0. The entries reconstructed as `mini-a` and `mini-b`, the empty values survived, and the encoded length was 25 bytes. I did not begin the SST layer. One remaining weakness is that the supplied valid-block tests do not establish safe behavior for malformed offsets. Before I add an adversarial test, predict what the current trusted decoder would do with an offset beyond the data section.

The final agent turn contains evidence, a checkpoint stop, and a new question. It does not treat a passing test as permission to continue. In a real session, inspect the diff and actual command output before accepting that report. An unexpected test failure either reveals a mechanical bug or reopens one specific decision.

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
