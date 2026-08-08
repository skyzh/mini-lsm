# Week 1 Student–Apprentice Walkthrough

## Setup

**Student:**

> Work with me through the Week 1 fast-forward checkpoints inside an isolated `mini-lsm-starter` workspace. Follow the student-owned design protocol, use only learner-visible sources, ask one decision question at a time, and wait for authorization before each implementation slice. Reveal supplied tests only after the corresponding checkpoint has a complete compiling first pass.

**Apprentice:**

> Handshake complete. I will use the starter code and book chapters only, keep the reference implementations off-limits, distinguish consequential decisions from mechanical edits, and stop for the Student's reasoning before changing product behavior. No implementation file or supplied test has been opened yet.

## Checkpoint 1: Ordered In-Memory State

### Start Instruction

**Student:**

> Build Day 1 with me, starting with ordered in-memory state. Follow the student-owned design protocol in `AGENTS.md` and never access `../mini-lsm`. Before coding, ask one short question at a time using a concrete example. Use plain English and introduce technical terms after I answer. Mark each question **Course rule** or **Your choice**. I may reply `simpler`, `example`, `hint`, or `choose for me`. Do not edit until my answers specify one small, coherent slice. After each slice, show me one important line and ask what it does and what would break if it changed.
>
> Implement ordered in-memory state.

### Memtable and Cursor Decisions

**Apprentice:**

> **Course rule — What order should a scan use?**
>
> We insert `pear → 1`, then `ant → 2`, then `cat → 3` into one memtable. A full scan starts immediately afterward.
>
> In what order should those three entries appear, and what about the keys determines that order?
>
> You can reply `simpler`, `example`, `hint`, or `choose for me`.

**Student:**

> The scan should return `ant → 2`, `cat → 3`, then `pear → 1`. The order is ascending lexicographic order of the raw key bytes, not insertion order.

**Apprentice:**

> Correct. I’ve recorded the course constraint as **ascending bytewise key order**: insertion order cannot affect a memtable scan.
>
> **Course rule — What survives a repeated write?**
>
> In the same mutable memtable, we write `cat → old` and then `cat → new`. Before any freeze, `get("cat")` and a scan each encounter that key.
>
> Should they expose one entry or two, and which value should be visible?

**Student:**

> They should expose one `cat` entry with `new`. A memtable keeps the latest value for each key, so the later write replaces the earlier mapping.

**Apprentice:**

> Correct. I’ve recorded **one entry per key within a memtable; a later write replaces the earlier value**.
>
> **Course rule — How should deletion survive into newer state?**
>
> An older immutable memtable contains `cat → old`. We delete `cat` while writing to the current mutable memtable. If the current memtable simply omits `cat`, a lookup could fall through and find `old`.
>
> What entry, if any, should the current memtable store for `cat`, and what should a user-facing lookup report when it sees that entry?

**Student:**

> The current memtable must store a tombstone for `cat`, represented by the course's empty value. A lookup that finds it should return `None` and stop; it must not continue into older state and resurrect `old`.

**Apprentice:**

> Correct. I’ve recorded **an empty value is a deletion tombstone; it wins over older values, and the user-visible lookup returns absence without probing farther**.
>
> **Course rule — What happens at the end of a cursor?**
>
> A scan cursor returns its final entry. We call `next()` once and it reaches the end, then call `next()` again.
>
> May that cursor ever become valid again or revisit an entry? What should its validity report after exhaustion?

**Student:**

> No. Once exhausted, the cursor stays exhausted: `is_valid()` remains false and later `next()` calls cannot make it valid again or revisit an entry.

### Memtable Slice Authorization

**Apprentice:**

> Correct. That gives us a coherent first slice: one ordered memtable and its bounded cursor.
>
> Decision ledger:
>
> | Decision or constraint | Student’s conclusion | Invariant/evidence | Consequence |
> |---|---|---|---|
> | Scan order | Ascending lexicographic raw bytes | Ordered-state contract | Insertion order is irrelevant |
> | Repeated key in one memtable | Keep only the latest value | One current mapping per key | `get` and scan expose one version |
> | Deletion representation | Empty value tombstone | Older values must not reappear | Internal cursor exposes it; user lookup treats it as absent |
> | Cursor exhaustion | Permanent invalid state | Cursor API contract | Repeated `next` cannot resume or revisit |
>
> Proposed slice: change only `src/mem_table.rs` to implement in-memory creation, point get/put with approximate-size accounting, byte-bounded ordered scan construction, and cursor `key`/`value`/`is_valid`/`next`. WAL, flush, engine-level reads/writes, merging, and LSM tombstone filtering remain untouched. The supplied `week1_day1` and `week1_day2` modules will be revealed only after the entire ordered-state checkpoint has a compiling first pass—not after this partial slice.
>
> Authorize this slice?

**Student:**

> Authorized. Implement that memtable-and-cursor slice, use only non-test compilation/formatting feedback, and stop for review afterward without copying or inspecting supplied tests.

### Memtable Slice Report and Review

**Apprentice:**

> The memtable-and-cursor slice is implemented and compiles. No supplied tests were copied, opened, or run.
>
> Changed `src/mem_table.rs`:
> - `MemTable::create`, `get`, and `put` now provide a concurrent ordered mapping; later writes replace the visible mapping and every write contributes key+value bytes to the soft size estimate.
> - `scan` converts borrowed bounds into owned bounds, retains the skiplist with `Arc`, and positions the cursor on its first matching item.
> - cursor access and advancement now maintain a permanently invalid exhausted state.
>
> The implementation relies on the ledger’s bytewise ordering, single-version-per-key, empty-value tombstone, and cursor-exhaustion invariants. `crossbeam_skiplist::SkipMap` supplies the ordered map and replacement behavior.
>
> Exact checks:
> - `cargo fmt --check -- src/mem_table.rs` initially reported formatting differences only.
> - `cargo fmt -- src/mem_table.rs` completed successfully.
> - `cargo check --lib` passed (`Finished dev profile`).
> - `git diff --check` passed with no output.
>
> Boundary not yet established by supplied tests: a scan holds the memtable alive, but concurrent insertions after cursor creation may or may not become visible depending on skiplist iteration behavior. We should treat that visibility as outside this slice’s logical guarantee unless we explicitly test and specify it.
>
> Review `src/mem_table.rs:190`:
>
> ```rust
> !self.borrow_item().0.is_empty()
> ```
>
> What is this line trying to distinguish? What plausible behavior would break if it checked whether the **value** was empty instead of whether the key was empty?

**Student:**

> It distinguishes a real positioned entry from the cursor's empty-key exhausted sentinel. Checking the value instead would make a valid tombstone look exhausted, so deletion entries could disappear before the merge/read layers use them to suppress older values.

### Point Operations and Freeze Decisions

**Apprentice:**

> Exactly. The key is an exhaustion sentinel; using the value would erase tombstones before they can defeat older data.
>
> **Course rule — Which copy wins across memtables?**
>
> The mutable memtable has `cat → newest`; the first immutable memtable has `cat → middle`; the last immutable memtable has `cat → oldest`.
>
> In what order should `get("cat")` probe these sources, and when should it stop?

**Student:**

> Probe the mutable memtable first, then immutable memtables from newest to oldest. Stop at the first entry found, whether it is a value or tombstone, because that entry is the newest version.

**Apprentice:**

> Correct. I’ve recorded **mutable first, then immutable memtables newest-to-oldest; the first found entry is authoritative, including a tombstone**.
>
> **Course rule — How long must a writer hold the state read guard?**
>
> Writer W identifies mutable memtable A. Before W inserts `cat → new`, freezer F replaces A with mutable memtable B and moves A into the immutable list. W then inserts into A.
>
> What went wrong, and which part of W’s operation must remain protected by the `state` read guard to prevent it?

**Student:**

> W wrote into A after A had been published as immutable, so readers or a concurrent flush could miss or race with a supposedly frozen write. W must retain the `state` read guard from selecting the current mutable memtable through completion of the insertion; the freezer needs the write guard and therefore cannot publish A as immutable until that write finishes.

**Apprentice:**

> Correct. I’ve recorded **the state read guard covers both selecting the mutable memtable and completing its insertion**, so freezing cannot publish a memtable while a write is still entering it.
>
> **Course rule — What must a second freezer recheck?**
>
> Two writers both observe that mutable memtable A reached the soft size limit. Writer 1 acquires `state_lock`, freezes A, and installs a fresh empty B. Writer 2 then acquires `state_lock` based on its earlier observation.
>
> Before freezing, what current condition must Writer 2 check, and what should it do in this case?

**Student:**

> It must re-read the latest state under `state_lock` and recheck the current mutable memtable's approximate size. Here the current table is fresh B and below the limit, so Writer 2 should do nothing rather than freeze B.

**Apprentice:**

