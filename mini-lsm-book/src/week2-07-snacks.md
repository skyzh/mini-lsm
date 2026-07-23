<!--
  mini-lsm-book © 2022-2026 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Batch Write and Checksums

<!-- ![Chapter Overview](./lsm-tutorial/week2-07-overview.svg) -->

The previous chapter completed the persistent LSM engine. This final chapter adds a small API improvement and integrity checks for every on-disk format. Welcome to Week 2 snack time!

By the end of this chapter, you will be able to:

* Implement the batch write interface.
* Add checksums to data blocks, SST metadata, bloom filters, manifest records, and WAL records.
* Define the exact byte range protected by each checksum and reject corrupted input.
* Explain why framing information is necessary for checksummed variable-length records.

**Note:** The starter suite does not provide dedicated tests for this chapter. `cargo x copy-test --week 2` therefore copies Days 1 through 6; requesting `--day 7` reports that no dedicated test exists. Run every earlier test, inspect each encoded format, and add corruption cases that verify your checksum boundaries.

## Before You Begin

This chapter changes every persistent format that the earlier chapters created. A checksum is useful only when the writer and reader agree on the exact bytes it covers and the reader can locate both those bytes and the stored checksum without trusting corrupted length fields blindly.

Keep these invariants in mind:

1. `put` and `delete` delegate to `write_batch`, so the existing single-record behavior remains unchanged. This Week 2 API processes a group of records but does not yet promise transactional atomicity.
2. A checksum is computed over the encoded bytes exactly as they appear on disk, including a consistent byte order for integer fields.
3. Each decoder isolates the same byte range that its encoder checksummed and verifies it before decoding or exposing the protected payload.
4. Offsets and lengths delimit variable-size sections. The checksum itself and unrelated preceding bytes are not accidentally included.
5. Corruption produces an error rather than silently returning data. Handling torn or truncated records without panicking is a separate robustness concern to consider.

> **Predict before coding:** `Bloom::encode` appends a bloom filter to an SST buffer that already contains data blocks and metadata. If the checksum is computed over the entire buffer, what happens when `Bloom::decode` receives only the bloom-filter section? Which offset must the encoder remember?

## Task 1: Write Batch Interface

Prepare for Week 3 by adding a write-batch API. You will need to modify:

```
src/lsm_storage.rs
```

The user passes `write_batch` a slice of `WriteBatchRecord<T: AsRef<[u8]>>`, so keys and values may use types such as `Bytes`, `&[u8]`, or `Vec<u8>`. The two record variants are delete and put. Handle them with the same semantics as the existing `delete` and `put` methods.

Then refactor `put` and `delete` to call `write_batch` with one record.

All tests from earlier chapters should continue to pass.

## Task 2: Block Checksum

Add a checksum after each encoded data block. You will need to modify:

```
src/table/builder.rs
src/table.rs
```

The format of the SST will be changed to:

```plaintext
---------------------------------------------------------------------------------------------------------------------------
|                   Block Section                     |                            Meta Section                           |
---------------------------------------------------------------------------------------------------------------------------
| data block | checksum | ... | data block | checksum | metadata | meta block offset | bloom filter | bloom filter offset |
|   varlen   |    u32   |     |   varlen   |    u32   |  varlen  |         u32       |    varlen    |        u32          |
---------------------------------------------------------------------------------------------------------------------------
```

Use CRC-32 through `crc32fast::hash` to checksum each encoded block.

Normally, the configured block size includes both content and checksum. A 4,096-byte target with a four-byte checksum would therefore leave 4,092 bytes for block content. To preserve the earlier tests, Mini-LSM continues to treat the configured size as the content target and appends the checksum afterward.

In `read_block`, separate the content from its checksum, verify the content, and decode it only after verification succeeds. Run every earlier test after changing the format.

## Task 3: SST Meta Checksum

Add checksums for bloom filters and block metadata. You will need to modify:

```
src/table.rs
src/table/bloom.rs
src/table/builder.rs
```

```plaintext
----------------------------------------------------------------------------------------------------------
|                                                Meta Section                                            |
----------------------------------------------------------------------------------------------------------
| no. of block | metadata | checksum | meta block offset | bloom filter | checksum | bloom filter offset |
|     u32      |  varlen  |    u32   |        u32        |    varlen    |    u32   |        u32          |
----------------------------------------------------------------------------------------------------------
```

Append a checksum in `Bloom::encode` and verify it in `Bloom::decode`. Because `encode` appends to an existing buffer, record the bloom filter's starting offset and checksum only the bytes added for that filter.

Then append a checksum to the block metadata. The existing block count can guide decoding; an explicit metadata length is another possible framing design.

## Task 4: WAL Checksum

In this task, you will need to modify:

```
src/wal.rs
```

Protect each WAL record with its own checksum. You have two implementation choices:

* Generate a buffer of the key-value record, and use `crc32fast::hash` to compute the checksum at once.
* Write one field at a time (e.g., key length, key slice), and use a `crc32fast::Hasher` to compute the checksum incrementally on each field.

Choose whichever approach makes the protected byte range clearest. Both methods must produce the same checksum: incremental hashing must feed the exact encoded bytes in their on-disk byte order. The new WAL encoding is:

```
| key_len | key | value_len | value | checksum |
```

## Task 5: Manifest Checksum

Finally, protect each manifest record. Unlike the WAL, the Day 5 manifest did not store record lengths. Add a length header before each JSON record and a checksum after it.

The new manifest format is like:

```
| len | JSON record | checksum | len | JSON record | checksum | len | JSON record | checksum |
```

After implementing every format change, run all previous tests and your own corruption cases.

## Chapter Checkpoint

All earlier behavior should still pass after the format changes. In addition, every persistent section should round-trip successfully and fail verification when one protected byte is changed. Confirm that appending a bloom filter or metadata section to a non-empty buffer does not make its checksum depend on preceding sections.

Build a small SST and identify every offset, length, payload, and checksum by byte range. Do the same for one WAL and one manifest record. For each format, flip a payload byte and predict which decoder reports the error. Also consider a truncated length or checksum and decide whether your decoder returns an error or panics.

## Test Your Understanding

### Correctness and Corruption

* Does `write_batch` in this chapter provide atomic visibility or durability for the whole batch? What additional synchronization would be needed to make that guarantee?
* Why must an incremental WAL checksum hash the encoded byte order of each length rather than the integer's native in-memory representation?
* For every SST checksum, identify the first protected byte, the last protected byte, and the location of the stored checksum.
* What happens if corruption changes a length or offset before the decoder has located the protected payload? How can a decoder validate bounds before slicing?
* Is it okay to put all block checksums together at the end of the SST file instead of storing each checksum with its block? Why or why not?

### API and Design

* Consider the case that an LSM storage engine only provides `write_batch` as the write interface (instead of single put + delete). Is it possible to implement it as follows: there is a single write thread with an mpsc channel receiver to get the changes, and all threads send write batches to the write thread. The write thread is the single point to write to the database. What are the pros/cons of this implementation? (Congrats if you do so you get BadgerDB!)

We do not provide reference answers to these questions, so feel free to discuss them in the Discord community.

## Bonus Tasks

* **Recovering from Corruption.** If a checksum fails, open the database in a read-only safe mode that can retrieve unaffected data.

{{#include copyright.md}}
