<!--
  mini-lsm-book © 2022-2026 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Day 3 - Transactions and MVCC

This is the third day of [Mini-LSM with Coding Agents](./agent-fast-forward-overview.md). You will turn the persistent Day 2 engine into a multi-version engine with stable snapshots, transaction-local writes, safe history reclamation, and commit-time conflict validation:

```text
user operations -> transaction snapshot + private workspace -> atomic commit
                          |                              |
                          v                              v
                 visible versions              timestamped memtable/WAL
                          \________ watermark + compaction ________/
```

Week 3 adds several guarantees that are easy to blur together. Naming the guarantee before choosing a mechanism is part of the work. A shared timestamp can make a batch visible at once without making it durable, and a stable snapshot can still permit write skew.

## What You Will Finish

At the end of this path:

- internal keys preserve `user key ascending, timestamp descending` through memory, SSTs, and compaction;
- every read in a transaction observes one stable timestamp, including after later writes, flushes, and compactions;
- a watermark protects the versions still needed by active transactions and scan iterators;
- transaction-local writes provide read-your-writes and commit as one timestamped, crash-atomic WAL batch;
- point-key validation rejects dependencies that would otherwise produce write skew;
- the documented scan-phantom limitation remains visible instead of being mislabeled as full serializability;
- compaction filters preserve versions above the watermark while making reads in a filtered prefix intentionally undefined; and
- you can distinguish version order, visibility, atomicity, durability, and isolation when reviewing code and failures.

The supplied tests are evidence for these guarantees, not their definitions. In particular, a latest-state test can pass while an old snapshot is broken, and key-only validation can pass while a range phantom remains possible.

## Keep a Guarantee Ledger

Extend the decision ledger from Days 1 and 2 with this Week 3 view. Fill in the mechanism and evidence as each checkpoint settles them:

| Guarantee | Question to keep separate | Completed in |
| --- | --- | --- |
| Version order | Which internal versions exist, and which comes first? | Checkpoint 1 |
| Snapshot visibility | Which committed version may this `read_ts` observe? | Checkpoint 2 |
| Atomic visibility | Can a reader observe only part of one committed batch? | Checkpoint 3 |
| Crash atomicity | Can recovery expose a prefix of a torn batch? | Checkpoint 3 |
| Durability | After which successful operation must the batch survive a crash? | Checkpoint 3 |
| Isolation | Is the outcome equivalent to an allowed serial order? | Checkpoint 4, for tracked point-key dependencies |

For every adversarial case, first name which row it challenges. If an explanation uses “atomic” or “consistent,” require the speaker to say exactly which kind.

## Prepare Day 3

Begin from a Day 2 implementation that passes its complete suite. In this chapter, **Day 3** names the course day, while `--week 3 --day N` names progressive **Week 3 test module Day N**. Use that full phrase when crossing test gates so the test-module number is not mistaken for the course day.

Week 3 changes public interfaces as it proceeds, so copy tests progressively; later test modules may not compile against an intentionally incomplete earlier checkpoint. The preparation command below is the **sole course-defined exception** to the starter rule that supplied tests are revealed only after an independent first pass: copy Week 3 test module Day 1 before implementation so the initial compatibility failure is captured as evidence. After this preparation exception, do not copy any later Week 3 test module until the corresponding slice has a complete independent first pass.

From the repository root, copy Week 3 test module Day 1 and record the first failure:

```shell
cargo x copy-test --week 3 --day 1
cargo x scheck
```

The starter's timestamp-aware key module is a provided course input. Copy it with this exact repository-root command; do not browse sibling implementations:

```shell
cp mini-lsm-mvcc/src/key.rs mini-lsm-starter/src/key.rs
```

After the copy, the key module is part of the permitted starter workspace. Never open its source path directly. Everything else under `mini-lsm-mvcc/`, and the entire reference implementation under `mini-lsm/`, remain prohibited except for `cargo x copy-test`.