> Correct. I’ve recorded **serialize structural changes with `state_lock`, then recheck the current memtable’s size; an obsolete observation must not freeze its replacement**.
>
> **Course rule — When does the soft limit trigger?**
>
> The target size is 100 bytes. After a write, the current memtable’s accumulated key-plus-value estimate is exactly 100.
>
> Should the writer attempt a freeze at 100, only above 100, or before the write—and why?

**Student:**

> Attempt the freeze after the write when the estimate is at least 100 bytes. It is a soft capacity limit, so the write is accepted first; equality is enough to trigger maintenance.

### Point/Freeze Slice Authorization

**Apprentice:**

> Correct. We now have enough decisions for the engine point-operation and freeze slice.
>
> Added ledger entries:
>
> | Decision or constraint | Student’s conclusion | Invariant/evidence | Consequence |
> |---|---|---|---|
> | Cross-memtable lookup | Mutable, then immutable newest-to-oldest; stop at first hit | Latest version is authoritative | Tombstones prevent resurrection |
> | Write/freeze guard | Hold `state.read()` through insertion | A published immutable table must never receive another write | Freeze waits for in-flight writes |
> | Competing freezes | Take `state_lock`, then recheck current size | Earlier observations can become stale | Fresh replacement is not frozen |
> | Soft limit | Insert first; attempt freeze at `size >= target` | Capacity is best-effort and inclusive | The triggering write is retained |
>
> Proposed slice: change only `src/lsm_storage.rs` to implement memtable-only `get`, `put`, `delete`, and `force_freeze_memtable`. Structural freezing will create the replacement before the short state write-lock section, move the old table to the front of the newest-first immutable list, and rely on `state_lock` supplied by the caller. Disk reads, scans/merging, WAL, and flushing stay untouched. `week1_day1` remains unrevealed until the whole ordered-state checkpoint compiles.
>
> Authorize this slice?

**Student:**

> Authorized. Implement the memtable-only point operations and freeze slice as described. Use no supplied tests and stop for review after non-test checks pass.

### Point/Freeze Slice Report and Review

**Apprentice:**

> The point-operation and freeze slice is implemented and compiles. No supplied tests were copied, inspected, or run.
>
> Changed `src/lsm_storage.rs`:
> - `get` probes the mutable table and immutable tables newest-to-oldest, stops at the first hit, and converts an authoritative tombstone to `None`.
> - `put` holds the state read guard through insertion, checks the post-write soft limit, serializes a possible freeze, and rechecks the current table.
> - `delete` routes through the same guarded write path using the empty-value tombstone.
> - `force_freeze_memtable` prepares a replacement, then atomically swaps the structural snapshot and inserts the old table at the newest side of `imm_memtables`.
>
> Exact checks:
> - `cargo fmt --check -- src/lsm_storage.rs` passed.
> - `cargo check --lib` passed (`Finished dev profile`).
> - `git diff --check` passed with no output.
>
> Boundary not yet established by supplied tests: `force_freeze_memtable` itself will freeze even an empty table when explicitly called; the automatic path’s size check prevents that under normal writes. Callers of the force API therefore own the precondition if empty immutable tables are undesirable.
>
> Review `src/lsm_storage.rs:322` inside the scope that begins with the state read guard:
>
> ```rust
> state.memtable.put(key, value)?;
> ```
>
> What is this line trying to guarantee by running before the read guard is dropped? What concrete behavior could break if we first cloned the memtable pointer, dropped the guard, and only then performed this insertion?

**Student:**

> It guarantees that the table remains the mutable table until the insertion completes. If we cloned the pointer and dropped the guard first, a freezer could publish that table as immutable or begin flushing it, then this delayed insertion could be missing from the frozen snapshot/SST or race with an object that readers assume no longer changes.

### Merge and Logical Iterator Decisions

**Apprentice:**

> Correct. That is the exact unsafe interleaving the read guard prevents.
>
> **Course rule — Which equal key does a merge expose?**
>
> We construct a merge from two already-sorted cursors. Input 0 contains `cat → new`; input 1 contains `cat → old`. Both currently point at `cat`.
>
> Which entry should the merge expose, and what must happen to the other `cat` when the merge advances?

**Student:**

> Expose input 0's `cat → new`, because inputs are ordered newest to oldest and the lower input index wins equal keys. When advancing, consume every other cursor currently on `cat` so the merged output contains the key only once and cannot later emit `old`.

**Apprentice:**

> Correct. I’ve recorded **lower input index wins equal keys, and advancing consumes every input positioned on that key before selecting the next output**.
>
> **Course rule — Can an iterator recover after an error?**
>
> A user-visible scan is valid on `cat`, but its next advancement reports an I/O-style error. The caller invokes `is_valid()` and then calls `next()` again.
>
> What must those later calls report, and why must the cursor not resume even if an underlying child still has entries?

**Student:**

> After the first error, `is_valid()` must remain false and every later `next()` must return an error without advancing the child. Otherwise a caller could continue after a partially failed transition and observe a stream whose position or ordering is no longer trustworthy.

**Apprentice:**

> Correct. I’ve recorded **the user-facing fused cursor enters a permanent errored state: invalid thereafter, with every later `next()` returning an error and no child advancement**.
>
> **Course rule — At which layer may a tombstone disappear?**
>
> Newer input 0 has `cat → tombstone`; older input 1 has `cat → old`. If input 0 drops the tombstone before duplicate resolution, the merge can expose `old`.
>
> Should the merge keep or remove the tombstone, and which layer should finally hide it from the user?

**Student:**

> The merge must keep the tombstone so it wins duplicate resolution and suppresses `old`. Only the outer `LsmIterator`, after source precedence has been resolved, should skip tombstones from the user-visible stream.

### Merge Slice Authorization

**Apprentice:**

> Correct. I’ve recorded **merge tombstones as ordinary precedence-bearing entries; only `LsmIterator`, after duplicate resolution, hides them from callers**.
>
> We have enough decisions for the final ordered-scan slice.
>
> Added ledger entries:
>
> | Decision or constraint | Student’s conclusion | Invariant/evidence | Consequence |
> |---|---|---|---|
> | Equal keys across inputs | Lower input index wins | Inputs are constructed newest-to-oldest | One newest version is emitted |
> | Duplicate advancement | Consume every child on the emitted key | Each logical key appears once | Older copies cannot appear later |
> | Error behavior | Permanently invalidate and keep returning error | Position after partial failure is untrustworthy | User cannot resume a failed stream |
> | Tombstone layer | Preserve through merge; hide in `LsmIterator` | Deletion must defeat older values first | No deleted key or resurrected value reaches user |
>
> Proposed slice:
> - `src/iterators/merge_iterator.rs`: heap construction, newest-first equal-key precedence, duplicate consumption, exhaustion/error-safe child removal.
> - `src/lsm_iterator.rs`: delegate the merged stream, skip tombstones only after resolution, and enforce fused exhaustion/error behavior.
> - `src/lsm_storage.rs`: build bounded memtable cursors in mutable/newest-immutable order and expose the fused scan.
>
> No SST or two-type merging is included. After this compiles, Checkpoint 1 has its independent first pass; only then I will reveal `week1_day1` and `week1_day2` with the prescribed copy commands and run their focused tests.
>
> Authorize this slice?

**Student:**

> Authorized. Implement the merge, LSM/fused iterator, and memtable-only scan slice. After it compiles, reveal only Week 1 Days 1 and 2 and run their focused tests as prescribed.

### Checkpoint 1 Result and Review

**Apprentice:**

> Checkpoint 1’s implementation and supplied validation are complete: all 14 focused tests passed.
>
> Final slice behavior:
> - `MergeIterator` keeps only valid children in a min-key/newest-index heap, exposes one current entry, advances every equal-key child, and removes exhausted or errored heap children safely.
> - `LsmIterator` filters tombstones only after merge precedence is resolved.
> - `FusedIterator` forbids access when invalid and permanently records a child error.
> - engine `scan` constructs bounded inputs in mutable-then-immutable-newest-first order.
> - `cargo x copy-test` added only the supplied harness, `week1_day1`, `week1_day2`, and rewrote `src/tests.rs`; I did not hand-edit them.
>
> Exact commands and results:
> - `cargo fmt --check -- src/iterators/merge_iterator.rs src/lsm_iterator.rs src/lsm_storage.rs` — passed.
> - `cargo check --lib` — passed.
> - `git diff --check` — passed.
> - `cargo x copy-test --week 1 --day 1` — copied `harness` and `week1_day1`.
> - `cargo x copy-test --week 1 --day 2` — added `week1_day2`.
> - `cargo test -p mini-lsm-starter week1_day1` — 6 passed, 0 failed.
> - `cargo test -p mini-lsm-starter week1_day2` — 8 passed, 0 failed.
>
> The consolidated ledger is the accepted ordered byte scan; one latest mapping per memtable; empty-value tombstones; permanent exhaustion/error; mutable then immutable newest-first lookup; read guard through write; serialized/rechecked freeze; post-write `>=` soft limit; lower-index equal-key precedence; consume all duplicate cursors; and hide tombstones only after merge.
>
> Boundary not established by these supplied tests: whether a memtable scan created before a concurrent insertion should observe that insertion. The implementation promises ordering of entries it observes, not snapshot isolation for a live skiplist cursor.
>
> Review `src/iterators/merge_iterator.rs:99`:
>
> ```rust
> if iter.1.key() != current.1.key() {
>     break;
> }
> ```
>
> What is this comparison trying to decide? What plausible output bug would appear if it broke on equality instead, leaving the older cursor positioned on the same key?

