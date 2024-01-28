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