The key copy command above and every `cargo x copy-test` command in this chapter run from the repository root even though the guided agent normally works from `mini-lsm-starter`. Treat each directory change as a narrow course operation: run the command exactly as shown, then return the agent to `mini-lsm-starter` before inspecting or editing anything. Do not replace it with exploratory reads of either source directory.

With the agent running from `mini-lsm-starter`, send:

> Build Day 3 with me, starting with timestamped internal keys and batches. Follow the student-owned design protocol in `AGENTS.md`. Never access `../mini-lsm` or `../mini-lsm-mvcc`; only the exact key and test copy commands documented for Week 3 may read from those sources. Maintain both a decision ledger and the Day 3 guarantee ledger. Ask one short question at a time using a concrete version stream, snapshot, crash point, or transaction interleaving. Name the guarantee being tested, then mark the decision **Course rule** or **Your choice**. I may reply `simpler`, `example`, `hint`, or `choose for me`. Do not edit until my answers specify one small, coherent slice. After each slice, connect one important line to its guarantee and ask what would break if it changed.

The first useful response orders a few timestamped keys. It should not define every MVCC term, present the whole checkpoint table, or edit code. Use the checkpoints in order unless you can explain why another order preserves their dependencies.

## Checkpoint 1: Store and Order Versions

Ask:

> Refactor the engine to store timestamped versions and make latest-state reads select one version.

This checkpoint combines the representation and write-path work from [Timestamp Key Encoding](./week3-01-ts-key-refactor.md) and [Memtables and Timestamps](./week3-02-snapshot-read-part-1.md). The accepted design should use the final Week 3 batch-capable WAL framing now; this intentionally overrides the linked Day 2 chapter's temporary per-record format, so do not implement both.

Derive these course rules from small states and byte layouts:

| Behavior to work out | Small case that exposes it |
| --- | --- |
| Internal ordering | Sort `a@9`, `a@4`, `aa@7`, and `b@2`. |
| Prefix compression | Encode two versions with the same user key and reconstruct each independently. |
| SST summaries | Seek across a block boundary whose first and last user keys have several timestamps. |
| Bloom-filter identity | Compare the fingerprints for `k@8` and `k@3`. |
| User-range mapping | Translate `(a, b]` into internal bounds without leaking any `a` version or losing a `b` version. |
| Batch timestamp allocation | Commit two concurrent batches, each containing two keys. |
| Compaction output splitting | Cross the SST target in the middle of one user's version history. |
| Latest-state collapse | Read `k@9=delete, k@7=v7, k@3=v3` at the latest timestamp. |

The fixed order is user-key bytes ascending and timestamp descending. Prefix compression applies only to user-key bytes; each timestamp is stored in full. Block metadata preserves complete internal first and last keys, while Bloom filters hash only user-key bytes because a lookup asks whether any version of that user key may exist.

Translate each user bound once into an internal-key bound, then use that same result for memtable scans, L0 SST seeks, and leveled-SST seeks. Trace all four lower/upper inclusion combinations before and after flushing into both L0 and a leveled SST; no storage path may reinterpret the user bounds independently.

One committed write batch receives one new timestamp. Hold `write_lock` across timestamp allocation, insertion into one memtable and WAL, and publication of `latest_commit_ts`; otherwise concurrent writers can reuse or publish timestamps out of order. A batch may exceed the memtable size target, but it must not be split between memtables.

Use the final WAL batch grammar:

```text
batch_body_len:u32
  | key_len:u16 | user_key | ts:u64 | value_len:u16 | value
  | ...
  | body_checksum:u32
```

The checksum covers the complete body. Reject keys, values, and batch bodies that do not fit their `u16` or `u32` fields instead of truncating a cast. `Wal::put` should use the batch path for one record.

Recovery uses one complete frame, or write batch, as its atomicity unit. Clean EOF succeeds when the cursor is exactly between frames. If EOF arrives inside the final frame's header, body, or checksum, treat that frame as a crash-torn tail: emit a warning, discard only the incomplete frame, and return success. Earlier complete frames remain replayable.