**Student:**

> It keeps consuming heap children while their current key equals the key just emitted, and stops at the first different key. If it broke on equality, the older cursor would remain on the duplicate and could emit the same logical key again—possibly surfacing an older value immediately after a newer value or tombstone.

**Apprentice:**

> Correct. That comparison is the duplicate-elimination gate; reversing it would allow stale versions to re-enter the stream.
>
> **Course rule — Adversarial prediction before adding any test**
>
> Newer input: `b → tombstone, c → 4`. Older input: `a → 1, b → 2, c → 3`. We scan the full user-visible range.
>
> What exact sequence should the caller observe, and which layer removes each hidden entry? I will not add a new test unless you explicitly authorize it.

**Student:**

> The caller should observe `a → 1`, then `c → 4`. `MergeIterator` discards the older duplicate `b → 2` behind the newer tombstone and the older `c → 3` behind `c → 4`; `LsmIterator` then hides the winning `b` tombstone. Do not add a test. Proceed to Checkpoint 2: implement the durable block and SST representation.

## Checkpoint 2: Durable Block and SST Representation

### Block Format Decisions

**Apprentice:**

> **Course rule — Which block format is this checkpoint targeting?**
>
> A block stores `mini-a`, `mini-b`, and `mini-c`. The earlier Day 3 format writes each full key; the final Day 7 format writes the first key in full and later keys as shared-prefix length plus remaining bytes.
>
> Because this fast-forward checkpoint will eventually validate Days 3, 4, and 7 together, should the independent first pass implement the earlier full-key layout first, or go directly to the final prefix-compressed layout—and what compatibility goal determines that choice?
>
> You can reply `simpler`, `example`, `hint`, or `choose for me`.

**Student:**

> Go directly to the final Day 7 prefix-compressed layout. The checkpoint must pass the combined Days 3, 4, and 7 acceptance target, so implementing and then replacing the earlier layout would be wasted work and obscure the final format contract.

**Apprentice:**

> Correct. I’ve recorded **the final Day 7 prefix-compressed block layout is the checkpoint’s sole acceptance format**.
>
> **Course rule — What should every compressed key depend on?**
>
> A block contains `mini-a`, `mini-b`, and `mini-c`. An iterator may jump directly to the third entry using its offset. If `mini-c` stores only its difference from `mini-b`, decoding it would require walking through entry 2 first.
>
> Should each later key record its shared prefix with the first key or with the immediately previous key, and why?

**Student:**

> Use the first key as the prefix basis for every later key. Then an iterator can reconstruct any entry from the first key plus that entry alone, preserving direct offset access and allowing independent comparisons during seek.

**Apprentice:**

> Correct. I’ve recorded **every key is encoded relative to the block’s first key, so any indexed entry is independently reconstructible**.
>
> **Course rule — What bytes make one final-format entry?**
>
> The first entry is key `a` (`0x61`) with value `X` (`0x58`). Its shared-prefix length is necessarily zero.
>
> In field order, which lengths and payload bytes must this entry contain, and how should the fixed-width integers be encoded?

**Student:**

> The entry is `overlap_len:u16 = 0`, `rest_key_len:u16 = 1`, the remaining key byte `0x61`, `value_len:u16 = 1`, then value byte `0x58`. Encode every `u16` in big-endian order, so the bytes are `00 00 00 01 61 00 01 58`.

**Apprentice:**

> Correct. The one-entry data section is exactly `00 00 00 01 61 00 01 58`.
>
> **Course rule — How can a decoder locate entries from the end?**
>
> That entry begins at data offset 0 and is the only entry in the block. What footer bytes must follow the data section, and which final field tells the decoder how many preceding offsets belong to the footer?

**Student:**

> Append the entry offset `0` as `00 00`, then the entry count `1` as `00 01`. The final `u16` count tells the decoder that the preceding one `u16` belongs to the offset table and where the data section ends.

**Apprentice:**

> Correct. The complete one-entry block is the 8-byte entry, then `00 00` offset and final `00 01` count; decoding from the last two bytes locates the offset table and data boundary.
>
> **Course rule — Is the target inclusive?**
>
> A non-empty builder has room for another record whose projected encoded block size—including its entry bytes, new 2-byte offset, and final 2-byte count—would be exactly the 64-byte target.
>
> Should `add` accept or reject that record, and which comparison expresses the rule?

**Student:**

> Accept it. The target is inclusive, so a non-first entry is rejected only when the projected encoded size is greater than the target, not when it is equal.

**Apprentice:**

> Correct. I’ve recorded **accept projected size equal to the target; reject only when a non-first entry would make it greater**.
>
> **Course rule — What if one record is larger than an empty block’s target?**
>
> An empty builder has a 16-byte target, but one valid key/value record requires 30 encoded bytes including its offset and the count. Rejecting it would leave the builder empty forever when the SST builder retries it.
>
> Should the empty builder accept this first record, and what should happen to later records?

**Student:**

> Accept the oversized first record so the builder can make progress and produce a one-entry block. Once non-empty, reject later records whose projected size exceeds the target.

**Apprentice:**

> Correct. I’ve recorded **an empty builder accepts one representable oversized record; only subsequent additions are constrained by the target**.
>
> **Course rule — Where does a missing-key seek land?**
>
> A block contains keys `1`, `3`, and `5`. We seek to `0`, `3`, `4`, and `6` in four fresh iterators.
>
> Where should each iterator land, including the case after the block’s last key?

**Student:**

> Seek uses lower-bound semantics: `0` lands on `1`, `3` on `3`, `4` on `5`, and `6` makes the iterator invalid because no stored key is greater than or equal to it.

**Apprentice:**

> Correct. That is **lower-bound seek**: first stored key greater than or equal to the target, otherwise invalid.
>
> **Your choice — How should a block find that lower bound?**
>
> For a block with many indexed entries, we can decode entries from the front until one is large enough, or binary-search entry indices and decode only logarithmically many candidates. Linear scan is simpler; binary search is faster for large blocks and is possible because every key uses the first key as its prefix basis.
>
> Which approach should this implementation use?

**Student:**

> Use binary search over entry indices. The offset table and first-key prefix basis make each candidate independently decodable, so logarithmic comparisons are worthwhile without introducing a dependency chain.

**Apprentice:**

> Accepted. I’ve recorded **binary-search entry indices for lower-bound seeks**, using independent reconstruction from the first key.
>
> **Your choice — How defensive should `Block::decode` be?**
>
> Its API returns `Block`, not `Result<Block>`. For malformed bytes, we can either rely on course-format invariants and let indexing/conversion panic naturally, or add focused assertions for footer length, offset-table bounds, and monotonic/in-range offsets, still panicking but with clearer failures. The latter is safer and clearer but adds code outside behavior exercised by valid-input tests.
>
> Which level should we implement?

**Student:**

> Add focused assertions for the minimum footer size, computed offset-table bounds, and monotonic in-range offsets. The API cannot return a recoverable error, but explicit invariant failures are clearer and prevent obscure out-of-bounds behavior.

**Apprentice:**

> Accepted. I’ve recorded **focused decode assertions at the non-`Result` trust boundary**, covering footer size, offset-table bounds, and monotonic in-range entry offsets.
>
> **Course rule — What records are representable?**
>
> The format stores overlap length, remaining-key length, value length, and entry offsets as `u16`. A key remainder or value is 65,536 bytes, or the data section’s next offset no longer fits in 16 bits.
>
> Should the builder truncate those numbers, reject the addition cleanly, or fail an invariant—and what corruption would truncation cause?

**Student:**

> Reject the addition cleanly with `false`, even on an empty builder, when any encoded length or offset cannot fit in `u16`. Truncation would wrap the field and make the decoder slice the wrong key/value bytes or jump to the wrong entry boundary, corrupting the block.

### Block Slice Authorization and Report

**Apprentice:**

