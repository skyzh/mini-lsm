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