Stage the entries for one frame separately and publish them only after that frame's lengths and checksum validate. A fully present frame with a checksum mismatch or invalid nested key/value length returns `Err` as corruption; do not treat it as a normal torn tail. If a bad frame follows a valid one, entries from the earlier validated frame remain replayed, but recovery never publishes a prefix of the bad frame.

Append the complete WAL frame before inserting any entry into the memtable, and publish `latest_commit_ts` only after every memtable entry is installed. A WAL error therefore exposes no in-memory update. After a process stop, recovery may replay every complete persisted frame and discard an unsynchronized torn final frame, but it must never expose a prefix of any frame. Synchronization remains a separate durability guarantee.

Compaction keeps every version for now. It may split output between user keys but never between versions of one user key. Latest-state `get` and `scan` skip repeated versions after selecting the newest visible entry and treat a selected tombstone as absence rather than continuing to an older value.

Reusing the merged scan path for `get` does not remove the point-read Bloom optimization. Hash only the user key and discard impossible SST probes before constructing their iterators; otherwise a correct refactor can quietly turn every point read into work across all overlapping files.

Checkpoint 1 is not one edit. Use this default slice order and obtain separate authorization for each slice:

1. block/SST encoding, metadata, and Bloom identity;
2. finish the timestamp-zero Week 3 test module Day 1 engine refactor and pass its focused tests;
3. implement timestamped memtables, internal range bounds, latest-state iteration, final WAL framing, batch timestamp allocation, and the write path; and
4. implement compaction retention and output boundaries.

After slice 3 forms one compiling first pass, run the non-test gate and only then reveal Week 3 test module Day 2:

```shell
cargo check -p mini-lsm-starter --lib
cargo x copy-test --week 3 --day 2
```

Then implement slice 4 before the focused tests and independent checks below.

Before approving the checkpoint, independently calculate one encoded entry and exercise all four included/excluded range combinations through the memtable and again after flush into L0 and a leveled SST. After one valid WAL frame, truncate a second frame in its header, body, and checksum; each case should warn, succeed, replay the first frame, and publish none of the second. Then corrupt the checksum or nested lengths of a fully present second frame; recovery should return `Err`, retain the first frame's entries, and publish none of the corrupt frame. Explain which guarantee each case checks. After focused tests pass, have the agent point to the shared user-bound conversion, the internal-key comparison, and the condition that prevents an SST split within one user key.

Do not use the complete historical suite as the pass/fail gate for this intermediate state. Checkpoint 1 deliberately retains bottom-level tombstones, while the Week 2 all-tombstones test expects the pre-MVCC compactor to remove them. Run the focused Week 3 test modules Day 1 and Day 2 instead:

```shell
cargo test -p mini-lsm-starter --lib week3_day
```

Classify the one legacy tombstone mismatch as expected; do not restore unsafe deletion merely to make it green. Checkpoint 2 reintroduces safe bottom-level removal using the watermark, after which `cargo x scheck` must pass again.

## Checkpoint 2: Hold a Stable Snapshot and Reclaim Safely

Ask:

> Add fixed-timestamp transactions, recovery of the timestamp oracle, and watermark-based garbage collection.

Use [Snapshot Read - Transaction API](./week3-03-snapshot-read-part-2.md) and [Watermark and Garbage Collection](./week3-04-watermark.md) as references.

Work out these rules from concrete version streams and reader lifetimes:

| Behavior to work out | Small case that exposes it |
| --- | --- |
| Snapshot selection | Read `a@7=delete, a@5=v5, a@2=v2` at timestamps 8, 6, 5, and 1. |
| Stable transaction timestamp | Read, commit a newer batch elsewhere, then read again in the original transaction. |
| Iterator ownership | Drop the transaction handle while its scan iterator remains alive. |
| Reader counts | Hold readers at timestamps 3, 3, and 7, then drop them one at a time. |
| Version garbage collection | Compact `k@8`, `k@5`, and `k@2` with watermark 5. |
| Bottom-level tombstone removal | Compare the same tombstone in a middle-level and bottom-level task. |
| Timestamp recovery | Reopen once with the maximum timestamp in an SST and once in a live WAL-backed memtable. |

