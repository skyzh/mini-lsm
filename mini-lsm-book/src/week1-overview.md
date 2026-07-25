<!--
  mini-lsm-book © 2022-2026 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Week 1 Overview: Mini-LSM

![Chapter Overview](./lsm-tutorial/week1-overview.svg)

In the first week, you will build the storage engine's core formats, read path, and write path. By the end of its seven chapters, you will have a working LSM-based key-value store.

| Chapter | Before | After |
| --- | --- | --- |
| [Day 1: Memtable](./week1-01-memtable.md) | The storage interfaces are stubs. | The engine supports in-memory point reads, writes, deletes, and memtable freezing. |
| [Day 2: Merge Iterator](./week1-02-merge-iterator.md) | The engine can query one key at a time. | It can scan an ordered range across multiple memtables. |
| [Day 3: Block Encoding](./week1-03-block.md) | All data structures are in memory. | Key-value pairs can be encoded into and decoded from an on-disk block format. |
| [Day 4: SST Encoding](./week1-04-sst.md) | The engine has individual blocks. | Blocks form seekable SSTs whose data is loaded on demand and cached. |
| [Day 5: Read Path](./week1-05-read-path.md) | Memtables and SSTs have separate iterators. | Point reads and scans produce one logical view across both. |
| [Day 6: Write Path](./week1-06-write-path.md) | The test harness creates SSTs for you. | The engine flushes frozen memtables to L0 and filters irrelevant SSTs. |
| [Day 7: SST Optimizations](./week1-07-sst-optimizations.md) | The engine is correct but performs avoidable I/O and stores repeated key bytes. | Bloom filters reduce point-read I/O, and prefix encoding makes blocks smaller. |

## How to Use This Week

The implementation is the laboratory in which you explore the design. Passing the tests is an important checkpoint, but it is not the final learning goal: the provided tests cannot cover every boundary condition, malformed input, or concurrent execution.

For each chapter:

1. Read the capability and core-invariant sections before writing code.
2. Predict the behavior of the small examples without running them.
3. Implement the tasks and run the chapter tests.
4. Answer the correctness questions with evidence from your implementation. When a question asks what can go wrong, construct a minimal counterexample or test.
5. Compare with the reference solution only after making a serious attempt. A different implementation can still be correct if it preserves the same invariants.

You may use an LLM or other coding tools, but treat generated code as an untrusted contribution: identify the invariants first, review the result against them, and add tests that exercise behavior not covered by the supplied suite. The ability to explain, challenge, and validate an implementation matters more than who typed it.

At the end of the week, your storage engine should be able to handle `get`, `scan`, `put`, and `delete` requests. The remaining work is to persist the LSM state across restarts and organize SSTs on disk more efficiently. You will have a working **Mini-LSM** storage engine.

Before moving to Week 2, check that you can explain:

- why each component introduced this week is necessary;
- the central correctness invariant of each component;
- one plausible bug in each component and a test that exposes it;
- which data is in memory, which data is on disk, and which structure owns each piece;
- how the read and write paths choose the newest visible value for a key.

## End-of-Week Self-Check

Try these scenarios without running the code. Open the answer criteria only after writing down both the result and the invariant that produces it.

### 1. One View from Several Sources

The mutable memtable contains `b -> delete` and `d -> 4`. The newest immutable memtable contains `a -> 1` and `b -> 2`. L0, from newest to oldest, contains one SST with `a -> 0`, `c -> 3`, and `d -> 3`. What do `get(a)`, `get(b)`, and the inclusive scan `[a, d]` return?

<details>

<summary>Answer criteria</summary>

`get(a)` returns `1`, because every memtable is newer than every L0 SST. `get(b)` returns not found: the tombstone wins and must stop the search. The scan returns `a -> 1, c -> 3, d -> 4`. A complete explanation identifies where duplicate keys are resolved before tombstones are hidden and why the final stream is sorted and unique.

</details>

### 2. Freeze and Flush without Changing Results

The immutable-memtable IDs are `[7, 6, 5]` from newest to oldest, and L0 IDs are `[4, 3]` from newest to oldest. After one correct flush, what are the two lists? Which logical read result is allowed to change because of the flush?

<details>

<summary>Answer criteria</summary>

The oldest immutable memtable, ID 5, is flushed, so the lists become `imm_memtables = [7, 6]` and `l0_sstables = [5, 4, 3]`. The SST reuses the memtable ID. No logical read result may change: the state update replaces one physical representation with another at the same recency position.

</details>

### 3. Safe Optimizations

An SST's range contains `k`, but its Bloom filter reports “may contain,” and an SST seek lands on the next key `m`. May `get(k)` return `m`? Would the answer change if the Bloom filter reported “definitely absent”?

<details>

<summary>Answer criteria</summary>

“May contain” is not proof of membership, so the engine must seek and then check exact key equality; it cannot return `m`. A valid “definitely absent” result lets the engine skip the SST entirely. Both optimizations may reduce work but must not change the value returned.

</details>

{{#include copyright.md}}