> Correct. I’ve recorded **return `false` rather than truncate any length/offset that cannot fit in `u16`; oversized-first acceptance applies only to representable records**.
>
> We have enough decisions for a coherent block-format slice.
>
> Decision ledger:
>
> | Decision or constraint | Student’s conclusion | Invariant/evidence | Consequence |
> |---|---|---|---|
> | Acceptance format | Final Day 7 prefix layout | Combined Day 3/4/7 target | No temporary full-key format |
> | Prefix basis | First key in block | Indexed entry must decode independently | Direct seek remains possible |
> | Entry encoding | BE `overlap:u16`, `rest_len:u16`, rest bytes, `value_len:u16`, value | Course byte protocol | Exact reader/writer agreement |
> | Footer | BE offset per entry, then BE count | Decoder starts from file end | Data/footer boundary is recoverable |
> | Target boundary | Equality accepted | Target is inclusive | Reject non-first only if projected size is `>` |
> | Oversized first | Accept if representable | Builder must make progress | May produce one-entry oversized block |
> | Seek | First key `>=` target | Lower-bound contract | After-last seek is invalid |
> | Seek algorithm | Binary search (student choice) | Independently decodable indexed entries | Logarithmic candidate comparisons |
> | Decoder checks | Focused assertions (student choice) | API cannot return `Result` | Clear invariant failures on malformed bytes |
> | Representability | Cleanly reject unencodable `u16` fields | Truncation corrupts boundaries | No wrapped lengths or offsets |
>
> Proposed slice: implement `src/block/builder.rs`, `src/block.rs`, and `src/block/iterator.rs` end to end for the final compressed representation, including build/encode/decode, bounds-safe indexed entry decoding, iteration, and binary-search lower-bound seek. SSTs and Bloom filters remain untouched. Day 3 tests will not be revealed until the entire durable-representation checkpoint—including SST and Bloom work—has a compiling first pass.
>
> Authorize this slice?

**Student:**

> Authorized. Implement the complete final-format block builder, codec, and iterator slice. Use only non-test checks and stop for review without revealing Days 3, 4, or 7 yet.

**Apprentice:**

> The final-format block slice is implemented and compiles. Days 3, 4, and 7 remain unrevealed.
>
> Changed:
> - `src/block/builder.rs`: first-key prefix compression, clean `u16` representability rejection, inclusive projected-size accounting, oversized-first progress, and raw data/offset construction.
> - `src/block.rs`: exact big-endian data/offset/count encoding and focused footer/offset assertions on decode.
> - `src/block/iterator.rs`: independent indexed reconstruction, borrowed value ranges, permanent exhaustion, and binary-search lower-bound seek.
>
> Exact checks:
> - Workspace-wide `cargo fmt --check` reported formatting changes in the three implementation files and also wanted to reorder generated `src/tests.rs`; I did not let it modify supplied tests.
> - `rustfmt src/block.rs src/block/builder.rs src/block/iterator.rs` formatted only implementation files.
> - `cargo check --lib` passed.
> - `git diff --check` passed.
>
> Boundary not established by future valid-input tests: `Block::decode` verifies footer location and ordered in-range offsets, while entry-length validation occurs lazily during iteration. It does not prove that one malformed entry’s claimed payload ends before the next entry offset; a production decoder could validate every entry boundary eagerly.
>
> Review `src/block/builder.rs:79`:
>
> ```rust
> if !self.is_empty() && projected_size > self.block_size {
> ```
>
> What are the two separate rules expressed by this condition? What plausible behavior would break if `>` changed to `>=` or if the `!self.is_empty()` guard were removed?

**Student:**

> The non-empty guard implements progress for one representable oversized first record; the strict `>` makes the target inclusive for later records. Changing it to `>=` would reject an entry that fits exactly, while removing the guard could leave an empty builder unable to accept a large first record and cause the SST builder to retry forever.

### SST and Bloom Decisions

**Apprentice:**

> Correct. Those are exactly the two independent boundary rules.
>
> **Course rule — How does an SST locate both variable-length trailers?**
>
> The final file is `blocks | metadata | meta_offset:u32 | bloom | bloom_offset:u32`. A reader knows only the total file length at open time.
>
> Which field does it read first, and once it has that value, where exactly does it find the metadata offset?

**Student:**

> Read the final four bytes as `bloom_offset:u32`. That points to the start of the Bloom section; the metadata offset is the four-byte field immediately before it, at byte range `bloom_offset - 4 .. bloom_offset`.

**Apprentice:**

> Correct. I’ve recorded **read `bloom_offset` from the final four bytes, then read `meta_offset` from the four bytes immediately preceding the Bloom section**.
>
> **Course rule — What must one block’s metadata preserve?**
>
> Block 0 begins at file byte 0 and contains keys `a..c`; block 1 begins at byte 120 and contains `e..g`. The reader must seek without loading both blocks and must know each encoded block’s byte span.
>
> What three facts should each `BlockMeta` record, and in what order must the metadata records appear?

**Student:**

> Each `BlockMeta` records the encoded block's starting byte offset plus its actual first and last keys. Records appear in the same ascending key/block order as the data blocks; the next block's offset, or the metadata-section offset for the last block, gives the current block's byte span.

**Apprentice:**

> Correct. I’ve recorded **offset plus actual first/last key for every block, in the same ascending order as encoded blocks**.
>
> **Your choice — How should the metadata section delimit records?**
>
> The reader already receives the exact metadata byte slice. We can either prefix it with a `u32` record count and encode each record as `offset:u32, first_len:u16, first_key, last_len:u16, last_key`, or omit the count and parse records until the slice ends. A count gives a direct allocation/check and an explicit cardinality; parsing to the end saves four bytes but makes trailing/truncated-data diagnosis less explicit.
>
> Which representation should we use?

**Student:**

> Prefix the metadata section with a `u32` record count, then encode each record with its `u32` offset and `u16`-length-prefixed first and last keys. The explicit count is worth four bytes for clearer cardinality and truncation checks.

**Apprentice:**

> Accepted. I’ve recorded **count-prefixed big-endian metadata records with `u32` offsets and `u16`-length-prefixed boundary keys**.
>
> **Course rule — How should an SST seek across a gap?**
>
> Block 0 contains `a,b,c`; block 1 contains `e,f,g`. Metadata search by first keys selects block 0 for target `d`, but the block iterator reaches its end because no key in block 0 is `>= d`.
>
> What must the SST iterator do next, and where should it finally land?

**Student:**

> It must advance to block 1 and seek to that block's first entry, landing on `e`. A lower-bound seek cannot become invalid merely because the initially selected predecessor block ended before the target.

**Apprentice:**

> Correct. I’ve recorded **if the candidate block exhausts during seek, advance to the next block’s first entry before declaring the SST iterator invalid**.
>
> **Your choice — Which metadata boundary should choose the candidate block?**
>
> For target `d` between blocks `a..c` and `e..g`, we can binary-search first keys and initially choose the predecessor block (`a..c`), then advance when its local seek is invalid; or binary-search last keys and directly choose the first block whose last key is `>= d`. First-key search matches the simplest course outline; last-key search avoids the extra candidate read but relies on accurate last-key metadata.
>
> Which approach should we use?

**Student:**

> Use last-key metadata to choose the first block whose last key is at least the target. We already treat last keys as required accurate metadata, and this avoids reading a predecessor block that cannot satisfy the seek.

**Apprentice:**

> Accepted. I’ve recorded **binary-search block last keys and select the first block whose last key is `>=` the target**.
>
> **Course rule — What identifies one cached block?**
>
> SST 7 and SST 9 both request their block index 0. If the cache key is only `0`, one table could receive the other table’s bytes.
>
> What pair must be used as the cache key, and what should concurrent misses for that same pair share?

**Student:**

> Use `(sst_id, block_index)` as the cache key. Concurrent misses for the same pair should share one loaded `Arc<Block>`/single cache fill, while equal block indices from different SST IDs remain distinct.

**Apprentice:**

> Correct. I’ve recorded **cache identity is `(sst_id, block_index)`, and same-pair concurrent misses share one cached `Arc<Block>` load**.
>
> **Course rule — Which Bloom answer is authoritative?**
>
> For a lookup of `cat`, the filter can say either “definitely absent” or “may contain.” A false positive is possible; a false negative must not be.
>
> Which answer permits skipping the SST, and what lookup work must happen for the other answer?

**Student:**

> Only “definitely absent” permits skipping the SST. “May contain” is inconclusive, so the engine must still perform the normal metadata/block seek and compare the actual key.

**Apprentice:**

> Correct. I’ve recorded **only a Bloom negative can skip an SST; a positive always falls through to the normal exact lookup**.
>
> **Course rule — What must the writer and reader hash?**
>
> The builder inserts `cat` into the filter. Later the reader probes for `cat`. Hashing a prefix, an encoded compressed form, or timestamp-decorated bytes on only one side could produce a false negative.
>
> Which bytes and hash function must both sides use in this Week 1 format?

**Student:**

