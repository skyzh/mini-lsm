<!--
  mini-lsm-book © 2022-2025 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Fast-Forward Week 1: Build Mini-LSM with a Coding Agent

This is an alternative path through Week 1 for students who intend to use a coding agent. The agent may write most of the code. Your job is to define what correct means, constrain the work, challenge the result, and leave with a mental model you can use without the agent.

The goal is not to finish seven chapters as quickly as possible. It is to build and defend one working storage engine:

```text
put/delete -> mutable memtable -> immutable memtables -> L0 SSTs
                     \____________ read + merge ____________/
```

Use the existing Week 1 chapters as a reference library when you need a deeper explanation. You do not need to follow them one day at a time.

## The Completion Contract

At the end of this path:

- `put`, `delete`, `get`, and bounded `scan` work across memory and disk;
- a frozen memtable can be flushed into an L0 SST;
- Bloom filters and prefix encoding improve the implementation without changing its results;
- the complete Week 1 test suite passes; and
- you can trace a key through the engine, explain why the newest value wins, and design a test for a plausible bug.

The tests are evidence, not the specification. Generated code remains untrusted until you can connect it to an invariant and try to falsify it.

## Prepare the Repository and the Agent

This section contains the complete setup for the agent-assisted path. Do all repository-wide preparation first, then start the agent from the starter directory—not from the repository root.

### 1. Install the Toolchain and Course Tools

Install Rust with [rustup](https://rustup.rs) if it is not already available. Then clone the repository and install the tools used by the course:

```shell
git clone https://github.com/skyzh/mini-lsm
cd mini-lsm
cargo x install-tools
```

The repository pins its Rust toolchain in `rust-toolchain.toml`, so Cargo will select it automatically when Rust is managed by `rustup`.

If you already have the repository and tools, update your checkout as appropriate and begin from the repository root.

### 2. Copy the Complete Week 1 Test Suite

The normal course reveals tests one chapter at a time. The fast-forward path starts with the complete acceptance suite:

```shell
for day in 1 2 3 4 5 6 7; do
  cargo x copy-test --week 1 --day "$day"
done
cargo x scheck
```

The initial check should fail because the starter contains unfinished code. Record the first failure; it gives you a reproducible baseline. Do not ask the agent to make this failure disappear by changing the tests.

### 3. Start the Agent from `mini-lsm-starter`

Change into the starter directory before launching your coding agent:

```shell
cd mini-lsm-starter
pwd
# Start your coding agent here using the command for your tool.
```

The final component of `pwd` should be `mini-lsm-starter`. This matters for two reasons:

1. repository-aware agents discover the `AGENTS.md` in this directory and apply its learning constraints; and
2. the agent begins with the starter as its working scope instead of treating the neighboring reference implementation as ordinary project context.

Starting in this directory is not a security sandbox: an agent can still traverse to a parent directory if instructed. The local `AGENTS.md` therefore explicitly prohibits reading, searching, diffing, or copying `../mini-lsm/`, including attempts to reconstruct the solution through Git history or an online copy.

Do not open the whole repository as the agent's workspace if your tool lets you choose a directory. Open `mini-lsm-starter`. The agent may consult the copied tests, starter interfaces, Rust documentation, and the Week 1 chapters under `../mini-lsm-book/src/`.

### 4. Verify the Instructions Before Coding

Do not assume the tool discovered `AGENTS.md`. Make the first prompt a handshake that performs no implementation:

> Before editing anything, confirm that your working directory is `mini-lsm-starter` and read `./AGENTS.md`. Summarize its hard boundaries and working agreement. You must never inspect or copy the reference solution in `../mini-lsm`, directly or indirectly. Tell me which local sources you are allowed to use, then stop without changing files.

If the response omits the reference-solution boundary, test protection, or review stops, correct the agent before continuing. If the tool cannot load repository instructions automatically, paste the contents of `AGENTS.md` into its persistent project instructions.

## Prompt the Agent in Reviewable Steps

A useful prompt states the scope, invariant, evidence, and stopping point. “Implement Week 1 and make the tests pass” gives the agent no reason to expose its assumptions and gives you no natural place to inspect them.

Use three kinds of prompts throughout this path.

### Prompt 1: Ask for a Model, Not Code

After the instruction handshake, ask the agent to understand the whole task without editing:

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

### Prompt 2: Implement One Gate

Use a fresh prompt for each review gate:

> Implement only Review Gate `<number and name>`. Before editing, restate the invariants for this gate and list the files you expect to change. Keep the diff focused and do not modify supplied tests, public interfaces, or unrelated code.
>
> Run focused checks while working. When the gate is implemented, stop and report the changed behavior, the exact commands and results, one remaining uncertainty, and one adversarial case that I should predict. Do not continue to the next gate.

Replace the placeholder with the gate below. A gate may require several internal iterations, but it should produce one coherent diff that you can review before the next subsystem depends on it.

### Prompt 3: Challenge the Result

After inspecting the diff and answering the agent's boundary question, ask for evidence rather than reassurance:

> Review this gate as an untrusted contribution. Connect each changed behavior to an invariant and a supplied test. Identify one plausible bug that could still pass those tests, propose the smallest additional test or manual check that exposes it, and wait for my approval before adding that test. If you find a real problem, explain the failing invariant before changing the implementation.

Do not let “all tests pass” end the review. Conversely, do not ask the agent to invent speculative refactors once the gate's contract and adversarial checks are satisfied.

Repeat Prompts 2 and 3 for each gate. The review stops are where you catch a locally reasonable decision before it spreads across the system.

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

Before approving this gate, ask the agent to point to the exact comparison that resolves duplicate keys. Then explain in your own words why changing `>` to `>=`, or reversing the input order, would affect correctness.

## Review Gate 2: Durable Representation

Have the agent implement blocks, block iterators, SST builders, SST readers, and SST iterators. Consult [Block](./week1-03-block.md), [Sorted String Table](./week1-04-sst.md), and [SST Optimizations](./week1-07-sst-optimizations.md) as needed.

Treat the file format as a protocol between the writer and reader. Check these properties:

- entries and offsets agree on exact byte boundaries;
- a seek returns the first key greater than or equal to its target;
- the first and last keys in SST metadata describe the actual key range;
- an oversized entry still produces a valid block rather than an empty table or loop;
- every prefix-compressed key decodes to its original bytes; and
- Bloom filters may return false positives but never false negatives.

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
