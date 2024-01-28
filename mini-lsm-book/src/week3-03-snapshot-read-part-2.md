# Snapshot Read - Engine Read Path and Transaction API

In this chapter, you will:

* Finish the read path based on previous chapter to support snapshot read.
* Implement the transaction API to support snapshot read.
* Implement the engine recovery process to correctly recover the commit timestamp.

At the end of the day, your engine will be able to give the user a consistent view of the storage key space.

## Task 1: Lsm Iterator with Read Timestamp

## Task 2: Multi-Version Scan and Get

For now, inner = `Fused<LsmIterator>`, do not use `TxnLocalIterator`

explain why store txn inside iterator

do not implement put and delete

## Task 3: Store Largest Timestamp in SST

## Task 4: Recover Commit Timestamp

## Test Your Understanding

* So far, we have assumed that our SST files use a monotonically increasing id as the file name. Is it okay to use `<level>_<begin_key>_<end_key>_<max_ts>.sst` as the SST file name? What might be the potential problems with that?
* Consider an alternative implementation of transaction/snapshot. In our implementation, we have `read_ts` in our iterators and transaction context, so that the user can always access a consistent view of one version of the database based on the timestamp. Is it viable to store the current LSM state directly in the transaction context in order to gain a consistent snapshot? (i.e., all SST ids, their level information, and all memtables + ts) What are the pros/cons with that? What if the engine does not have memtables? What if the engine is running on a distributed storage system like S3 object store?

We do not provide reference answers to the questions, and feel free to discuss about them in the Discord community.


{{#include copyright.md}}