> Both sides must hash the exact raw Week 1 user-key bytes with `farmhash::fingerprint32`. The builder stores those `u32` fingerprints and lookup computes the same fingerprint from the requested key; neither compressed encoding nor a different key projection may enter one side.

**Apprentice:**

> Correct. I’ve recorded **both writer and reader use `farmhash::fingerprint32` over identical raw Week 1 key bytes**.
>
> **Course rule — What filter target applies?**
>
> The course fixes the Bloom false-positive rate at `0.01`. Should that target be stored as an on-disk setting, exposed as a new option, or used only by the SST builder to compute bits per key while the encoded filter carries the resulting bitset and probe count?

**Student:**

> Use `0.01` only in the SST builder to compute bits per key. The on-disk Bloom section needs only the resulting bitset and probe count `k`; do not add a public option or persist the target probability itself.

**Apprentice:**

> Correct. I’ve recorded **`0.01` is a builder-only sizing input; the encoded Bloom filter stores only its bitset and `k`**.
>
> **Your choice — Should decoded block keys reuse storage?**
>
> Each iterator move reconstructs `first_key_prefix + suffix`. We can allocate a fresh key buffer for every entry, which is simple but adds allocation churn, or keep the starter’s `KeyVec`, clear it, and append into reusable capacity.
>
> Which approach should we retain?

**Student:**

> Reuse the iterator's existing `KeyVec` capacity: clear it and append the first-key prefix plus suffix for each decoded entry. That preserves the same observable behavior while avoiding an allocation per move.

### Bloom Core Slice

**Apprentice:**

> Accepted. I’ve recorded **reuse the iterator’s `KeyVec` allocation while reconstructing each key**; the current block slice already follows that choice.
>
> We have enough decisions for a small Bloom core slice before SST integration.
>
> Relevant ledger:
>
> | Decision or constraint | Student’s conclusion | Invariant/evidence | Consequence |
> |---|---|---|---|
> | Bloom authority | Only negative skips; positive performs exact lookup | False positives allowed, false negatives forbidden | Optimization cannot change results |
> | Hash input | `farmhash::fingerprint32` of exact raw key bytes on both sides | Writer/prober identity | Present key cannot be skipped by projection mismatch |
> | FPR target | Builder-only `0.01` sizing input | Course contract | Disk stores bitset and `k`, not policy |
>
> Proposed slice: change only `src/table/bloom.rs` to set the derived `k` bit positions for every supplied hash and probe the same sequence, preserving the starter’s encoded bitset-plus-`k` representation. SST building/opening and engine lookup integration remain untouched. Days 3, 4, and 7 remain unrevealed until all durable representation work compiles.
>
> Authorize this Bloom core slice?

**Student:**

> Authorized. Implement the Bloom core only, using non-test checks and leaving the durable-representation tests unrevealed.

**Apprentice:**

> The Bloom core is implemented and compiles; Days 3, 4, and 7 remain unrevealed.
>
> Changed `src/table/bloom.rs`: building and probing now use the same rotate/add sequence, modulo the identical bit count. A build sets all `k` positions for each hash; a probe returns false at the first clear bit and true only when all positions are set.
>
> Exact checks:
> - `rustfmt src/table/bloom.rs` completed.
> - `cargo check --lib` passed.
> - `git diff --check` passed.
>
> Boundary not established by future valid-input tests: `Bloom::decode` assumes a non-empty encoded section and would panic on an empty slice; the final SST opener must validate trailer boundaries before calling it, but this API still treats filter bytes as trusted.
>
> Review `src/table/bloom.rs:120`:
>
> ```rust
> if !self.filter.get_bit(hash as usize % nbits) {
>     return false;
> }
> ```
>
> What logical claim does this line make? What correctness failure would become possible if a probe returned false even though all of a present key’s builder-set positions were actually set?

**Student:**

> It says that one clear bit among the key's required probe positions proves the key was never inserted, so the filter may return “definitely absent.” Returning false when all builder-set positions are present would be a false negative; the read path could skip an SST containing the key and return stale data or absence.

### Final SST Decisions and Authorization

**Apprentice:**

> Correct. That line is safe only because builder and prober use the identical positions; violating that identity creates a result-changing false negative.
>
> **Your choice — How defensive should `SsTable::open` be?**
>
> Unlike `Block::decode`, this API returns `Result`. We can trust all trailer offsets and let slicing/indexing panic on a truncated file, or validate the minimum trailer, `bloom_offset`, `meta_offset`, and section ordering and return descriptive errors before reading sections. The latter adds checks but keeps malformed disk data from crashing the process.
>
> Which approach should we use?

**Student:**

> Validate the minimum trailer length, both offsets, and `blocks <= metadata < meta_offset field < bloom < final bloom_offset field` ordering, returning descriptive errors before slicing. Since `open` already returns `Result`, malformed disk bytes should not panic the process.

**Apprentice:**

> Accepted. I’ve recorded **`SsTable::open` returns descriptive errors for malformed trailer length, offsets, and section ordering before slicing**.
>
> We now have enough decisions for the final durable SST slice.
>
> Proposed changes:
> - `src/table.rs`: count-prefixed block-metadata codec; validated final trailer opening; block reads; `(sst_id, block_index)` cache; last-key candidate search.
> - `src/table/builder.rs`: split final-format blocks, capture actual boundary keys/offsets, hash raw keys, append metadata + `meta_offset` + Bloom + `bloom_offset`, and build the in-memory table metadata.
> - `src/table/iterator.rs`: lazy one-block-at-a-time first/lower-bound seek and cross-block advancement using cached reads.
>
> This completes Checkpoint 2’s independent first pass. Engine-level Bloom filtering stays for Checkpoint 3, where the SST point-read path exists; the Day 1 guide assigns SST filtering to that logical-engine checkpoint. Once the slice formats and compiles, I will reveal only Days 3, 4, and 7 and run their focused tests.
>
> Authorize this final SST slice?

**Student:**

> Authorized. Implement the SST codec, builder, cache integration, and iterator slice. Once the checkpoint compiles, reveal Days 3, 4, and 7 and run only their focused tests.

### Checkpoint 2 Result and Review

**Apprentice:**

> Checkpoint 2’s implementation and supplied validation are complete: all 19 focused tests passed (9 Day 3, 7 Day 4, 3 Day 7).
>
> Final SST slice:
> - metadata is count-prefixed and stores each block’s offset plus actual first/last keys;
> - the file layout is `blocks | metadata | meta_offset | bloom | bloom_offset` with validated open-time boundaries;
> - the builder hashes raw keys, emits the `0.01`-sized Bloom filter, and retains matching in-memory metadata;
> - block reads use exact adjacent offsets and cache by `(sst_id, block_index)`;
> - SST iterators hold only the current block, select candidates by last-key lower bound, and cross block boundaries lazily.
>
> Exact commands and results:
> - `rustfmt src/table.rs src/table/builder.rs src/table/iterator.rs` — completed.
> - `cargo check --lib` — passed before tests were revealed.
> - `cargo x copy-test --week 1 --day 3` — copied Day 3.
> - `cargo x copy-test --week 1 --day 4` — copied Day 4.
> - `cargo x copy-test --week 1 --day 7` — copied Day 7.
> - `cargo test -p mini-lsm-starter week1_day3` — 9 passed, 0 failed.
> - `cargo test -p mini-lsm-starter week1_day4` — 7 passed, 0 failed.
> - `cargo test -p mini-lsm-starter week1_day7` — 3 passed, 0 failed.
> - final `git diff --check` — passed.
>
> The consolidated ledger additionally records final prefix format; first-key prefix basis; big-endian entry/footer protocol; inclusive size target; representable oversized-first progress; lower-bound binary search; focused block checks; count-prefixed SST metadata; final trailer offsets; actual boundary keys; last-key block selection; two-part cache identity; matched raw-key FarmHash; authoritative Bloom negatives only; builder-only `0.01`; reusable decoded-key storage; and descriptive SST trailer errors.
>
> Boundary not established by the supplied tests: the SST iterator’s last-key search assumes metadata faithfully describes block contents. The builder establishes that invariant, but an independently corrupted yet structurally decodable last key could make a seek skip the correct block; production integrity checks/checksums belong later.
>
> Review `src/table.rs:268`:
>
> ```rust
> .partition_point(|meta| meta.last_key.as_key_slice() < key)
> ```
>
> What is this comparison trying to select? What exact-key behavior would break if `<` changed to `<=`?

**Student:**

> It counts blocks whose last key is strictly before the target, so the selected index is the first block with `last_key >= target`. Changing `<` to `<=` would skip a block whose final key exactly equals the target and could miss that exact match.

**Apprentice:**

