# Week 3 Student–Apprentice Walkthrough

## Setup

### Student

Prepare a fresh isolated starter workspace from the pinned Week 2 baseline. Run the course-authorized Day 1 preparation gate, record its raw initial failure, perform only the permitted timestamped-key module copy, and then begin the Week 3 design dialogue from the learner-visible fast-forward chapter.

### Apprentice

The isolated starter workspace is clean at the pinned baseline. The earlier-week suite passes, the expected Day 1 pre-implementation failure is recorded, and the permitted key-module copy is complete. I will use only learner-visible material, ask one design question at a time, and wait for authorization before each implementation slice.

## Walkthrough

### Student

**Day 3 opening prompt** (restarted from the beginning, verbatim from `week3-fast-forward.md`; the walkthrough starts from a fresh context):

> Build Day 3 with me, starting with timestamped internal keys and batches. Follow the student-owned design protocol in `AGENTS.md`. Never access `../mini-lsm` or `../mini-lsm-mvcc`; only the exact key and test copy commands documented for Week 3 may read from those sources. Maintain both a decision ledger and the Day 3 guarantee ledger. Ask one short question at a time using a concrete version stream, snapshot, crash point, or transaction interleaving. Name the guarantee being tested, then mark the decision **Course rule** or **Your choice**. I may reply `simpler`, `example`, `hint`, or `choose for me`. Do not edit until my answers specify one small, coherent slice. After each slice, connect one important line to its guarantee and ask what would break if it changed.

Apprentice — confirm your working directory is `mini-lsm-starter`-scoped (handshake), then begin Checkpoint 1 with the first one-question design step. The first question should order a few timestamped keys. No edits until my answers specify the first coherent slice.

### Apprentice

Handshake: `pwd` confirms I am in `<isolated-worktree>/mini-lsm-starter`; I’ll return to the repository root only for exact course-authorized copy/test commands.

**Guarantee: Version order — Course rule.**

If the engine contains `a@9`, `a@4`, `aa@7`, and `b@2`, what order should an iterator return them in, and why should the two `a` versions be in that order for a snapshot read?

You can reply `simpler`, `example`, `hint`, or `choose for me`.

### Student

**My answer (Course rule — version order):** the iterator should return `a@9, a@4, aa@7, b@2`.

Reasoning: sort by user-key bytes ascending first (`a` < `aa` < `b`), and within the same user key sort by timestamp **descending** (`a@9` before `a@4`). The descending timestamp matters for snapshot reads: a read at timestamp `ts` walks internal keys in order and takes the *first* version of each user key whose timestamp is ≤ `ts` — which is by construction the newest version that snapshot may legally see — then skips all remaining versions of that user key. Ascending timestamps would surface the oldest version first and could hand a snapshot a stale value.

### Apprentice

Accepted. The learner-visible chapter specifies that exact order, and your snapshot explanation is correct.

Decision ledger update: `internal key order | user key ascending, then timestamp descending | Course rule from Week 3 Checkpoint 1 | first version at or below read timestamp is the newest visible version`.

Guarantee ledger update: `Version order | internal comparator uses that order | derived design decision | Checkpoint 1 (in progress)`.

**Guarantee: Version order — Course rule.**

For two adjacent encoded entries `cat@9` and `cat@4`, which bytes may the second entry reuse from the first, and which part must it store again? Why would reusing the timestamp as though it were part of the shared prefix be unsafe?

You can reply `simpler`, `example`, `hint`, or `choose for me`.

### Student

**My answer (Course rule — prefix compression):** for adjacent `cat@9` then `cat@4`, the second entry may reuse only the shared **user-key** bytes (`c`, `a`, `t`) from the first, and it must store its own **full 8-byte timestamp** again.

Why not reuse the timestamp as part of the shared prefix: prefix compression exists to reconstruct each entry **independently** from its own stored bytes plus the entry's own prefix. A timestamp borrowed from the previous entry would make entry 2's key depend on entry 1's bytes — decoding entry 2 would require entry 1, so an iterator could no longer seek straight to an entry (and block summaries/bounds would break). It would also be semantically wrong: the two entries have different timestamps by design (timestamp-descending order puts `cat@9` before `cat@4`), so the shared prefix must stop at the user-key boundary; the timestamp is the differing part and must be stored in full, exactly as the course rule states: "Prefix compression applies only to user-key bytes; each timestamp is stored in full."

### Apprentice

Accepted. The chapter confirms prefix compression is only over user-key bytes and each timestamp is stored in full. I also inspected the relevant permitted starter interfaces (`src/block`, `src/table`, and `src/key`); block metadata currently holds first/last keys and block/table iterators compare `KeySlice`, so this slice must preserve full internal-key ordering at those boundaries.

Decision ledger update: `prefix compression | reuse user-key prefix only; encode each timestamp in full | Course rule | each internal key reconstructs independently and keeps version identity`.

**Guarantee: Version order — Course rule.**

Imagine one SST block ends with `a@4` and the next begins with `aa@7`. Should that boundary metadata record just the user keys (`a`, `aa`) or the complete internal keys (`a@4`, `aa@7`)? Explain what a seek for `a@5` would need the metadata to distinguish.

### Student

**My answer (Course rule — SST summaries):** the block metadata must record the **complete internal keys** — block 1's last key `a@4` and block 2's first key `aa@7` — not just the user keys.

What a seek for `a@5` needs: internal ordering says `a@5` compares **greater than** `a@4` (same user key, lower timestamp) but **less than** `aa@7` (different, larger user key). So `a@5` sorts *after* block 1's last entry and *before* block 2's first entry; the first internal key ≥ `a@5` is `aa@7`, i.e., the seek must land at the start of block 2.

With user-key-only metadata ("last = `a`", "first = `aa`"), a seek for `a@5` sees "user key `a` is covered by block 1" and targets block 1 — but block 1's newest entry is `a@4`, which is *older* than `a@5`; the seek would return `a@4` (wrong: it's `< a@5`) or need an extra fallback hop to discover the right block. Recording the full internal keys makes the boundary decision exact: `a@5 > a@4` excludes block 1, `a@5 ≤ aa@7` selects block 2. So the timestamp belongs in the boundary metadata precisely because seeks can fall *between versions of one user key*.

### Apprentice

Your main conclusion is right: the metadata must store complete internal keys. One correction to the comparison trace: because timestamps sort descending, `a@5` sorts **before** `a@4`, not after it. The starter’s copied `Key::cmp` confirms this by comparing `(user_key, Reverse(timestamp))`; its current `find_block_idx` tests `last_key < target`.

So for `a@5`, a block ending at `a@4` is still the candidate block, and a lower-bound seek can correctly return `a@4` as the first key ≥ `a@5`. Complete internal metadata remains required so seeks and block boundaries faithfully use the engine’s internal ordering, including versions of the same user key. Please restate the corrected ordering relationship in one sentence; then we’ll record this decision.

### Student

**Corrected restatement (one sentence):** for the same user key, timestamps sort **descending**, so `a@5` sorts *before* `a@4` in internal order — which means a seek for `a@5` still treats a block ending at `a@4` as the candidate block, and its lower-bound result is `a@4` (the first key ≥ `a@5` when `a@5` itself is absent).

Good catch — I had the comparison backwards. The decision still stands (complete internal keys in block summaries), but for the right reason: block selection depends on the version-aware internal comparator, not on the user key alone.

### Apprentice

Accepted. That corrected trace matches the copied key comparator and `find_block_idx` boundary rule.

Decision ledger update: `SST/block summaries | store complete internal first/last keys | Course rule; seek uses version-aware internal comparator | block selection remains exact across versions`.

**Guarantee: Version order — Course rule.**

An SST contains `k@8` and `k@3`. Should its Bloom filter insert two different fingerprints (one per internal key) or the same user-key fingerprint for both? A latest-state point lookup asks for user key `k`; explain which choice lets the filter safely reject impossible SST probes without falsely rejecting `k`.

### Student

**My answer (Course rule — Bloom-filter identity):** the Bloom filter must insert the **same user-key fingerprint** for both `k@8` and `k@3` (hash only the user-key bytes, not the internal key).

Reasoning: a point lookup asks "might any version of user key `k` be in this SST?" — and a Bloom filter is only safe to *reject* on a negative (no false negatives allowed; positives may be false). If the filter had stored per-internal-key fingerprints (`k@8`, `k@3`) but the lookup hashes just user key `k`, the lookup's fingerprint would match neither stored one and the filter would **falsely reject** an SST that actually contains `k` — a false negative, which is the one thing a Bloom filter must never do. Storing the user-key fingerprint for every version makes any present version produce a positive; the lookup then probes the SST (costly but correct), and absent user keys are rejected safely. That's exactly the course rule: "Bloom filters hash only user-key bytes because a lookup asks whether any version of that user key may exist."