A transaction records `read_ts = latest_commit_ts` and registers it with the watermark before compaction can race past it. For each user key, the iterator skips versions above `read_ts`, chooses the first remaining version, suppresses it if it is a tombstone, and then skips the rest of that user's versions. `TxnIterator` owns the transaction so a partially consumed scan keeps its snapshot registered.

The watermark stores a reader count for each timestamp, not merely a set of timestamps. Dropping one of two readers at timestamp 3 must therefore leave timestamp 3 registered. The starter's diagnostic `num_retained_snapshots()` reports the number of distinct timestamp buckets (`readers.len()`), not the sum of those counts; use the watermark movement, not that diagnostic alone, to prove duplicate-reader correctness. Compaction retains every version above the watermark and the newest version at or below it. It may remove that selected version when it is a tombstone only if the task reaches the bottom and no older value can survive elsewhere.

Store each SST's maximum timestamp and initialize the timestamp oracle from every live SST and recovered WAL-backed memtable. The first post-recovery batch must receive a strictly larger timestamp. This prevents reuse among recoverable versions; if a production design must never reuse a timestamp after all evidence of it has been garbage-collected, it needs an additional durable timestamp high-water mark.

This checkpoint necessarily evolves public interfaces: starting a transaction must return a transaction handle, and both engine scan methods must return an iterator that owns its transaction. These are **Course rule** changes, not compiler-directed mechanics. The complete required signature set is:

```text
MiniLsm:
pub fn new_txn(&self) -> Result<Arc<crate::mvcc::txn::Transaction>>
pub fn scan(&self, lower: Bound<&[u8]>, upper: Bound<&[u8]>)
    -> Result<crate::mvcc::txn::TxnIterator>

LsmStorageInner:
pub fn new_txn(self: &Arc<Self>) -> Result<Arc<crate::mvcc::txn::Transaction>>
pub fn scan(self: &Arc<Self>, lower: Bound<&[u8]>, upper: Bound<&[u8]>)
    -> Result<crate::mvcc::txn::TxnIterator>
```

Treat this block as the approval list: have the agent cite the copied tests and starter interfaces, preview all four signatures together, and obtain slice authorization for the complete list before editing any of them. If the proposed diff changes another public signature, stop and request separate approval instead of classifying it as a mechanical follow-on.

The starter already gives `Transaction::get`, `scan`, and `commit` a `Result` return type, while `put` and `delete` return `()`. Do not silently change the latter two merely to unify use-after-commit handling; that is a separate public-API choice and the supplied tests call them as non-fallible statements.

The starter also already uses the final `TxnIterator` shape that merges a local iterator with the engine iterator. Before Day 5, preserve that shape by making the local side an empty, validly constructed stream. Do not simplify the struct for Day 3 and then churn it back two slices later.

Implement snapshot selection, transaction ownership, and timestamp recovery as one coherent first pass. Once that slice compiles without supplied tests, reveal Week 3 test module Day 3 and run its focused gate:

```shell
cargo check -p mini-lsm-starter --lib
cargo x copy-test --week 3 --day 3
cargo test -p mini-lsm-starter --lib week3_day3
```

After Day 3 passes, implement watermark reader counts and compaction garbage collection as another independent first pass. Compile before revealing Week 3 test module Day 4:

```shell
cargo check -p mini-lsm-starter --lib
cargo x copy-test --week 3 --day 4
cargo test -p mini-lsm-starter --lib week3_day4
```

Before approving the checkpoint, keep old transactions alive across overwrite, deletion, flush, and compaction. Compare their `get` and bounded `scan` results before and after each transition. Separately, close and reopen databases whose largest timestamp is first in an SST and then in a live WAL-backed memtable; confirm the next batch receives a larger timestamp. Have the agent identify the ownership edge that keeps an abandoned scan's timestamp live and ask what compaction could reclaim if that edge disappeared.

## Checkpoint 3: Publish One Transaction Atomically

Ask:

> Add a private transaction workspace, read-your-writes, and one atomic commit path.

This checkpoint covers [Transaction Workspace and Atomic Commit](./week3-05-txn-occ.md). Keep the guarantee ledger visible: local isolation, atomic visibility, crash atomicity, and durability are different claims.