> Correct. Strict `<` preserves a block whose last key is the exact lower bound.
>
> **Course rule — Adversarial decode prediction**
>
> A block’s first key is encoded as `overlap=0, rest="mini-a"`; its second as `overlap=5, rest="b"`; its third as `overlap=0, rest="z"` with `value_len=0`.
>
> What three full keys must the iterator reconstruct, and should the third entry remain a valid block entry or disappear at this layer? I will not add a new test unless you authorize it.

**Student:**

> The keys are `mini-a`, `mini-b`, and `z`. The third remains a valid block entry with an empty value; the block/SST layers preserve tombstones, and only the outer logical iterator hides them after precedence resolution. Do not add a test. Proceed to Checkpoint 3: connect the components into one logical engine.

## Checkpoint 3: One Logical Engine

### Read-Path Decisions

**Apprentice:**

> **Course rule — Which source owns the logical key?**
>
> `cat` appears as `tombstone` in the mutable memtable, `middle` in the newest immutable memtable, and `old` in the newest L0 SST.
>
> What should `get("cat")` return, what internal entry should a full scan let win before filtering, and what complete source-priority order produces those answers?
>
> You can reply `simpler`, `example`, `hint`, or `choose for me`.

**Student:**

> `get("cat")` returns `None`. Internally, the mutable tombstone wins the scan merge and is then filtered from user output. Priority is mutable memtable, immutable memtables newest to oldest, then L0 SSTs newest to oldest; the first entry for a duplicate key is authoritative.

**Apprentice:**

> Correct. I’ve recorded **mutable > immutable newest-to-oldest > L0 newest-to-oldest; first duplicate wins, then an authoritative tombstone becomes absence**.
>
> **Course rule — What must `get` verify after an SST seek?**
>
> The requested key is `b`, but lower-bound seek on the merged L0 stream lands on `c → 3` because `b` is absent.
>
> What comparison must `get` make before returning a value, and what should it return here?

**Student:**

> After seeking, `get` must require both a valid iterator and `iterator.key() == requested_key`. Here it lands on `c`, so `b` is absent and `get` returns `None` rather than `3`.

**Apprentice:**

> Correct. I’ve recorded **an SST lower-bound result is returned only after exact key equality; the next greater key is not a point match**.
>
> **Course rule — Where does an upper scan bound stop?**
>
> The logical stream is `a,b,c,d`. Compare scans `[b,d)` and `[b,d]` after all sources are merged.
>
> Which keys should each return, and which layer should enforce the upper bound given that SST iterators only receive a lower-bound seek?

**Student:**

> `[b,d)` returns `b,c`; `[b,d]` returns `b,c,d`. The outer `LsmIterator` should enforce the upper bound on the already merged logical stream, since child iterators are positioned from the lower bound and source merging must happen before the final range cutoff.

**Apprentice:**

> Correct. I’ve recorded **`LsmIterator` enforces excluded end with `< end` and included end with `<= end` after merge**.
>
> **Course rule — When must the state guard be released on reads?**
>
> A scan clones the current structural snapshot, then creating each SST iterator may read a block from disk. Holding `state.read()` through those reads can block a freeze or flush from publishing a new snapshot for milliseconds.
>
> What should remain alive for consistency, and what must be dropped before SST iterator creation begins?

**Student:**

> Keep the cloned `Arc<LsmStorageState>` snapshot alive so the scan sees one coherent set of memtables/SST handles. Drop the `state.read()` guard immediately after cloning that snapshot, before any SST block I/O or iterator creation.

**Apprentice:**

> Correct. I’ve recorded **retain the cloned state `Arc`, but release the read guard before constructing/ seeking SST iterators or doing I/O**.
>
> **Your choice — How should memory and disk streams be composed?**
>
> We can wrap memtable and SST iterators in a common dynamic enum/trait object and feed one large merge, or keep a homogeneous memtable merge and homogeneous L0 merge, then combine those two different types with `TwoMergeIterator` where the memory side wins ties. The second preserves static dispatch and the starter architecture; the first is conceptually flatter but adds dispatch/boxing complexity.
>
> Which composition should we use?

**Student:**

> Keep homogeneous memtable and L0 `MergeIterator`s, then combine them with `TwoMergeIterator` so the memory side wins equal keys. This fits the starter's static-dispatch architecture without boxing heterogeneous children.

**Apprentice:**

> Accepted. I’ve recorded **merge memory and L0 separately, then use `TwoMergeIterator` with memory as side A so it wins equal keys**.
>
> **Course rule — When can range filtering skip an SST?**
>
> An SST’s key range is `[b,d]`. Compare scan ranges `(d,f]` and `[d,f]`.
>
> May the engine exclude this SST for each scan, and which endpoint equality makes the answers differ?

**Student:**

> For `(d,f]`, the SST may be skipped because its largest key equals the excluded lower bound, so it contains nothing greater than `d`. For `[d,f]`, it must remain because key `d` is included. Thus an excluded lower bound skips when `sst.last_key <= bound`, while an included lower bound skips only when `sst.last_key < bound`.

**Apprentice:**

> Correct. I’ve recorded the lower-side overlap rule exactly.
>
> **Course rule — What is the symmetric upper-side rule?**
>
> The same SST spans `[b,d]`. Compare scans `[a,b)` and `[a,b]`.
>
> May the SST be skipped for each range, and how do excluded versus included upper bounds compare against the SST’s first key?

**Student:**

> For `[a,b)`, skip the SST because its first key equals the excluded upper bound. For `[a,b]`, keep it because `b` is included. An excluded upper bound skips when `sst.first_key >= bound`; an included upper bound skips only when `sst.first_key > bound`.

**Apprentice:**

> Correct. I’ve recorded the symmetric upper-side overlap rule.
>
> **Course rule — What should active-iterator accounting count?**
>
> A memtable merge has one current child and two children still in its heap; an L0 merge has one current child and one heap child. They are combined by `TwoMergeIterator`.
>
> What should `num_active_iterators()` report for the combined logical iterator, and why must the merge’s current child be included?

**Student:**

> It should report `3 + 2 = 5`. Each merge's current child is temporarily outside its heap but still live and able to produce data, so omitting it would undercount actual active iterators and make filtering/accounting results misleading.

### Read-Path Slice Authorization and Report

**Apprentice:**

> Correct. I’ve recorded **active counts sum all live children, including each merge’s current child outside its heap**.
>
> We have enough decisions for a coherent read-path slice.
>
> Proposed changes:
> - `src/iterators/two_merge_iterator.rs`: merge two iterator types, prefer side A on equality, consume side B’s duplicate, and sum active counts.
> - `src/iterators/merge_iterator.rs`: include current plus heap children in active counts.
> - `src/lsm_iterator.rs`: compose memory and L0 merges, enforce included/excluded upper bounds after merge, retain tombstone filtering, and delegate active counts.
> - `src/lsm_storage.rs`: clone/release the state snapshot before SST I/O; implement exact `get` across memtables and range/Bloom-filtered L0; implement bounded scan with precise SST overlap filtering and newest-first sources.
>
> No flush, lifecycle, or background-trigger behavior changes yet. Days 5 and 6 remain unrevealed until the full logical-engine checkpoint compiles.
>
> Authorize this read-path slice?

**Student:**

> Authorized. Implement the complete memory-plus-L0 read path and iterator accounting slice, using non-test checks and leaving Days 5 and 6 unrevealed.

**Apprentice:**

> The memory-plus-L0 read-path slice is implemented and compiles. Days 5 and 6 remain unrevealed.
>
> Changed:
> - `TwoMergeIterator` now merges different child types, gives side A equality precedence, removes side B’s duplicate, and sums active counts.
> - `MergeIterator`, `LsmIterator`, and `FusedIterator` now propagate accurate active counts.
> - `LsmIterator` owns the final upper bound and stops correctly for included/excluded endpoints after merging and tombstone filtering.
> - `get` and `scan` retain a cloned state snapshot but release the lock before SST I/O; they use newest-first L0 order, exact point equality, Bloom/key-range filtering, precise scan overlap, and excluded-lower handling.
>
> Exact checks:
> - `rustfmt src/iterators/two_merge_iterator.rs src/iterators/merge_iterator.rs src/lsm_iterator.rs src/lsm_storage.rs` — completed.
> - `cargo check --lib` — passed.
> - `git diff --check` — passed.
>
> Boundary not yet established by supplied tests: the public scan path assumes a sensible range (lower not logically after upper). The implementation precisely handles endpoint inclusion, but it does not define a special error contract for reversed bounds; that remains an API precondition inherited from the child range implementation.
>
> Review `src/iterators/two_merge_iterator.rs:42`:
>
> ```rust
> self.use_a = self.a.is_valid() && (!self.b.is_valid() || self.a.key() <= self.b.key());
> ```
>
> What is this comparison trying to choose? What visible behavior would break if `<=` changed to `<` when side A is memory and side B is L0?

**Student:**

