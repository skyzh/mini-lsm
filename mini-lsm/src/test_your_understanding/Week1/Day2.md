### Test your understanding

#### LSM-specific

- What is the time/space complexity of using your merge iterator?
  - Merge iterator is an iterator over the Memtable iterators. Memtable iterator is an iterator over SkipMap. Calling next() on SkipMap and
  hence the Memtable is amortized to O(1). Calling next() on Merged iterators is O(log(M)), as it maintains a binary heap of all memtables (M). So merged iterator time complexity will be O(N*log(M)), where N is the total number of entries across all memtables, in the worst case. The space complexity of the merged iterator will be equivalent to the number of memtables, because the iterator itself just stores the reference
  to individual memtable iterators. 

- If a key is removed (there is a delete tombstone), do you need to return it to the user? Where did you handle this logic?
  - No, we don't have to return it. We are returning a None for non-existent keys from lsm_storage where it's handled. Memtable
  stores an empty value for deleted entries so just returns that and isn't handled there.

- If a key has multiple versions, will the user see all of them? Where did you handle this logic?
  - If a key has multiple versions, our merged iterator's next() method handles it as it skips over the key for older memtables.

- What happens if your key comparator cannot give the binary heap implementation a stable order?
  - We may return old and incorrect values for a key.

- Why do we need to ensure the merge iterator returns data in the iterator construction order?
  - The iterator construction order specifies the "age" of memtables, where the earliest element is the latest.

#### Rust-specific

- Why do we need a self-referential structure for memtable iterator?
  - Because we want the iterator to outlive the underlying Memtable or SkipMap. Without a self-referential structure, the borrow checker
  will complain about lifetime errors, as it can't determine whether the iterator lives long enough.

- Is it possible to implement a Rust-style iterator (i.e., next(&self) -> (Key, Value)) for LSM iterators? What are the pros/cons?
  The scan interface is like fn scan(&self, lower: Bound<&[u8]>, upper: Bound<&[u8]>). How to make this API compatible with Rust-style range (i.e., key_a..key_b)? If you implement this, try to pass a full range .. to the interface and see what will happen.
  The starter code provides the merge iter

- If we want to get rid of self-referential structure and have a lifetime on the memtable iterator (i.e., MemtableIterator<'a>, where 'a =        memtable or LsmStorageInner lifetime), is it still possible to implement the scan functionality?
  What happens if (1) we create an iterator on the skiplist memtable (2) someone inserts new keys into the memtable (3) will the iterator see the new key?
  - <Unanswered>