### Apprentice

Accepted. This is the required Bloom identity rule: false positives cost work, but a false negative would lose a real version.

Decision ledger update: `Bloom identity | hash only user-key bytes | Course rule; point lookups ask whether any version exists | safe negative SST rejection`.

**Guarantee: Version order — Course rule.**

A user scan has bounds `(a, b]`: it must exclude every version of `a` and include every version of `b`. Using the provided internal timestamp extremes, what internal lower and upper keys should represent those bounds? Explain why the lower bound cannot start at `a@0` and the upper bound cannot end at `b@0`.

### Student

**My answer (Course rule — user-range mapping):** with the comparator `(user asc, Reverse(ts))` and the provided extremes (`TS_RANGE_BEGIN = u64::MAX`, `TS_RANGE_END = 0`), the scan bounds `(a, b]` map to internal keys:

- lower = **`Excluded(a @ TS_RANGE_END)`** — i.e., `Excluded(a@0)`
- upper = **`Included(b @ TS_RANGE_END)`** — i.e., `Included(b@0)`

Reasoning: `a@0` is the *last* (largest) internal key of user key `a`, so "keys strictly greater than `a@0`" is exactly "all versions of user keys > `a`" — every version of `a` is excluded, nothing leaks. Symmetrically, `b@0` is the last internal key of user key `b`, so "keys ≤ `b@0`" covers **every** version of `b` (from `b@MAX` down to `b@0`).

- Why the lower cannot **start** at `a@0`: `a@0` is itself a real version of user key `a`. If the lower bound were `Included(a@0)`, the scan would start *at* it and return a version of `a` — leaking the excluded user key. It is only safe as the excluded bound key, which places the scan strictly past the end of `a`.
- Why the upper cannot **end** at `b@0` (exclusively): `b@0` is the last version of `b`; an `Excluded(b@0)` upper bound would stop before it and silently drop one version of an included user key. The bound must be inclusive at `b@0` to retain every version of `b`.