> It selects the smaller current key and deliberately selects side A on equality. If `<=` became `<`, an equal key from L0 would win over the newer memory entry, exposing stale data or even resurrecting a value hidden by a memory tombstone.

### Flush and Lifecycle Decisions

**Apprentice:**

> Correct. Equality is the cross-layer recency rule; changing it would invert memory-over-disk precedence.
>
> **Course rule — Which immutable memtable is flushed?**
>
> `imm_memtables` is `[7,6,5]` from newest to oldest, and L0 is `[4,3]` from newest to oldest. One flush completes successfully.
>
> Which memtable ID should be selected, and what should both vectors contain afterward?

**Student:**

> Select the oldest immutable memtable, ID 5. After installing its SST, `imm_memtables` is `[7,6]` and L0 is `[5,4,3]`, with the newly flushed SST inserted at the newest side.

**Apprentice:**

> Correct. I’ve recorded **flush the last/oldest immutable table and install its SST ID at the front/newest side of L0**.
>
> **Course rule — What must the install transaction verify?**
>
> The SST was built from immutable memtable 5 outside the state read-write lock. Before installation, the code reacquires the write lock and removes one immutable entry.
>
> Which identity must it assert, and which three structural updates must be published together in the replacement snapshot?

**Student:**

> Assert that the current oldest/last immutable memtable is still ID 5—the exact source used to build the SST. Publish removal of that memtable, insertion of the new `SsTable` into the `sstables` map, and insertion of ID 5 at the front of `l0_sstables` in one replacement state snapshot.

**Apprentice:**

> Correct. I’ve recorded **verify the popped ID matches the built SST and atomically remove that memtable, add the table object, and prepend its ID to L0**.
>
> **Your choice — How should the flush serialize without holding the state write lock during I/O?**
>
> We can hold `state_lock` across selection, SST construction/file I/O, and the short install transaction while releasing `state.read()` after cloning the selected memtable. This is simple and prevents two flushers selecting the same table, but other structural changes wait. Or we can add an explicit in-flight reservation to release `state_lock` during I/O, which increases concurrency but adds new failure/recovery state.
>
> Which Week 1 design should we use?

**Student:**

> For Week 1, hold `state_lock` across selection, SST construction/I/O, and installation, while releasing the `state` read-write guard during the expensive build. This prevents duplicate flush selection without introducing an in-flight reservation protocol.

**Apprentice:**

> Accepted. I’ve recorded **hold `state_lock` for the whole flush operation, but hold the state read/write guard only for selection and final installation—not during SST construction or file I/O**.
>
> **Course rule — What if the flush observation is stale?**
>
> A caller observes a non-empty immutable list, waits for `state_lock`, and another flush drains the list first. Once this caller acquires `state_lock`, the latest state has no immutable memtable.
>
> Should it fail, build anything, or return successfully without work?

**Student:**

> Recheck after acquiring `state_lock` and return `Ok(())` without building anything when the immutable list is empty. The earlier observation was only a hint; an already-completed flush is not an error.

**Apprentice:**

> Correct. I’ve recorded **recheck under `state_lock`; an already-drained immutable list is a successful no-op**.
>
> **Course rule — When should the background worker flush?**
>
> `num_memtable_limit` is 2. On a worker tick, there are exactly two immutable memtables waiting.
>
> Should it flush at equality or only above the limit, and which end of the list does that triggered flush remove?

**Student:**

> Flush at equality: trigger when `imm_memtables.len() >= num_memtable_limit`. The flush still removes the oldest entry at the end of the newest-first immutable list.

**Apprentice:**

> Correct. I’ve recorded **background flush triggers at `len >= limit` and still selects the oldest immutable table**.
>
> **Course rule — What does `close()` guarantee in Day 1?**
>
> The engine still has data in its mutable memtable. The user calls `close()` twice.
>
> Which worker actions must the first call perform, what should the second call do, and should either call implicitly freeze/flush the remaining mutable data?

**Student:**

> The first call signals the existing background workers to stop and joins their handles. The second is harmless because those handles have already been taken. Day 1 `close()` does not implicitly freeze or flush the remaining mutable memtable.

**Apprentice:**

> Correct. I’ve recorded **`close` signals and joins each existing worker once, is idempotent after handles are taken, and does not implicitly flush mutable state**.
>
> **Course rule — What should opening a new path do?**
>
> The requested database directory does not exist yet, and the first eventual flush needs to create `00001.sst` inside it.
>
> Should `open` reject the path, defer directory creation until flush, or create the directory before constructing the engine—and why?

**Student:**

> Create the directory during `open` before constructing the engine. Then later SST creation has a valid parent path, and opening a fresh database path works without deferring an avoidable filesystem failure to the first flush.

### Final Slice Authorization

**Apprentice:**

> Correct. I’ve recorded **`open` creates the database directory up front so SST writes have a valid parent**.
>
> We have enough decisions for the final write/flush/lifecycle slice.
>
> Added ledger entries:
>
> | Decision or constraint | Student’s conclusion | Invariant/evidence | Consequence |
> |---|---|---|---|
> | Flush selection | Oldest/last immutable table | Immutable list is newest-first | `[7,6,5] → [7,6]` |
> | SST install | Verify built ID equals popped ID; map insert + L0 prepend atomically | One source transitions memory→disk | No loss/duplication/wrong table |
> | Lock scope | Hold `state_lock` across whole flush; release state RW guard during build/I/O | Prevent duplicate selection without long global RW lock | Structural changes serialize |
> | Stale request | Recheck and return `Ok(())` if empty | Earlier observation is only a hint | Concurrently completed work is no-op |
> | Trigger | Flush at immutable count `>= limit` | Limit is inclusive | Oldest backlog entry drains |
> | Close | Signal/join handles once; no implicit mutable flush | Fixed Day 1 lifecycle | Repeated close is harmless |
> | New path | Create directory in `open` | Flush needs a valid parent | Fresh database opens cleanly |
>
> Proposed changes:
> - `src/mem_table.rs`: stream the sorted memtable, including tombstones, into `SsTableBuilder`.
> - `src/lsm_storage.rs`: create the directory, perform oldest immutable flush with I/O outside the state RW lock and atomic verified install, sync the directory entry, and implement idempotent worker shutdown.
> - `src/compact.rs`: trigger background flushing at the configured inclusive limit.
>
> After this compiles, Checkpoint 3’s first pass is complete; I will then reveal only Days 5 and 6 and run their focused tests.
>
> Authorize this final slice?

**Student:**

> Authorized. Implement the memtable-to-SST flush, background trigger, directory setup, and idempotent Day 1 lifecycle slice. After non-test compilation succeeds, reveal Days 5 and 6 and run only their focused tests.

### Checkpoint 3 Result and Review

**Apprentice:**

> Checkpoint 3’s implementation and supplied validation are complete: Day 5 passed 8/8 and Day 6 passed 5/5.
>
> Final slice behavior:
> - memtable flush preserves sorted entries and tombstones in a final-format SST;
> - flush holds `state_lock`, releases state RW guards during build/I/O, rechecks empty state, verifies the installed source ID, and atomically moves it to newest L0;
> - `open` creates the directory and flush syncs its directory entry;
> - the background worker triggers at `imm_memtables.len() >= num_memtable_limit`;
> - `close` signals and joins existing handles once without flushing mutable state.
>
> Exact validation:
> - `rustfmt src/mem_table.rs src/compact.rs src/lsm_storage.rs` — completed.
> - `cargo check --lib` and `git diff --check` — passed before revealing tests.
> - `cargo x copy-test --week 1 --day 5` and `--day 6` — copied the two supplied modules.
> - `cargo test -p mini-lsm-starter week1_day5` — 8 passed, 0 failed.
> - `cargo test -p mini-lsm-starter week1_day6` — 5 passed, 0 failed.
> - First `cargo x scheck` — 46/46 tests passed; Clippy reported one collapsible conditional.
> - I applied only that mechanical cleanup. A direct `let`-chain form was rejected by this repository’s pre-2024 formatter, so I used an edition-compatible `matches!` guard.
> - Final `cargo x scheck` — formatting, check, 46/46 nextest tests, and Clippy all passed with no warnings.
>
> Audit: no assertions or supplied test bodies were removed/changed; test modules were added through `cargo x copy-test`, and `src/tests.rs` contains only their module declarations. No new lint suppressions or unresolved Week 1 placeholders were added. Remaining `unimplemented!` calls are explicitly later-week WAL, batch, compaction, or sync work. New `unwrap`/assert sites protect already-checked decoder lengths, valid-iterator access, non-empty builder metadata, or internal state-ID invariants; none masks a recoverable Week 1 operation.
>
> Boundary not established by the supplied tests: a filesystem failure after the SST file is created/synced but before the state snapshot is installed can leave an orphan file. Recovery/manifest cleanup is a later durability concern; the in-memory state remains unchanged.
>
> Review `src/lsm_storage.rs:442-446`:
>
> ```rust
> assert_eq!(removed.id(), sst_id, "installed SST must match flushed memtable");
> ```
>
> What is this assertion trying to prove about the transition? What corruption could follow if installation silently removed a different immutable memtable than the one used to build the SST?

