# Snapshot Read - Memtables and SSTs

During the refactor, you might need to change the signature of some functions from `&self` to `self: &Arc<Self>` as necessary.

## Task 1: MemTable, Write-Ahead Log, and Read Path

Memtable store timestamp, change to scan, encode ts in wal

## Task 2: Write Path

assign mvcc object, take write lock, increase ts by 1

## Task 3: MVCC Compaction

keep all versions, split file, run merge iterator tests

## Task 4: LSM Iterator

return the latest version

pass all tests except week 2 day 6

## Test Your Understanding

* What is the difference of `get` in the MVCC engine and the engine you built in week 2?
* In week 2, you stop at the first memtable/level where a key is found when `get`. Can you do the same in the MVCC version?
