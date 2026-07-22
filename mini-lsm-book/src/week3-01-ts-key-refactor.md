<!--
  mini-lsm-book © 2022-2026 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Timestamp Key Encoding + Refactor

By the end of this chapter, you will be able to:

* Encode a `(user_key, timestamp)` internal key in blocks and SST metadata.
* Preserve the order `user_key ascending, timestamp descending` through every storage iterator.
* Explain why Bloom filters hash only the user-key portion and why key-range checks ignore timestamps.

To copy and run the test cases:

```
cargo x copy-test --week 3 --day 1
cargo x scheck
```

**Note:** The MVCC subsystem is not complete until Day 2. At the end of this chapter, only the Day 1 tests and Week 1 tests are expected to pass. Week 2 compaction still uses pre-MVCC assumptions.

## Before You Begin

This chapter changes representation, not visibility. Every existing layer that stores, compares, seeks, or summarizes keys must agree on the same internal ordering.

Keep these invariants in mind:

1. Internal keys sort by user key in ascending order and timestamp in descending order.
2. Prefix compression applies only to user-key bytes. The timestamp is encoded in full for every entry.
3. Block metadata stores complete first and last internal keys, including their timestamps.
4. Bloom filters hash user-key bytes only, because a lookup asks whether any version of that user key may exist.
5. Day 1 still writes `TS_DEFAULT`; seeing multiple versions through `LsmIterator` is temporarily acceptable until Day 2.

> **Predict before coding:** In what order should `a@7`, `a@3`, `aa@9`, and `b@1` appear? If a block entry shares all user-key bytes with the previous entry, which fields are still encoded for that entry?

## Task 0: Use MVCC Key Encoding

You will need to replace the key encoding module to the MVCC one. We have removed some interfaces from the original key module and implemented new comparators for the keys. If you followed the instructions in the previous chapters and did not use `into_inner` on the key, you should pass all test cases on day 3 after all the refactors. Otherwise, you will need to look carefully on the places where you only compare the keys without looking at the timestamps.

Specifically, the key type definition has been changed from:

```rust,no_run
pub struct Key<T: AsRef<[u8]>>(T);
```

...to:

```rust,no_run
pub struct Key<T: AsRef<[u8]>>(T /* user key */, u64 /* timestamp */);
```

...where we have a timestamp associated with the keys. We only use this key representation internally in the system. On the user interface side, we do not ask users to provide a timestamp, and therefore some structures still use `&[u8]` instead of `KeySlice` in the engine. We will cover the places where we need to change the signature of the functions later. For now, you only need to run,

```
cp mini-lsm-mvcc/src/key.rs mini-lsm-starter/src/
```

There are other ways of storing the timestamp. For example, we can still use the `pub struct Key<T: AsRef<[u8]>>(T);` representation, but assume the last 8 bytes of the key is the timestamp. You can also implement this as part of the bonus tasks.

```plaintext
Alternative key representation: | user_key (varlen) | ts (8 bytes) | in a single slice
Our key representation: | user_key slice | ts (u64) |
```

In the key+timestamp ordering, the smallest user key appears first, and the largest timestamp for one user key appears first. For example:

```
("a", 233) < ("a", 0) < ("b", 233) < ("b", 0)
```

## Task 1: Encode Timestamps in Blocks

Replacing the key module makes representation assumptions visible as compiler errors. In this task, update:

```
src/block/builder.rs
src/block/iterator.rs
```

`raw_ref()` and `len()` are removed from the key API. Use `key_ref()` for user-key bytes and `key_len()` for their length. Update the block builder and decoder to encode the timestamp explicitly. The new block entry is:


```
key_overlap_len (u16) | remaining_key_len (u16) | key (remaining_key_len) | timestamp (u64)
```

Use `raw_len()` to estimate the space required by the complete internal key, and store the timestamp after the remaining user-key bytes.

After you change the block encoding, update the block iterator to decode and reconstruct timestamped keys accordingly.

## Task 2: Encoding Timestamps in SSTs

Then, you can go ahead and modify the table format,

```
src/table.rs
src/table/builder.rs
```

Change block metadata encoding to include the timestamps of its first and last keys. Because `seek` and `add` accept `KeySlice`, the key comparator then carries the same ordering into SST construction and lookup.

Use `key_ref()` to build the Bloom filter. All timestamped versions of one user key then share one fingerprint.

## Task 3: LSM Iterators

As we use associated generic type to make most of our iterators work for different key types (i.e., `&[u8]` and `KeySlice<'_>`), we do not need to modify merge iterators and concat iterators if they are implemented correctly. The `LsmIterator` is the place where we strip the timestamp from the internal key representation and return the latest version of a key to the user. In this task, you will need to modify:

```
src/lsm_iterator.rs
```

For now, we do not modify the logic of `LsmIterator` to only keep the latest version of a key. We simply make it compile by appending a timestamp to the user key when passing the key to the inner iterator, and stripping the timestamp from a key when returning to the user. The behavior of your LSM iterator for now should be returning multiple versions of the same key to the user.

## Task 4: Memtable

For now, we keep the logic of the memtable. We return a key slice to the user and flush SSTs with `TS_DEFAULT`. We will change the memtable to be MVCC in the next chapter. In this task, you will need to modify:

```
src/mem_table.rs
```

## Task 5: Engine Read Path

In this task, you will need to modify,

```
src/lsm_storage.rs
```

Now that we have a timestamp in the key, and when creating the iterators, we will need to seek a key with a timestamp instead of only the user key. You can create a key slice with `TS_RANGE_BEGIN`, which is the largest ts.

When you check if a user key is in a table, you can simply compare the user key without comparing the timestamp.

At this point, all stored keys still use `TS_DEFAULT` (timestamp 0). The next two chapters assign real commit timestamps and select versions by read timestamp.

## Chapter Checkpoint

Your engine should encode and decode timestamped keys without changing Day 1 visibility semantics.

Verify these cases explicitly:

1. Round-trip two entries with the same user key and different timestamps through a block and an SST.
2. Seek to a timestamp between two versions and confirm that descending timestamp order positions the iterator correctly.
3. Confirm that two versions of one user key add the same Bloom-filter fingerprint.

## Test Your Understanding

* Why is timestamp order reversed while user-key order is not?
* Which encoded structures would become inconsistent if block metadata omitted timestamps?
* Why should a point lookup for `k` test one user-key fingerprint rather than a separate fingerprint for every possible `k@ts`?
* During Day 1, why is it acceptable for `LsmIterator` to return repeated user keys, and why must that behavior change on Day 2?
* Construct a seek target that distinguishes comparing full internal keys from comparing only user keys.

We do not provide reference answers to these questions, so feel free to discuss them in the Discord community.

{{#include copyright.md}}