The agent should help you derive:

| Behavior to work out | Small case that exposes it |
| --- | --- |
| Local precedence | Overlay `a=new, b=delete, c=new` on snapshot `a=old, b=old`. |
| Local visibility | Read the workspace from its owner, another new transaction, and an older transaction. |
| Use after commit | Call every transaction operation after the first commit attempt. |
| Atomic visibility | Give all records one timestamp, pause after one memtable insertion, and withhold timestamp publication. |
| Crash atomicity | Stop after each prefix of one framed WAL batch. |
| Publication order | Fail WAL append, pause after the first memtable insertion, fail freeze maintenance, and fail `sync` separately. |
| Oversized commit | Commit a batch larger than the memtable target. |

The private skiplist is the newest source for its owning transaction. Local tombstones hide both local and engine values. Mark a transaction committed before publishing its writes so repeated `get`, `scan`, `put`, `delete`, or `commit` calls are all rejected under their existing contracts. Methods returning `Result` can return an error; the starter's non-fallible `put` and `delete` need a deterministic rejection such as an assertion unless the student explicitly approves changing their public signatures. Consistency here means no operation continues using the transaction, not that every method must acquire the same return type.

Submit the collected workspace through the existing batch path. WAL append precedes memtable visibility. Publish the commit timestamp after the batch is accepted and before fallible freeze maintenance, so a maintenance error cannot cause timestamp reuse. Check the memtable size only after the complete batch is in one memtable and WAL.

The skiplist insertion path has no provided failpoint or returned insertion error. Treat the mid-insertion case as a controlled pause: a concurrent transaction must still receive the old published timestamp and see none of the new batch. If the process stops there, recovery may replay the complete frame or lose it before `sync`, but it must never expose a prefix. Treat WAL, freeze, and `sync` failures as explicit traces unless the local code already offers a narrow failpoint; do not invent a new public failure API merely for this checkpoint.

Calibrate each claim independently:

- one shared timestamp, complete memtable installation, and delayed publication of `latest_commit_ts` together give **atomic visibility**; a shared timestamp alone is insufficient if a reader can select it mid-installation;
- one validated WAL frame gives **crash atomicity** during recovery;
- successful `sync` establishes the documented **durability** boundary; and
- none of these performs conflict validation or prevents write skew.

Complete the private workspace, iterator merge, rejection, batch commit, and framed-WAL recovery as one coherent first pass. Once it compiles without the supplied module, reveal Week 3 test module Day 5 and run its focused gate:

```shell
cargo check -p mini-lsm-starter --lib
cargo x copy-test --week 3 --day 5
cargo test -p mini-lsm-starter --lib week3_day5
```

Before approving the checkpoint, predict what the caller and recovery may observe at each failure point. Run a torn-batch case that would expose a prefix if recovery mutated the memtable before validating the frame. After focused tests pass, have the agent explain the exact line separating WAL acceptance from memtable publication.

## Checkpoint 4: Validate Dependencies and Filter History

Ask:

> Add point-key serializable validation, then integrate compaction filters with their undefined-read contract.

Use [Serializable Validation](./week3-06-serializable.md) and [Compaction Filters](./week3-07-compaction-filter.md) as references.

First work out commit-time validation:

| Behavior to work out | Small case that exposes it |
| --- | --- |
| Write skew | T1 reads `b` and writes `a`; T2 reads `a` and writes `b`. |
| Missing-key dependency | T1 reads missing `x`; T2 inserts `x`; T1 tries to commit a dependent write. |
| Blind writes | Two transactions write the same key without reading it. |
| Validation race | Release `commit_lock` after checking but before publishing. |
| Read-only transaction | Commit an empty workspace without allocating a timestamp or retaining metadata. |
| Metadata reclamation | Advance the watermark beyond old committed write sets. |
| Range phantom | Scan an empty interval while another transaction inserts into its gap. |