**Student:**

> It proves the atomic transition removes exactly the immutable memtable whose contents produced the installed SST. If a different memtable were removed, one table's data could be lost while the flushed source remained visible alongside its SST, creating both a missing source and duplicate/stale state.

### Final Adversarial Prediction

**Apprentice:**

> Correct. The assertion ties the removed in-memory source to the exact durable replacement; without it, one transition could simultaneously lose unrelated data and duplicate the flushed data.
>
> **Course rule — Final adversarial state prediction**
>
> Mutable: `b → tombstone, d → 4`
>
> Newest immutable: `a → 1, b → 2`
>
> Newest L0: `a → 0, c → 3, d → 3`
>
> What should `get(a)`, `get(b)`, and scan `[a,d]` return? Name the winning source for each visible or deleted key. I will not add a new permanent test unless you authorize it.

**Student:**

> `get(a)` returns `1` from the newest immutable memtable; `get(b)` returns `None` because the mutable tombstone wins over immutable `b → 2`. Scan `[a,d]` returns `a → 1`, `c → 3`, `d → 4`: `a` wins from the immutable memtable over L0, `b` is deleted by mutable state, `c` comes from L0, and mutable `d → 4` wins over L0 `d → 3`. Do not add a permanent test. Complete the remaining Day 1 audit and deliberate-fault experiment from the guide, reverting the fault immediately, then provide the final evidence report and your candid feedback on this agent path.

## Deliberate-Fault Experiment

**Apprentice:**

> Deliberate-fault prediction: I will temporarily change `LsmIterator`’s excluded upper-bound comparison from `<` to `<=`. I expect `tests::week1_day5::test_task2_storage_scan_end_bound_at_seek_position` to fail because a key exactly at the excluded endpoint becomes visible. I am starting from the clean 46/46 passing state and will revert immediately after the one narrow run.

The predicted test failed at the supplied assertion that the iterator must be invalid at the excluded endpoint. The Apprentice immediately restored `<`, reran the exact test, and observed it pass. No deliberate-fault code remained in the final tree.

## Final Evidence Report

**Apprentice:**

> Day 1 is complete in `<isolated-worktree>`. I did not commit, push, or access `../mini-lsm`.
>
> Implementation
>
> - Ordered memtables, point operations, tombstones, freezing, bounded cursors, merge iterators, and fused error behavior.
> - Final prefix-compressed block codec with inclusive sizing, oversized-first progress, indexed binary-search seeking, and decoder checks.
> - SST metadata/trailer codec, Bloom filters, block cache, lazy SST iteration, and validated opening.
> - Unified memory/L0 `get` and `scan`, exact bounds, source precedence, Bloom/range filtering, and active-iterator accounting.
> - Oldest-memtable flush to newest L0, atomic verified installation, automatic flush triggering, directory setup/sync, and idempotent worker shutdown.
> - Supplied Week 1 Days 1–7 tests copied only after each checkpoint’s compiling first pass.
>
> Key decisions
>
> No decisions were delegated.
>
> - Raw bytewise ordering; latest mapping wins.
> - Empty values are tombstones preserved until the outer logical iterator.
> - Priority: mutable > immutable newest-to-oldest > L0 newest-to-oldest.
> - Writers retain the state read guard through insertion.
> - Final Day 7 first-key prefix format, big-endian fields, inclusive target.
> - Binary-search block seeks and last-key SST candidate selection.
> - Count-prefixed block metadata and validated SST trailers.
> - Bloom writer/reader use identical FarmHash input; only negatives skip.
> - Static memtable/L0 merges joined by `TwoMergeIterator`.
> - `state_lock` spans flush selection/build/install, while state RW guards do not span I/O.
> - `close` joins workers but does not implicitly flush mutable state.
>
> Validation
>
> - Week 1 Day 1: 6 passed.
> - Day 2: 8 passed.
> - Day 3: 9 passed.
> - Day 4: 7 passed.
> - Day 7: 3 passed.
> - Day 5: 8 passed.
> - Day 6: 5 passed.
> - Final `cargo x scheck`: formatting, compilation, 46/46 tests, and Clippy passed without warnings.
> - `git diff --check` passed.
>
> The deliberate-fault experiment changed excluded upper-bound handling from `<` to `<=`. As predicted, `test_task2_storage_scan_end_bound_at_seek_position` failed at `assertion failed: !iter.is_valid()`. The fault was immediately reverted, and the same test passed.
>
> Data flow
>
> - `put`: hold the state read guard through memtable insertion, update approximate size, then serialize/recheck a possible freeze.
> - `get`: clone the state snapshot, probe memory by recency, then range/Bloom-filter L0, lower-bound seek, and require exact equality.
> - bounded `scan`: construct bounded memory cursors and filtered L0 cursors after releasing the state guard; merge by recency, remove duplicates/tombstones, and enforce the final upper bound.
> - flush: hold `state_lock`, select the oldest immutable table, release state RW guards during SST creation, then verify and atomically replace that source with its SST in newest L0.
>
> The main concurrency risk is writing into a memtable after it has been published immutable; retaining the state read guard through insertion prevents the freezer from acquiring its write guard during that interval. The main correctness-preserving optimization is SST range/Bloom filtering: it changes iterator and I/O counts, never logical results.
>
> A remaining weakness is crash cleanup between SST creation and snapshot installation, which can leave an orphan file. Manifest recovery belongs to later course work. Live skiplist scans also do not promise snapshot isolation from concurrent insertions.
>
> Candid path feedback
>
> The implementation-before-test boundary worked very well: every newly revealed suite passed on its first run, showing that the book plus student-owned decisions were sufficient without assertion-by-assertion test reconstruction.
>
> The main drawback is interaction volume. One-question-at-a-time plus separate slice authorization is pedagogically strong but made “fast-forward” quite turn-heavy. A progress marker and a documented mastery shortcut would help experienced students without weakening the default protocol.
>
> There are two instruction rough edges worth fixing:
>
> - The Day 1 text says both not to copy tests before checkpoints and, elsewhere, to begin with tests copied.
> - `AGENTS.md` says only `cargo x copy-test` may rewrite `src/tests.rs`, while required `cargo x scheck` runs `cargo fmt`, which also reorders that generated file. `copy-test` should emit deterministic rustfmt-ready ordering or the formatter should be explicitly exempted.
>
> One flaw in my own walkthrough: the first memtable slice included approximate-size accounting before we had explicitly reasoned through the freeze threshold. The slice preview disclosed it and was authorized, but a cleaner dialogue would defer that field to the freeze slice.
>
> Finally, because every product suite passed immediately, this walkthrough did not exercise the newly specified failure-coaching behavior—explaining test evidence and waiting for the student’s diagnosis. A separate intentional learner-bug scenario would be useful for validating that part of the path.

## Appendix: Final Implementation and Results

### Implementation Files

- `src/block.rs`
- `src/block/builder.rs`
- `src/block/iterator.rs`
- `src/compact.rs`
- `src/iterators/merge_iterator.rs`
- `src/iterators/two_merge_iterator.rs`
- `src/lsm_iterator.rs`
- `src/lsm_storage.rs`
- `src/mem_table.rs`
- `src/table.rs`
- `src/table/bloom.rs`
- `src/table/builder.rs`
- `src/table/iterator.rs`

The supplied harness and Week 1 Day 1–7 modules were added only through the course's `cargo x copy-test` commands after their checkpoint implementations compiled.

### Result Summary

| Validation target | Result |
|---|---:|
| Week 1 Day 1 | 6 passed |
| Week 1 Day 2 | 8 passed |
| Week 1 Day 3 | 9 passed |
| Week 1 Day 4 | 7 passed |
| Week 1 Day 5 | 8 passed |
| Week 1 Day 6 | 5 passed |
| Week 1 Day 7 | 3 passed |
| Full `cargo x scheck` | 46 passed, Clippy clean |
| Deliberate-fault narrow test | Failed as predicted, then passed after immediate revert |

### Feedback Summary

- The implementation-before-test protection successfully prevented test-shaped development.
- Concrete questions led to correct independent implementations across all three checkpoints.
- The student demonstrated the ordering, representation, bounds, concurrency, flush, and optimization invariants without delegating any decision.
- The path is educational but interaction-heavy; progress markers and a mastery shortcut would make the “fast-forward” framing more accurate.
- The book's test-copy wording and the `copy-test`/`cargo fmt` ownership of `src/tests.rs` should be reconciled.
- A future trial should intentionally begin with a student-owned product bug so the failure-explanation-and-student-diagnosis protocol is exercised directly.
