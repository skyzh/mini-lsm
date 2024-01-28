# Snapshot Read - Engine Read Path

## Task 1: Store Largest Timestamp in SST

## Task 2: Recover Commit Timestamp

## Task 3: Lsm Iterator with Read Timestamp

## Task 4: Multi-Version Scan and Get

For now, inner = `Fused<LsmIterator>`, do not use `TxnLocalIterator`

explain why store txn inside iterator

do not implement put and delete

## Test Your Understanding

* So far, we have assumed that our SST files use a monotonically increasing id as the file name. Is it okay to use `<level>_<begin_key>_<end_key>_<max_ts>.sst` as the SST file name? What might be the potential problems with that?
* Consider an alternative implementation of transaction/snapshot. In our implementation, we have `read_ts` in our iterators and transaction context, so that the user can always access a consistent view of one version of the database based on the timestamp. Is it viable to store the current LSM state directly in the transaction context in order to gain a consistent snapshot? (i.e., all SST ids, their level information, and all memtables + ts) What are the pros/cons with that? What if the engine does not have memtables? What if the engine is running on a distributed storage system like S3 object store?

We do not provide reference answers to the questions, and feel free to discuss about them in the Discord community.


{{#include copyright.md}}