For a writing transaction, hold `commit_lock` across validation, timestamp allocation, publication, and insertion of its committed write set. Compare the current read set with write sets committed after its `read_ts`. Record point-read misses because absence can influence a decision. A failed validation publishes neither data nor metadata.

This is conservative point-key validation, not the full Serializable Snapshot Isolation algorithm. It can reject some serializable histories, and scans record returned-key hashes rather than predicates or gaps. The empty-range insertion above can therefore pass. Treat that limitation as a required explanation, not a hidden test gap or permission to claim full serializability.

Implement commit-time point-key validation and metadata reclamation as one coherent first pass. Compile before revealing Week 3 test module Day 6:

```shell
cargo check -p mini-lsm-starter --lib
cargo x copy-test --week 3 --day 6
cargo test -p mini-lsm-starter --lib week3_day6
```

Then add compaction filters as another independent first pass. Versions above the watermark remain untouched even when their user key matches. When the first matching version at or below the watermark is selected, omit it and every older version of that user key from the task's output. This deletion policy intentionally overrides ordinary snapshot visibility inside the filtered prefix: even an existing transaction may lose a version at or below the watermark. Such reads remain undefined, and older matching versions can also survive outside the current task.

Once the filter slice compiles without its supplied tests, reveal Week 3 test module Day 7 and run its focused gate:

```shell
cargo check -p mini-lsm-starter --lib
cargo x copy-test --week 3 --day 7
cargo test -p mini-lsm-starter --lib week3_day7
```

Before approving the checkpoint, reproduce the point-read write-skew abort and the scan-phantom limitation. Name why one is prevented and the other is not. Then, with watermark 5, filter `k@8, k@5, k@2`: predict that `k@8` survives while `k@5` and `k@2` may disappear, and explain why a read at timestamp 5 is outside the contract. Advance the watermark to 8, compact again, and trace the remaining history.

## Audit the Finished MVCC Engine

Run the complete project check from the repository root:

```shell
cargo x scheck
```

Then ask the agent for a final evidence report containing:

1. the combined decision and guarantee ledgers, including delegated choices;
2. the exact commands and outcomes;
3. one internal-key byte layout and user-range-to-internal-range translation;
4. one key traced through overwrite, tombstone, an old live snapshot, flush, and compaction, plus a separate timestamp-oracle trace through restart;
5. one transaction traced through local reads, WAL acceptance, memtable publication, timestamp publication, `sync`, and later visibility;
6. one write-skew dependency that aborts and one scan phantom the implementation still permits;
7. the versions retained at a selected watermark, with and without a matching compaction filter; and
8. one weakness not established by the supplied tests.

Inspect the actual diff and output. Check for modified supplied tests, removed assertions, broad lint suppressions, unsafe lifetime conversions that can escape a synchronous lookup, timestamp allocation outside its lock, WAL recovery that mutates state before validating a complete batch, and claims of full serializability based only on returned scan keys.

Finally, introduce and immediately revert one small fault whose guarantee you can name. Good choices include sorting timestamps ascending, dropping a scan's transaction owner, retaining only versions above the watermark, publishing the memtable before WAL append, releasing `commit_lock` between validation and publication, or filtering versions above the watermark. Predict the failing test or trace before running it.

## Day 3 Completion Checkpoint

You have completed the three-day track when you can do these without delegating the explanation back to the agent:

- order and encode several timestamped internal keys and translate user range bounds;
- select visible values and tombstones for multiple read timestamps;
- explain why an iterator owns its transaction and how the watermark protects one historical version;
- distinguish atomic visibility, crash atomicity, durability, snapshot visibility, and serializability;
- place failures through commit and recovery and state exactly what may be visible or durable at each point;
- construct a point-read write skew that validation rejects and a range phantom it misses;
- explain why compaction and filters preserve versions above the watermark; and
- turn a suspected invariant violation into a minimal test or execution trace.

If one item is unclear, return to the corresponding Week 3 chapter and ask the agent for a smaller version stream, crash point, or transaction interleaving. Finishing the code is not enough: you should be able to name the guarantee, find its mechanism, and design evidence that could prove your shared implementation wrong.

{{#include copyright.md}}