(Using `TS_RANGE_BEGIN` on the upper, e.g. `Included(b@MAX)`, would stop at `b`'s *first* version and miss all older versions — the opposite error.)

### Apprentice

Accepted. The lower-bound wording needed one precision: `a@0` is exactly the right **excluded** lower boundary, and `b@0` is exactly the right **included** upper boundary. Your derivation correctly excludes every `a` version and retains every `b` version.

Decision ledger update: `user range (a, b] | Excluded(a@0) to Included(b@0) | Course rule from internal ordering | no a-version leak; no b-version loss`.

The first coherent slice is now specified: update the permitted starter’s block/SST key encoding and decoding so timestamps remain full per entry; preserve complete internal first/last keys in block metadata and table boundaries; and hash only user-key bytes for Bloom filters. Expected files are within `mini-lsm-starter/src/key.rs`, `src/block/{builder,iterator}.rs`, `src/table/{builder,iterator,bloom}.rs`, and possibly `src/table.rs`; no test or public-API edits.

The already course-revealed `week3_day1` module will be used only after this slice is independently implemented and compiles. May I make this one slice now?

### Student

**Authorization — slice 1 (Checkpoint 1, default slice order #1):** yes, make this slice.

Scope confirmed: block/SST key encoding/decoding keeps each timestamp full per entry; block metadata and table boundaries preserve complete internal first/last keys; Bloom filters hash user-key bytes only. Files as previewed (`src/key.rs` already swapped, `src/block/*`, `src/table/*`, `src/table.rs` as needed); no test or public-API edits.

Protocol for this slice: complete the independent first pass (compile without the supplied tests), then reveal/run the focused check — `week3_day1` is already copied from the preparation step, so run the narrowest relevant command (e.g. `cargo test -p mini-lsm-starter week3_day1`) and show the exact result. If a product-behavior test fails, explain what it sets up and what it expects, then ask me to diagnose — do not trace/fix on your own. After the slice, per `AGENTS.md`: point to one important changed line and ask what it does and what would break if it changed.

### Apprentice

Slice-1 independent compile result: `cargo fmt --check` first reported one formatting-only change, which I applied with `cargo fmt`. `cargo check -p mini-lsm-starter` then failed before tests with 30 compiler errors caused by the provided timestamp-aware `Key` interface replacing the old key-only helpers: stale `raw_ref`, key-only `from_slice`, and metadata encode/decode calls no longer type-check.

This is not a supplied-test product failure. I will make only compiler-directed compatibility updates needed for the authorized block/SST slice: preserve timestamps in metadata, use user-key access only where the current Day-2 public operation still needs it, and use the provided default timestamp for untouched pre-MVCC paths. I will then rerun the independent compile before any Day-1 test.

### Student

**Review answer (Guarantee: Version order):** `self.data.put_u64(key.ts())` appends the entry's **full 8-byte timestamp** to the encoded block data — for a second entry like `cat@4` it writes `4` in full after that entry's key-suffix bytes, so the timestamp is stored per-entry, not prefix-compressed from `cat@9`.

If that line were removed while the decoder still expects a `u64` timestamp per entry, the decoder would misparse: it would read the first 8 bytes of the *value* (or the next entry's data) as `cat@4`'s timestamp. Concretely that corrupts the reconstructed internal key (wrong timestamp, so `cat@4` would look like some other version), and because the timestamp length shifts the remaining parse (value length, next key's suffix), every subsequent entry in the block also decodes incorrectly — the block iterator would return wrong keys/values or garbage, version ordering breaks, and a latest-state read would pick the wrong version or fail outright. The multi-version tests exercise exactly this: they compare reconstructed internal keys like `key00000@5`, which only parse correctly when each timestamp is stored in full.

### Apprentice

Accepted. That is exactly the encoding/decoding dependency: the timestamp is part of every reconstructed internal key, and removing its bytes would misalign the rest of the block.

Slice-1 handoff ledger:

```text
Decision | Student's choice | Invariant/evidence | Consequence
internal key order | user asc, timestamp desc | Week 3 course rule | first eligible version is newest visible
prefix compression | user-key bytes only; full timestamp per entry | encoded block layout + focused tests | independent reconstruction
SST summaries | complete first/last internal keys | comparator and block selection | exact version-aware seeks
Bloom identity | hash user-key bytes only | point lookups ask about any version | no false-negative SST rejection
user range (a, b] | Excluded(a@0) .. Included(b@0) | internal ordering | excludes all a, includes all b
```

**Guarantee: Version order — Course rule.**

Before batch timestamps are introduced, the course calls for finishing the timestamp-zero engine refactor. If the existing Day-2 write path stores one `put(k, v)` now, which timestamp should its internal key use, and why is a fixed zero the safe transitional rule rather than inventing a new timestamp per write before the batch/commit design is authorized?

### Student

**My answer (Course rule — timestamp-zero transitional writes):** the Day-2 write path should store the internal key with a **fixed timestamp of zero** (`TS_DEFAULT`/`TS_RANGE_END`).

Why zero is the safe transitional rule rather than inventing a per-write timestamp:
1. **Timestamps encode commit order.** Version order says "higher timestamp = newer = comes first." Before the batch/commit design exists, there is no correct, synchronized source of timestamps — allocating per-write timestamps without `write_lock`/`latest_commit_ts` lets two concurrent writers obtain timestamps whose order contradicts their actual write order, so version order would **lie about recency** and a reader could pick the wrong version.
2. **A fixed zero is order-consistent.** Every transitional write has the same timestamp, so version ordering can never contradict write order; the visible value is still decided by the existing Day-2 precedence (memtable overwrite, and merge-source order across memtables), which is exactly the semantics we're preserving while the representation becomes timestamp-aware.
3. **It is throwaway by design.** The real allocation arrives with the batch slice (one timestamp per committed batch, under `write_lock`, publishing `latest_commit_ts`). Any ad-hoc per-write allocation now would be replaced then — and worse, it could mask ordering bugs in the meantime. Zero keeps the transitional engine identical in behavior to Day 2 so regressions are attributable.

### Apprentice

Accepted. The fixed zero preserves Day-2 behavior while making the internal-key representation timestamp-aware; timestamp allocation remains reserved for the later, separately authorized batch slice.

The compiler-directed compatibility work from slice 1 already establishes this transitional behavior: the old key-only constructor now produces `TS_DEFAULT`, and the untouched memtable/lookup paths use it. The focused Day-1 tests passed with that behavior, so there is no additional implementation diff to make for default slice 2.

Decision ledger update: `pre-batch write timestamp | fixed TS_DEFAULT (0) | Course rule; no commit-time allocator exists yet | preserves Day-2 behavior until batch allocation`.

May I record default slice 2 as complete on that passing evidence and proceed to the course-authorized Day-2 test copy, which is the next checkpoint gate?

### Student

**Authorization — record default slice 2 complete; proceed to the Day-2 test gate.**

Agreed: the timestamp-zero engine refactor is the slice-1 compatibility behavior (old call sites produce `TS_DEFAULT`), already verified by the focused Day-1 tests passing; no separate diff needed. Record it complete on that evidence.

Next, run the course-authorized copy from the repository root:

```shell
cargo x copy-test --week 3 --day 2
```

Then run the narrowest relevant test command and report the exact result. Expectation: the newly copied `week3_day2` module targets the timestamped-memtable/engine work that isn't implemented yet, so it should fail or fail to compile until slice 4 — record that as the current gate state (do not fix it yet). After the report, propose the slice-4 design questions (timestamped memtables, internal range bounds, latest-state iteration, final WAL framing, batch timestamp allocation, write path) one at a time.

### Apprentice

Default slice 2 is recorded complete. I ran the authorized root command `cargo x copy-test --week 3 --day 2`; it succeeded and added only the permitted `week3_day2` module/tests index entry.

Narrowest relevant check:

```text
cargo test -p mini-lsm-starter --lib week3_day
running 3 tests
week3_day1::test_sst_build_multi_version_simple ... ok
week3_day1::test_sst_build_multi_version_hard ... ok
week3_day2::test_timestamped_batches_and_latest_reads ... FAILED

assertion `left == right` failed: expected key: b"a"@2, actual key: b"a"@0
```

Concrete gate state: the Day-2 scenario expects the engine to expose the committed version `a@2`; the current transitional write path still produces `a@0`. This is a product-behavior failure, and I have made no diagnosis-driven patch or edit.

**Guarantee: Version order / atomic visibility — Course rule.** Which execution path or invariant do you think is missing between accepting a batch and reading `a`, such that the engine returns the zero-timestamp version instead of `a@2`? Please explain your hypothesis; then we’ll settle the first slice-4 decision before editing.

### Student

**My diagnosis (Guarantee: Version order / atomic visibility):** the missing path is **batch commit with timestamp allocation** — the write path has no way to stamp a committed write with anything but `TS_DEFAULT`.

Concretely, between "accept a batch" and "read `a`", these pieces are absent:
1. **No timestamp oracle / allocator:** there is no `latest_commit_ts` (or equivalent) and no rule that "one committed batch receives one new timestamp." Nothing ever produces the `2` the test expects; every write falls through to the transitional fixed `0`.
2. **No batch write path:** `write_batch`/`put_batch` are not wired; the Day-2 `put` path inserts the internal key with ts 0 directly into the memtable, so the internal key never carries a commit timestamp.
3. **No `write_lock` discipline:** even if timestamps existed, nothing serializes allocation + memtable insertion + `latest_commit_ts` publication, so concurrent writers could reuse or publish timestamps out of order.

So the invariant that's missing is: *each committed batch receives exactly one fresh timestamp, held under `write_lock` across allocation → WAL append → memtable insertion → publication of `latest_commit_ts`*, and the write path stamps every key in the batch with that timestamp. Until that exists, a read of `a` can only ever see `a@0`.

### Apprentice

Accepted. Your diagnosis matches the permitted starter interfaces: `write_batch` still calls single-record memtable writes, while the future batch helpers are intentionally unimplemented. The missing invariant is one fresh timestamp per committed batch, with the lock covering allocation, WAL acceptance, memtable installation, and timestamp publication.

Decision ledger update: `committed batch timestamp | allocate one fresh timestamp under write_lock | Course rule | all records in a committed batch share one visibility point`.

**Guarantee: Atomic visibility — Course rule.**

Two concurrent batches arrive: B1 writes `a` and `b`; B2 writes `c` and `d`. If B1 gets timestamp 1 and B2 gets timestamp 2, what timestamps should the four internal keys carry, and at what exact point may a new reader start seeing B1? Explain why publishing timestamp 1 after only `a` is installed would violate the guarantee.

### Student

**My answer (Course rule — atomic visibility / batch timestamps):**
- The four internal keys carry one timestamp per batch: `a@1` and `b@1` (B1), `c@2` and `d@2` (B2). A batch's records are never split across timestamps.
- A new reader may start seeing B1 only **after both `a@1` and `b@1` are installed in the memtable and `latest_commit_ts` is published (= 1)**. The publication of `latest_commit_ts` is the visibility point: readers pick their snapshot timestamp from it, and reads at `read_ts ≥ 1` must then find the complete batch.
- Publishing `1` after only `a` was installed would violate **atomic visibility**: a reader at `read_ts = 1` would see `a@1` (new) but `b` still at its older version (e.g. `b@0`), i.e. it would observe only part of one committed batch — a torn view of a single atomic commit. That's exactly why the order is: WAL append first, then install **every** memtable entry, and only then publish `latest_commit_ts`; the shared timestamp alone isn't enough if a reader can select it mid-installation.

### Apprentice

Accepted. The distinction is correct: one shared timestamp identifies the batch, but publication only after every entry is installed provides atomic visibility.

Guarantee ledger update: `Atomic visibility | one timestamp per batch; install all entries before publishing latest_commit_ts under write_lock | design accepted | a reader never sees a committed-batch prefix`.

**Guarantee: Crash atomicity — Course rule.**

A WAL batch contains `a@1` and `b@1`. The process stops after writing the length header, halfway through the body, or after the body but before its checksum. In each case, should recovery replay any record from that batch? What must recovery validate before it inserts even the first entry into the memtable?

### Student

**My answer (Course rule — crash atomicity / WAL framing):** in **all three** stop points — after the length header only, halfway through the body, or after the body but before its checksum — recovery must replay **nothing** from that batch (drop the incomplete frame).

Before inserting even the first entry, recovery must validate the **complete frame**: (1) the declared `batch_body_len` plus the trailing `body_checksum` are fully present (the frame is not truncated at any point); (2) every `key_len:u16` and `value_len:u16` actually fits inside the remaining body and its field width — reject instead of truncating any cast; and (3) the `body_checksum` over the complete body matches. Only a fully validated frame may be replayed, and then all its entries are installed together.

This is exactly the course's boundary: recovery may replay a **complete persisted frame** or lose an **unsynchronized frame**, but it must never expose a **prefix** — a partial batch would be a torn write, breaking crash atomicity even though each individual record looks fine.

### Apprentice

Accepted. That gives the required crash-atomicity boundary: complete length-delimited body, complete checksum, and every nested length validated before recovery mutates the memtable.

Guarantee ledger update: `Crash atomicity | validate one complete WAL batch frame before replaying any entry | Course rule | recovery never exposes a torn batch prefix`.

**Guarantee: Latest-state visibility — Course rule.**

The internal stream for one user key is `k@9 = delete`, `k@7 = v7`, `k@3 = v3`. At the latest timestamp, should `get(k)` return absence or `v7`? Once it selects its answer, should the iterator keep scanning older versions of `k`? Explain why.

### Student

**My answer (Course rule — latest-state collapse):** at the latest timestamp, `get(k)` must return **absence**. The newest entry is `k@9 = delete` (a tombstone), and a tombstone determines the result — the search stops there and does not continue to older values.

Once the iterator selects its answer (the newest entry at or below the read timestamp), it must **not** keep scanning older versions of `k`, for two reasons:
1. **Correctness:** the first visible entry decides the outcome. Continuing to `k@7 = v7` or `k@3 = v3` would resurrect a deleted key (or let an older value override a newer one) — violating newest-value-wins precedence.
2. **Efficiency by construction:** the timestamp-descending internal order exists precisely so a read can stop at the first eligible version and skip the rest of that user key's history in one jump, instead of scanning every version. That's the collapse rule: select the newest visible entry; if it's a tombstone, the key is absent.

### Apprentice

Accepted. The tombstone is the selected newest state, so older values must not be resurrected. This completes the latest-state decision.

Decision ledger update: `latest-state collapse | select newest eligible version; selected tombstone means absent; skip older versions | Course rule | deletes do not resurrect older values`.

**Guarantee: Atomic visibility — Course rule.**

A single committed batch is larger than the target memtable size. Should the engine split it between the current memtable and a newly frozen one to stay under the target, or install the entire batch in one memtable and consider freezing afterward? Explain what a reader could observe if that batch were split across the boundary.

### Student

**My answer (Course rule — oversized batch):** install the **entire batch in one memtable** (even if it exceeds the target size), and consider freezing **afterward** — the memtable capacity is a soft limit, and a batch must never be split between memtables.

What a reader could observe if the batch were split across the boundary: the batch's entries share one timestamp and one publication point (`latest_commit_ts` published only after *every* entry is installed). If part of the batch went into the current memtable and the rest into a freshly frozen one, the two halves would be installed at different times and (worse) the freeze itself could race the reader's state snapshot — a reader could see the first half (e.g. `a@1` in the mutable memtable) while the second half (`b@1`) isn't in any visible source yet: a **torn view of one atomic batch**. It would also break crash atomicity, since the batch must be one complete WAL frame in one memtable's WAL; splitting it across two WALs makes a single validated frame impossible. So: one batch → one memtable → one WAL frame → one publication.

### Apprentice

Accepted. The target size is a post-batch maintenance threshold, never a reason to split a logical commit.

Decision ledger update: `oversized batch | install whole batch in one memtable, then consider freeze | Course rule | capacity maintenance cannot tear a commit`.

The next coherent slice is now specified: implement timestamped memtable keys and internal range mapping; collapse latest-state `get`/`scan`; introduce the final length-delimited, checksummed WAL batch grammar and complete-frame recovery; allocate one timestamp per batch under `write_lock`; append the WAL batch, install all memtable entries, then publish `latest_commit_ts`; and only then consider freezing. Expected changes are limited to starter `src/{key,mem_table,wal,lsm_storage,lsm_iterator}.rs` plus necessary internal iterator/table callers. No test, harness, public-interface, dependency, or workspace changes.

After its independent compile pass, I will run the already-revealed focused Week-3 Day-1/Day-2 test command. May I implement this slice?

### Student

**Authorization — slice 4 (Checkpoint 1, default slice order #4):** yes, implement this slice.

Scope confirmed: timestamped memtable keys + internal range mapping; latest-state `get`/`scan` collapse; final length-delimited checksummed WAL batch grammar with complete-frame recovery; one timestamp per batch under `write_lock`; order = WAL append → install all memtable entries → publish `latest_commit_ts` → then consider freeze. Files as listed; no test/harness/public-interface/dependency/workspace changes.

Protocol: complete the independent compile pass first, then run the already-revealed focused check:

```shell
cargo test -p mini-lsm-starter --lib week3_day
```

Expect `week3_day1` (2 tests) and `week3_day2` (1 test) to pass. If a *legacy* Week 1/2 tombstone/compaction test fails, classify it as the known expected mismatch (per the fast-forward: Checkpoint 1 deliberately retains bottom-level tombstones; safe removal returns in Checkpoint 2 with the watermark) — do not restore unsafe deletion to make it green. Any product-behavior failure in the week3_day tests: explain setup/expected/observed, then ask me to diagnose. After the slice, point to one important changed line and ask what it does and what would break if it changed.

### Apprentice

Slice-4 implementation is underway. I read the learner-visible `week3-02-snapshot-read-part-1.md` and confirmed the concrete starter transition: `SkipMap<Bytes, Bytes>` must become `SkipMap<KeyBytes, Bytes>`; memtable/WAL APIs carry `KeySlice`; `write_batch` allocates `latest_commit_ts + 1` under the existing MVCC write lock; and `LsmIterator` collapses repeated user keys. No additional source or test material was read, and no new edit has been made since authorization.

### Apprentice

First slice-4 compile attempt is currently blocked at compiler-directed migration work, not a supplied-test result: changing the memtable to `KeyBytes` exposed 16 stale type boundaries in the permitted starter (`LsmStorageInner` still passes user-byte bounds/keys, WAL recovery still builds a byte-key map, and the self-referential memtable iterator still declares byte-key storage). I’m continuing the mechanical API migration within the authorized slice; no test run or diagnosis-driven behavior patch has occurred.

### Apprentice

The focused Week 3 gate found one product-behavior failure, so I stopped before diagnosing or changing it.

`cargo check -p mini-lsm-starter` passes. `cargo test -p mini-lsm-starter --lib week3_day` passes 5/6; `test_task2_lsm_iterator_mvcc` fails its final bounded scan: after snapshot6, `(a, c)` should be empty but the iterator remains valid (the harness finds an extra key). The earlier snapshot/get/scan assertions and SST max-timestamp test pass.

Guarantee: snapshot range visibility. Which path/invariant do you think is wrong—the conversion of the excluded user bounds into internal-key bounds, or the iterator’s later range check? Once you choose/authorize the diagnosis, I’ll make one repair.

### Student

**My diagnosis (snapshot range visibility):** the excluded-bound → internal-key **conversion** is the primary suspect — specifically the timestamp extreme used on an excluded endpoint.

The failure signature is an *empty* expected range `(a, c)` that yields one extra key — i.e., one of the excluded endpoints (`a` or `c`) is leaking its versions into the internal range. The correct mapping (from our checkpoint-1 exercise): excluded **lower** bound → `Excluded(user @ TS_RANGE_END=0)` (start strictly after the user key's last version); excluded **upper** bound → `Excluded(user @ TS_RANGE_BEGIN=MAX)` (stop strictly before the user key's first version). A swap — e.g. using `TS_RANGE_BEGIN` on the excluded lower (leaks `a@MAX-1…a@0`) or `TS_RANGE_END` on the excluded upper (leaks `c@MAX…c@1`) — includes a boundary user key that should be excluded, exactly matching "iterator remains valid with an extra key."

**How to verify before fixing:** trace the internal bounds produced for `(a, c)` and check which endpoint leaks; the leaked user key (a vs c) will match the extra key the harness found. If the conversion is actually correct, then the fallback suspect is a missing user-bound filter in the iterator's range check — but I'd bet on the conversion first.

### Apprentice

Your mapping is correct and accepted. I checked the authorized diagnosis against the current slice: the memtable conversion already uses excluded-lower → `Excluded(a@0)` and excluded-upper → `Excluded(c@MAX)`, but the SST seek path still starts any lower endpoint at `@MAX` and does not apply that converted bound. That can leak `a` for `(a, c)` after a flush.

Please authorize one minimal repair: make the SST/level seek use the same internal lower-bound mapping (and retain the existing user-level end check), then rerun the focused gate.

### Student

**Authorized — one minimal repair:** make the SST/level seek apply the same internal lower-bound mapping as the memtable (excluded lower → `Excluded(user@0)`; included lower → `Included(user@MAX)`), retaining the existing user-level end check. Then rerun `cargo test -p mini-lsm-starter --lib week3_day` and post the exact result.

### Apprentice

Repair complete: SST and level iterators now seek excluded lower bounds from `user@0`, skipping that exact endpoint when present. This keeps `(a, c)` from returning either boundary after a flush.

`cargo test -p mini-lsm-starter --lib week3_day` now passes: 6 passed, 0 failed (71 filtered out).

Slice review: the seek now uses `TS_RANGE_END` for an excluded lower user bound. What is that line trying to do, and what behavior would break if it used `TS_RANGE_BEGIN` instead?

### Student

**Review answer (excluded-lower seek bound):** the line maps an excluded user key `user` to the internal bound `Excluded(user @ TS_RANGE_END=0)`. `user@0` is the *last* (largest) internal key of that user key, so "first key strictly greater than `user@0`" is exactly "the next user key's newest version" — the excluded user key's entire version history is skipped, with nothing leaking.

If it used `TS_RANGE_BEGIN` (`Excluded(user@MAX)`) instead, the seek would start at `user@(MAX-1)` — the bound would only skip the *first* version of the excluded user key and leak the rest of its history (`user@MAX-1 … user@0`) into the scan. For `(a, c)` that means `a`'s versions appear in the result — the exact boundary leak this repair was fixing. So the `TS_RANGE_END` choice is what makes "excluded" mean "no versions of this user key at all."

### Apprentice

Accepted. Your explanation correctly ties `@0` to the last internal version, so the strict bound advances to the next user key; `@MAX` would leak the excluded key’s older versions.

The next coherent slice is Checkpoint 2’s watermark tracking only: implement per-timestamp reader counts, register a transaction before it can race with compaction, remove its registration only when its final `Arc` owner (including a scan iterator) drops. No compaction-GC yet. The Day 4 tests will be copied only after that independent implementation compiles. Authorize this slice when ready.

### Student

**Authorization — Checkpoint 2, slice 2 (watermark tracking), with one ordering correction.** The fast-forward explicitly sequences this slice as: "After snapshot reads and timestamp recovery pass their focused checks, copy the Day 4 test module, then implement the watermark and compaction-GC slice." So:

1. Run the course-authorized root command `cargo x copy-test --week 3 --day 4` now, and record the gate state (expected to fail until the watermark exists — report the raw failure without fixing).
2. Then implement the watermark tracking only (per-timestamp reader counts; register a transaction's `read_ts` before it can race with compaction; deregister when its final `Arc` owner — including a scan iterator — drops). No compaction-GC yet.
3. Independent compile, then the focused check `cargo test -p mini-lsm-starter --lib week3_day`; report exact results.

No parallel wakes; complete the slice in your continuous turn.

### Apprentice

Watermark-tracking slice is implemented: the `BTreeMap` counts readers per timestamp; `new_txn` captures and registers `read_ts` under one timestamp mutex; `Transaction::Drop` deregisters only when the final transaction/iterator `Arc` is gone.

`cargo check -p mini-lsm-starter` passes. Focused gate: 10/11 pass; the two watermark tests now pass, and the sole expected remaining failure is `week3_day4::test_task3_mvcc_compaction` (`expected b`, `actual a`) because compaction GC is intentionally not in this slice.

Review: the registration happens while holding the same mutex that contains `latest_commit_ts`. What race could occur if it read the timestamp, released the mutex, and registered only afterward?

### Student

**Review answer (registration race):** if the transaction read `latest_commit_ts` (say 5), released the mutex, and only then registered 5, a **gap opens between the read and the registration**. In that window a concurrent writer can commit a new batch (ts 6) and compaction can advance its watermark past 5 — reclaiming versions that the not-yet-registered transaction at `read_ts = 5` still needs. The transaction would then read missing/incorrect data, and the watermark would have "skipped over" a snapshot that existed but wasn't yet counted.

Holding the same mutex across the timestamp read and the registration closes the gap: `read_ts` is captured and registered atomically, so the watermark can never advance past a timestamp before that timestamp is registered as a reader. That's why capture-and-register must be one critical section.

### Apprentice

Accepted. The gap could let compaction reclaim the version needed by the just-created `read_ts=5` snapshot; the shared critical section closes it.

Next slice is watermark-based compaction GC: retain every version above the watermark and exactly the newest version at/below it; remove that selected tombstone only for bottom-level output, while never splitting one user’s versions across SSTs. The Day 4 compaction failure is its gate. Authorize this slice when ready.

### Student

**Authorization — Checkpoint 2, slice 3 (watermark-based compaction GC):** yes. Scope: compaction retains every version above the watermark plus exactly the newest version at/below it; the selected boundary version may be dropped only when it's a tombstone AND the output is bottom-level (no older value can survive elsewhere); never split one user key's versions across SSTs. The `week3_day4::test_task3_mvcc_compaction` failure is the gate.

Protocol: independent compile first, then `cargo test -p mini-lsm-starter --lib week3_day`; post the exact result. If a *legacy* Week 1/2 tombstone test regresses, classify it per the checkpoint rules. After the slice, one important-line question as usual.

### Apprentice

Compaction-GC slice is complete. For each user key, it preserves all versions above the watermark and only the first version at/below it; a selected tombstone is omitted only for bottom-level output. SST splitting now occurs before a new user key, so one user’s retained versions cannot span output files.

`cargo check -p mini-lsm-starter` passes. `cargo test -p mini-lsm-starter --lib week3_day` passes: 11 passed, 0 failed (71 filtered out).

Review: why must compaction keep the first version at or below the watermark, instead of deleting every version at/below it? What snapshot read would otherwise change?

### Student

**Review answer (boundary version retention):** compaction must keep the first version at/below the watermark because it is the **newest version the oldest live snapshot can see**. The watermark is the minimum live `read_ts`; a reader at exactly `read_ts = watermark` selects the newest version ≤ watermark — the boundary version. If compaction deleted *every* version at/below the watermark, that reader would find no version for the key and return **absence instead of the value** (or silently fall through to nothing) — a changed, wrong read. Older versions below the boundary are unobservable (no live reader's `read_ts` reaches them), so they can be reclaimed; the boundary version itself is the floor that must survive.

### Apprentice

Accepted. The boundary version is the newest version an oldest live snapshot can still select; deleting it would turn that visible value into absence.

**Guarantee: local isolation — Course rule.** A transaction’s private workspace has `a=new`, `b=delete`, and `c=new`; its snapshot has `a=old`, `b=old`, and no `c`. What should that transaction’s `get(a)`, `get(b)`, and `get(c)` return, and what should another transaction started at the same snapshot see? Reply `simpler`, `example`, `hint`, or `choose for me` if useful.

### Student

**My answer (Course rule — local isolation):**
- **This transaction's reads:** `get(a)` → **"new"** (the local workspace is the newest source and wins over the snapshot's `old`); `get(b)` → **absent** (the local `delete` is a tombstone that hides the snapshot's `b=old` — no fall-through); `get(c)` → **"new"** (only the workspace has `c`).
- **Another transaction started at the same snapshot** (no workspace of its own): `get(a)` → **"old"**, `get(b)` → **"old"**, `get(c)` → **absent** — the workspace is private to its owner; no other transaction sees it.

Rule: local writes/tombstones take precedence over the engine snapshot for the owning transaction, and nothing leaks to other transactions until commit.

### Apprentice

Accepted. The private workspace is the owner’s newest source; its tombstone stops fallback, and it stays invisible to every other transaction until commit.

**Guarantee: transaction lifetime — Course rule.** A transaction attempts `commit()` once, then the caller tries `get`, `scan`, `put`, `delete`, and `commit` again. Which operations must be rejected, and at what point must the transaction mark itself committed to prevent any later use from racing with publication?

### Student

**My answer (Course rule — use after commit):** after the first `commit()` attempt, **every** later call — `get`, `scan`, `put`, `delete`, and `commit` again — must be rejected: no operation may continue using the transaction.

The transaction must mark itself committed **before publishing its writes** (atomically, at the start of the commit path). That way, once publication begins, no later caller can race it: a `get` mid-publish could otherwise observe a partially published workspace, a `put` could add writes after publication (never committed), and a second `commit` could double-publish. Marking committed first makes every subsequent call deterministic — `get`/`scan`/`commit` (which return `Result`) return an error; the starter's non-fallible `put`/`delete` need a deterministic rejection (e.g. an assertion) unless we explicitly approve changing their signatures. Consistency requirement: no operation continues using the transaction; the return types don't all have to match.

### Apprentice

Accepted. Marking committed before publication prevents a racing read, late workspace write, or duplicate commit; `Result` methods error and the non-fallible methods reject deterministically without changing their public signatures.

**Guarantee: atomic visibility — Course rule.** A batch writes `a` and `b` with one timestamp. WAL append succeeds; the engine pauses after inserting only `a` into the memtable, before timestamp publication. A concurrent new transaction begins during the pause. Should it see `a`, `b`, or neither? Which event makes the complete batch visible?

### Student

**My answer (Course rule — atomic visibility / publication):** during the pause, a concurrent new transaction must see **neither `a` nor `b`**.

Why: the new transaction's `read_ts = latest_commit_ts`, which is still the *old* value (the batch's timestamp `T` hasn't been published). `a@T` sits in the memtable physically, but every reader's `read_ts` is `< T`, and version filtering skips versions above `read_ts` — so `a@T` is invisible, and `b` isn't even installed yet. The batch is therefore unobservable in its entirety.

The event that makes the **complete** batch visible is the **publication of `latest_commit_ts = T`**, done only after *every* memtable entry (`a` and `b`) is installed. From that point, new transactions get `read_ts ≥ T` and see both records together — atomic visibility.

### Apprentice

Accepted. A physical `a@T` is still invisible until `latest_commit_ts=T`; publishing only after all inserts makes the batch appear together.

**Guarantee: crash atomicity — Course rule.** A process stops halfway through writing one WAL batch frame. On reopen, why must recovery validate the complete frame (lengths and checksum) before it inserts any record, and what incorrect state appears if it inserts each parsed record immediately?

### Student

**My answer (Course rule — crash atomicity / recovery validation):** recovery must validate the **complete frame** (lengths and checksum) before inserting any record so a torn batch is never partially replayed. If it inserted each record as it parsed, a stop halfway through the frame would install a **prefix** of the batch — e.g. `a@1` restored but `b@1` missing — and after restart the database would expose a *partially committed* write that never actually committed. A reader could see `a`'s new value while `b` still shows the old one: exactly the torn-batch state crash atomicity forbids. Validation-first makes the outcome all-or-nothing: a complete, checksum-valid frame replays all its entries together; an incomplete/corrupt frame is dropped entirely — never a prefix.

### Apprentice

Accepted. Validation-first prevents a torn WAL frame from becoming a visible prefix after restart.

**Guarantee: timestamp uniqueness — Course rule.** WAL append and every memtable insert succeed, `latest_commit_ts=T` is published, then the fallible freeze-maintenance step fails. Should `T` remain published or be rolled back? Why does that choice matter for the next batch’s timestamp?

### Student

**My answer (Course rule — timestamp uniqueness / publication order):** `T` must **remain published** — the commit is already durable and visible (WAL accepted, all memtable entries installed, timestamp published). The fallible freeze-maintenance step happens *after* publication, so its failure cannot undo the commit.

Why it matters: if `T` were rolled back, the next batch would allocate `T` again while records stamped `T` already exist in the WAL/memtable — **timestamp reuse**. Two different batches would share one timestamp, so version order could no longer tell which batch came first (and recovery would see ambiguous duplicates). Publishing before fallible maintenance guarantees each batch's timestamp is consumed exactly once and the next batch always allocates strictly above the published value, even when maintenance fails.

### Apprentice

Accepted. Once WAL, all inserts, and publication succeed, `T` is consumed; rolling it back after failed maintenance would reuse a timestamp already present in recoverable state.

Proposed Checkpoint 3 slice: first run the course-authorized root command `cargo x copy-test --week 3 --day 5` and record its initial gate; then implement the private workspace and its iterator merge, owner-only reads/tombstones, deterministic post-commit rejection, and commit through the existing timestamped batch path. Files: `mvcc/txn.rs` and the existing storage batch integration only. No conflict validation yet. Authorize this slice when ready.

### Student

**Authorization — Checkpoint 3, slice 1 (transaction workspace + atomic commit path):** yes, in this order per the fast-forward: (1) run `cargo x copy-test --week 3 --day 5` and record the initial gate state (raw failure, no fixing); (2) implement the private skiplist workspace and its merge into the transaction iterator (owner-only), local tombstones hiding both local and engine values, deterministic post-commit rejection of all operations, and commit through the existing timestamped batch path (mark committed before publishing; WAL append → all memtable inserts → publish `latest_commit_ts` → then freeze maintenance). Files: `mvcc/txn.rs` + storage batch integration only. No conflict validation yet.

Protocol: independent compile first, then `cargo test -p mini-lsm-starter --lib week3_day`; post the exact result. Then the usual one-important-line review question.

### Apprentice

Day 5 tests were copied. The initial focused gate cannot compile, before any workspace behavior runs: seven calls in the supplied `week3_day5.rs` require `KeySlice::from_slice(key, ts)`, but the current permitted starter API exposes `from_slice(key)` and separate `from_slice_with_ts(key, ts)`.

This is a public-interface mismatch outside the authorized `mvcc/txn.rs` + storage-only slice, so I have not changed code. Please clarify whether you authorize the narrow API migration to the Day-5-required two-argument `KeySlice::from_slice`, with mechanical starter call-site updates, or want this recorded as course/test friction first.

### Student

**Decision — authorize the narrow API migration, and record the mismatch as experiment friction.**

1. **Authorized:** make `KeySlice::from_slice` accept the two-argument form `(key, ts)` as the Day-5 tests require (retaining the one-arg behavior via an explicit default-timestamp call where the starter still needs it — e.g. a `from_slice(key, 0)` or keeping `from_slice_with_ts` as the explicit variant). Apply mechanical call-site updates only; no other API changes.

Proceed with the migration + the rest of the slice (workspace, iterator merge, rejection, commit path), then the focused gate. Post the exact result.

### Apprentice

I found one remaining protocol/language conflict before changing it: Rust cannot overload `KeySlice::from_slice`, and the pre-existing provided `checkpoint4_tests.rs` still calls the one-argument form. Updating that supplied test would violate `AGENTS.md`’s no-hand-edit-tests rule; leaving it unchanged makes a two-argument-only Day-5 API fail the existing suite.

I can safely migrate production callers (`mem_table.rs`, `wal.rs`) and keep `from_slice_with_ts`, but cannot preserve both `from_slice(key)` and `from_slice(key, ts)` in Rust. Please route a course-authorized resolution for the existing provided test/API incompatibility; I will not edit the test unilaterally.

### Student

**Boundary report — course-authorized resolution needed (per the fast-forward: stop and report rather than improvise around the course).**

**The conflict:** the supplied `week3_day5` tests call `KeySlice::from_slice(key, ts)` (two args), but the pinned baseline's key module (the permitted Week-3 copy) exposes one-arg `from_slice(key)`, and the pre-existing supplied `checkpoint4_tests.rs` calls the one-arg form. Rust cannot overload by arity, so no single signature satisfies both supplied test modules; updating either supplied test violates AGENTS.md's no-hand-edit-tests rule.

1. **Authorize a mechanical test-compat shim for the experiment only:** change `KeySlice::from_slice` to the two-arg form and mechanically update the *one* conflicting call site in the supplied `checkpoint4_tests.rs` (`from_slice(key)` → explicit default-ts form), with the change recorded as experiment friction and a real course-side fix routed afterward.
2. **Treat as a course defect:** stop the Day-5 slice here, record the incompatibility as the experiment's key finding, and wait for the course material/base to be repaired before continuing Week 3.
3. Any other reconciliation you prefer.

No code has been changed beyond the authorized slices; Apprentice is holding at the boundary.

### Apprentice

Acknowledged. Day 5 is paused at the raw compile gate; I will not change the learner workspace or recopy tests until the canonical repair is verified and Student authorizes the clean restart.

### Student

**Authorization — clean Day-5 re-copy and resume** (the course repair passed independent review).

Apprentice — resume: (1) obtain the repaired copy path (the `xtask/src/main.rs` change from the repair head `<reviewed-course-repair>` — apply it to your worktree's xtask only; do not touch the learner implementation or any supplied test); (2) run the clean root command `cargo x copy-test --week 3 --day 5`; (3) confirm the copied Day-5 module now uses `from_slice_with_ts` for the seven timestamped calls; (4) resume the authorized Checkpoint-3 slice-1 implementation (private workspace + iterator merge, owner-only reads/tombstones, deterministic post-commit rejection, commit through the timestamped batch path); (5) independent compile, then `cargo test -p mini-lsm-starter --lib week3_day`; post the exact result. The raw compile gate from before stays as learner evidence.

### Apprentice

Clean Day-5 re-copy succeeded with exactly seven `from_slice_with_ts` calls. The authorized workspace slice compiles and the focused gate now passes 15/16.

The remaining product failure is `week3_day5::test_task4_wal_rejects_truncated_or_corrupt_batch_without_applying_it`: `Wal::recover` returns `Ok` for a truncated frame, while the test requires an error and an empty map. I stopped before changing recovery. Which invariant/path is wrong: should a torn header/body/checksum be treated as end-of-log, or as an invalid WAL frame that errors without replaying anything? Once you diagnose/authorize, I’ll make the minimal repair.

### Student

**Diagnosis + authorization (WAL recovery contract):** a torn header/body/checksum must be treated as an **invalid WAL frame that errors without replaying anything** — not as silent end-of-log.

Reasoning: the test (`test_task4_wal_rejects_truncated_or_corrupt_batch_without_applying_it`) pins the contract: if there are bytes that form an incomplete/corrupt frame, recovery must return an error and apply nothing. Silent end-of-log would discard evidence of corruption and could hide a torn write that *was* partially durable; erroring keeps the invariant "recovery either replays a complete, checksum-valid frame or fails cleanly with zero applied entries." Since validation already happens before any insertion, the map naturally stays empty on the error path.

**Authorize the minimal repair:** `Wal::recover` returns `Err` (rather than `Ok`) when a frame fails validation (truncated body/lengths or checksum mismatch), applying nothing. Then rerun `cargo test -p mini-lsm-starter --lib week3_day` and post the exact result.

### Apprentice

WAL repair complete: recovery now validates every frame before applying any recovered entry and returns an error for truncated length/body/value or checksum corruption. `cargo test -p mini-lsm-starter --lib week3_day` now passes: 16 passed, 0 failed (71 filtered).

Review: why does recovery keep parsed entries in a temporary collection until all frames validate, rather than inserting a valid earlier frame immediately?

### Student

**Review answer (all-or-nothing recovery):** recovery buffers parsed entries and inserts them only after **every** frame validates so that a single invalid frame makes the whole recovery **all-or-nothing**. If a valid earlier frame were inserted immediately and a later frame then failed validation, recovery would already have mutated the map before returning `Err` — leaving the database in a *partially recovered* state where the error claims nothing was applied but some entries actually were. Buffering until all frames validate guarantees the contract: any validation failure → `Err` with **zero** applied entries; full success → all entries applied together. This is the crash-atomicity boundary applied to the entire recovery pass, not just one frame.

### Apprentice

Accepted. Buffering makes the recovery pass itself all-or-nothing: any invalid frame leaves the map untouched, while full validation installs everything together.

### Apprentice

Proposal: treat Checkpoint 3’s implementation slice as complete and do its approval exercises before Day 6. First, trace four failure points—WAL append failure, pause after first memtable insert, freeze-maintenance failure after publication, and `sync` failure—stating visibility, recovery, and durability for each. Then use the existing Day-5 torn-frame test as the crash-atomicity evidence. Finally review `MemTable::put_batch`: its WAL append occurs before the insertion loop; changing that order would expose in-memory data after a WAL failure and break the acceptance/publication boundary. After those answers, we can close Checkpoint 3 and begin Checkpoint 4’s design dialogue.

### Student

**Checkpoint-3 approval exercises — my answers:**

1. **WAL append failure:** batch never enters the memtable (WAL precedes insertion) → caller gets `Err`; nothing visible, nothing durable, recovery has nothing to replay. Atomic visibility + crash atomicity hold trivially (no partial state exists).
2. **Pause after first memtable insert, before publication:** `a@T` sits physically in the memtable but `latest_commit_ts` isn't published → new transactions have `read_ts < T` and see **neither** `a` nor `b` (the whole batch is invisible). If the process stops here: the WAL holds the complete frame → recovery replays the whole batch, or (pre-`sync`) may lose the frame entirely — never a prefix. Durability only after successful `sync`.
3. **Freeze-maintenance failure after publication:** the commit (`T`) is already visible and durable; the maintenance failure cannot undo it → next batch allocates `T+1` (no timestamp reuse). Freeze failure is a maintenance event, not a commit failure.
4. **`sync` failure:** the batch is in WAL + memtable + published → **visible** in-memory; but durability is **unconfirmed** (caller gets `Err`); on crash the frame may be lost before it was synced. Visibility and durability are separate claims — `sync` success is the documented durability boundary.

**`MemTable::put_batch` order:** WAL append must precede the insertion loop so a WAL failure exposes **zero** in-memory entries (the error path leaves no partial state, matching recovery). Reversing the order (insert first, then WAL) would leave the batch visible in memory even though it was never durably accepted — and if the process stopped after insertion but before WAL append, recovery would never replay it: an accepted-looking but non-durable, non-recoverable write. The order *is* the acceptance/publication boundary.

Day-5 torn-frame test stands as the crash-atomicity evidence. Ready to close Checkpoint 3 and open Checkpoint 4's design dialogue.

### Apprentice

Mostly accepted, with one correction: freeze-maintenance failure after publication leaves `T` visible and consumed, but not necessarily durable—only a successful `sync` establishes that guarantee. It must remain published so the next batch gets `T+1`; a later crash may still lose an unsynced WAL frame. The other traces and the WAL-before-insert explanation are correct.

Checkpoint 3 is closed. **Guarantee: point-key isolation — Course rule.** T1 reads missing `x` and plans to write `a`; T2 commits `x=new`; then T1 commits. Should T1 be allowed, and which dependency must it record to decide correctly? Reply `simpler`, `example`, `hint`, or `choose for me` if useful.

### Student

**My answer (Course rule — point-key isolation):** T1 must **not be allowed** — its commit should be aborted.

The dependency T1 must record: a **point-read miss on `x`** (the key's absence at T1's `read_ts`). Absence can influence a decision, so T1's read set must include "x was missing" as well as any values it read. At commit time, validation compares T1's read set against write sets committed **after** T1's `read_ts`: T2's commit wrote `x`, and `x` is in T1's read set as a miss → the dependency T1 relied on (x absent) no longer holds → abort. Without recording misses, T1 would see only its own writes and commit anyway, producing the write-skew-like inconsistency where both transactions' decisions were based on each other's absent keys.

### Apprentice

Accepted. A missing-key read is a real dependency: T1 must record `x` and abort when T2’s later write set contains it.

**Guarantee: point-key isolation — Course rule.** T1 reads `b` then plans to write `a`; T2 reads `a` then plans to write `b`; both began from the same snapshot. Which transaction(s) may commit under the point-read validation rule, and which lock must cover validation through publication so they cannot both pass concurrently?

### Student

**My answer (Course rule — write skew / validation):** **exactly one** of the two may commit — the one that validates and publishes first; the other aborts.

Why: T1's read set is {b}, T2's read set is {a}. When T1 commits first, its read set {b} conflicts with nothing committed yet → passes, and it publishes a write to `a`. T2's commit then checks {a} against write sets committed after its `read_ts`: T1's `a` is there → conflict → T2 aborts (and vice versa if T2 commits first). Neither commits if... well, under the lock, exactly one gets through. This is what prevents write skew: both began from the same snapshot, each read a key the other wrote, so at most one can commit.

The lock that must cover **validation → timestamp allocation → publication → committed-write-set insertion** is the **`commit_lock`** — serializing the whole commit critical section so two transactions cannot both validate against the same snapshot state and both pass. Releasing it between validation and publication would let both read "nothing committed yet" and both commit — the exact race the lock prevents.

### Apprentice

Accepted. `commit_lock` serializes validation through write-set insertion, so exactly the first conflicting writer publishes and the other aborts.

**Guarantee boundary — Course rule.** T1 scans an empty range `(a, c)` and decides to write `z`; T2 inserts `b` and commits; T1 then commits. Does the point-key validation rule necessarily abort T1? Explain the limitation precisely.

### Student

**My answer (Course rule — scan-phantom limitation):** **no**, T1 is not necessarily aborted — the point-key validation rule **permits** this history.

The limitation, precisely: validation records only the **keys actually returned** by scans (returned-key hashes), not the **predicate or the gap**. T1's scan of `(a, c)` returned **nothing**, so it adds no key to T1's read set. T2's commit writes `b` after T1's `read_ts`, but `b` is not in T1's read set (T1 never observed `b` — it didn't exist). Validation finds no conflict and T1 commits, even though its decision to write `z` relied on the emptiness of `(a, c)` that T2's insert just violated. That is the documented **range phantom**: an insert into an empty range is invisible to point-key validation. This is conservative point-key validation, not full Serializable Snapshot Isolation — it rejects some serializable histories but also misses this one, which must be explained as a required limitation, not claimed away.

### Apprentice

Accepted. An empty scan records no returned keys, so an insert into its gap is a documented phantom that point-key validation permits; this is not full serializability.

Proposed Checkpoint-4 slice 1: run the authorized root copy `cargo x copy-test --week 3 --day 6`, record the initial gate, then add point-key read/write-set tracking and `commit_lock` validation through publication; read-only commits allocate neither timestamp nor metadata. No compaction filters yet. Authorize when ready.

### Student

**Authorization — Checkpoint 4, slice 1 (point-key serializable validation):** yes, in this order per the fast-forward: (1) run `cargo x copy-test --week 3 --day 6` and record the initial gate state (raw failure, no fixing); (2) implement point-key read/write-set tracking (record read misses too), and commit validation under `commit_lock` covering validation → timestamp allocation → publication → committed-write-set insertion; a failed validation publishes neither data nor metadata; read-only transactions commit without allocating a timestamp or retaining metadata. No compaction filters yet.

Protocol: independent compile first, then `cargo test -p mini-lsm-starter --lib week3_day`; post the exact result, then the usual one-important-line review question.

### Apprentice

Checkpoint-4 validation implementation now compiles and the focused Day-6 subset passes 5/5. I still need to run the required cumulative Week-3 gate before claiming the slice result.

### Apprentice

Checkpoint-4 slice-1 gate is green: `cargo test -p mini-lsm-starter --lib week3_day` = 21 passed, 0 failed (71 filtered). Read misses and scan-returned keys are tracked; `commit_lock` now holds validation through timestamped publication and write-set recording, while read-only commits return without allocating a timestamp.

Review: why must `commit_lock` remain held after validation until the committed write set is inserted? What race would appear if it were released immediately after validation?

### Student

**Review answer (commit_lock scope):** `commit_lock` must stay held until the committed write set is inserted so that the transaction's writes are **atomically visible to later validators**. Validation works by comparing a transaction's read set against write sets committed after its `read_ts`; if the lock were released right after validation but before the write-set insertion, a concurrent T2 could validate in that gap and **not see T1's just-committed writes** — T2's read set would conflict with T1's keys, but the registry would still show them uncommitted, so T2 would pass and both would commit. That's exactly the validation race: release the lock only after validation + timestamp allocation + publication + write-set insertion are all done, so every subsequent validator sees the full committed state.

### Apprentice

Accepted. The write-set insertion is part of publication to future validators; releasing `commit_lock` earlier lets a conflicting transaction validate against an incomplete registry.

Proposed final Checkpoint-4 slice: copy Day 7, record its initial gate, then add compaction filters that preserve versions above the watermark and omit the selected matching version at/below it plus its older matching history. Reads in the filtered prefix at/below the watermark are intentionally undefined. Authorize when ready.

### Student

**Authorization — Checkpoint 4, slice 2 (compaction filters):** yes, per the fast-forward: (1) run `cargo x copy-test --week 3 --day 7` and record the initial gate state; (2) implement compaction filters — versions **above** the watermark stay untouched even when the user key matches; when the first matching version **at or below** the watermark is selected, omit it **and every older version** of that user key from the task's output; reads in the filtered prefix at/below the watermark are intentionally undefined. Files: the compaction-filter integration only.

Protocol: independent compile first, then `cargo test -p mini-lsm-starter --lib week3_day`; post the exact result, then the usual one-important-line review question.

### Apprentice

Compaction-filter slice is green. Matching keys keep versions above the watermark, then discard the first matching version at/below it and all older versions. `cargo test -p mini-lsm-starter --lib week3_day` = 22 passed, 0 failed (71 filtered).

Review: with watermark 5 and filtered `k@8, k@5, k@2`, why must `k@8` remain while `k@5` and `k@2` may disappear?

### Student

**Review answer (compaction filter vs watermark):** with watermark 5, `k@8` must remain because it is **above the watermark** — a reader at `read_ts ≥ 8` may still legally observe it, and filters never touch above-watermark versions. `k@5` (the first matching version **at/below** the watermark) and `k@2` (older) may disappear because the filter's contract deliberately overrides ordinary snapshot visibility inside the filtered prefix: once the user key matches, the selected boundary version and everything older are omitted. Reads of that key at `read_ts ≤ 5` become **undefined** — outside the snapshot contract — which is the documented tradeoff: compaction filters sacrifice predictable old-version reads for space reclamation, and they must never remove anything a live above-watermark reader could still need.

### Apprentice

Accepted. The filter intentionally makes the at/below-watermark prefix undefined, but versions above the watermark remain protected. All focused Week-3 Day 1–7 gates are green; the next step is the final `cargo x scheck` and handoff audit when you authorize it.

### Student

**Authorization — Day-3 final audit sequence** (fast-forward "Audit the Finished MVCC Engine"):

1. Run from the repository root: `cargo x scheck` — post the exact result (per the fast-forward, Checkpoint 2's watermark-based tombstone removal should have made it pass again; any failure: classify, don't silently fix).
2. Produce the final **evidence report** with the fast-forward's 8 items: (1) combined decision + guarantee ledgers incl. delegated choices; (2) exact commands and outcomes; (3) one internal-key byte layout + user-range→internal-range translation; (4) one key traced through overwrite/tombstone/old-snapshot/flush/compaction + a timestamp-oracle restart trace; (5) one transaction traced through local reads → WAL acceptance → memtable publication → timestamp publication → sync → later visibility; (6) one write-skew dependency that aborts + one scan phantom still permitted; (7) versions retained at a watermark, with and without a matching filter; (8) one weakness not established by the supplied tests.
3. Then the **deliberate-fault/revert exercise**: introduce one small fault whose guarantee you can name (e.g. ascending timestamps, dropping a scan's transaction owner, publishing memtable before WAL, releasing commit_lock between validation and publication, filtering above-watermark versions) — predict the failing test/trace, run it, revert.

### Apprentice

`cargo x scheck` completed successfully: format and check passed; nextest ran 93 tests with 93 passed (one pre-existing leaky test); clippy completed with one non-fatal `collapsible_if` warning in the new transaction-iterator read-set bookkeeping. No behavior failure occurred. I will address that mechanical lint before the deliberate-fault exercise and final evidence report.

### Apprentice

Mechanical lint is fixed; `cargo x scheck` is clean: 93/93 passed and clippy emitted no warnings.

Deliberate-fault prediction: I will temporarily reverse `MemTable::put_batch` so memtable insertion happens before WAL append. This violates crash atomicity/acceptance order; the Day-5 WAL failure trace should expose in-memory state despite WAL failure. I’ll run the narrowest relevant check, then immediately revert the fault.

### Apprentice

Correction to the planned fault: the WAL-order trace has no existing injection point, so I used the allowed ordering fault instead. I temporarily changed internal-key timestamp order from descending to ascending. As predicted for version order/latest reads, `cargo test -p mini-lsm-starter --lib week3_day2` failed: expected `Some("2")`, got `None`. I immediately restored descending order; the working tree no longer contains that deliberate change.

### Apprentice

## Day 3 evidence report

1. **Ledgers.** Version order: user ascending/timestamp descending; snapshots select first version ≤ fixed `read_ts`; watermark is counted per timestamp and scan iterators retain their transaction; batch visibility is WAL → all memtable entries → `latest_commit_ts`; crash recovery validates all WAL frames before any replay; point-key validation holds `commit_lock` through write-set recording; filtered matching history at/below watermark is intentionally undefined. The Student derived the course-rule choices; the Apprentice implemented only the separately authorized slices.

2. **Commands.** `cargo test -p mini-lsm-starter --lib week3_day` finished 22 passed/0 failed. Final `cargo x scheck` finished clean: format/check/clippy clean; nextest 93 passed (one non-failing leaky classification). Deliberate ascending-timestamp fault made `week3_day2` fail (`expected Some("2"), got None`), then was reverted.

3. **Representation/ranges.** `a@9` is stored as user bytes `61` plus timestamp `00 00 00 00 00 00 00 09`; ordering compares `(user, Reverse(ts))`. `(a, b]` maps to `Excluded(a@0)` through `Included(b@0)`; excluded lower skips every `a` version, included upper includes every `b` version.

4. **Version/restart trace.** Write `k@1=v1`; old snapshot reads v1; write `k@2=delete`; latest reads absent while old snapshot still reads v1. After freeze/flush/compaction, watermark retention preserves `k@1` while that snapshot lives; after it drops, obsolete history may go. On reopen, oracle takes max timestamp across live SSTs and recovered WAL memtables, so the next batch is strictly higher.

5. **Commit trace.** Txn local `a=new,b=delete` shadows its snapshot only. Commit frames WAL first, inserts all `a@T,b@T` in one memtable, publishes `latest_commit_ts=T`, then may freeze; `sync` is the durability boundary. New transactions see both only after publication; before sync a crash may lose the complete frame but never replay a prefix.

6. **Isolation.** T1 reads b/writes a; T2 reads a/writes b: first commit’s write set conflicts with the other read set, so the second aborts. Empty scan `(a,c)` then another transaction inserts b: allowed phantom, because returned-key tracking records no gap/predicate.

7. **GC/filter.** With watermark 5, normal GC retains `k@8` and newest `k@5`, drops `k@2`; a matching filter retains `k@8` but may drop `k@5,k@2`, making reads of that filtered key at ≤5 undefined.

8. **Remaining weakness.** The supplied tests do not establish a durable timestamp high-water mark after all max-timestamp evidence is garbage-collected; a production engine that forbids any historical timestamp reuse needs that additional durable metadata.

### Student

**Day-3 completion checkpoint — accepted.** As the learner, I can now demonstrate each item without delegating: (1) encode/order timestamped internal keys and translate ranges (`Excluded(a@0)`..`Included(b@0)`); (2) select values/tombstones per read timestamp (8→absent, 6→v5, 5→v5, 1→absent); (3) the iterator owns its transaction so the watermark protects the snapshot; (4) distinguish atomic visibility (all-or-nothing publication), crash atomicity (whole-frame WAL validation), durability (`sync`), snapshot visibility (fixed `read_ts`), and serializability (point-key validation); (5) trace failures through commit/recovery; (6) build a write-skew abort + a permitted range phantom; (7) explain watermark/filter version retention; (8) the deliberate ascending-timestamp fault showed tests can detect a known invariant violation.
