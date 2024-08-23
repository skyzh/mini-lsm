### Test Your Understanding

- Why doesn't the memtable provide a delete API?
  - Because we don't delete from LSM-backed storage in the hot path. We add a "tombstone" entry, and deletion is handled
    later by the compaction process. Memtable simply stores a deleted key as a tombstone entry (either through empty value or explicit markers)

- Is it possible to use other data structures as the memtable in LSM? What are the pros/cons of using the skiplist?
  - We can use B-Tree or AVL tree as well. SkipLists are easier to implement and have good concurrency support. They take higher memory
    and the balancing is probabilistic rather than guaranteed.

- Why do we need a combination of state and state_lock? Can we only use state.read() and state.write()?
  - `state_lock` helps take locks for operations that don't necessasrily involve get/put but other operations. This helps avoid
  contention on the actual client workload while we are say "freezing a memtable".

- Why does the order to store and to probe the memtables matter? If a key appears in multiple memtables, which version should you return to the user?
  - The order to store and probe memtables is directly related to the correctness of the storage engine itself. Memtables earlier in order represent more recent mutations of the client data. If we didn't maintain an order, we'd be returning incorrect values of data when asked by a client.

- Is the memory layout of the memtable efficient / does it have good data locality? (Think of how Byte is implemented and stored in the skiplist...) What are the possible optimizations to make the memtable more efficient?
  - Skip lists' nodes are scattered so the memory layout is not cache friendly. We could do key prefix compression to save memory 
  but I dont know how to yet :-)

- So we are using parking_lot locks in this tutorial. Is its read-write lock a fair lock? What might happen to the readers trying to acquire the lock if there is one writer waiting for existing readers to stop?
  - Not fair. Writer can starve; need to introduce fairness.

- After freezing the memtable, is it possible that some threads still hold the old LSM state and wrote into these immutable memtables? How does your solution prevent it from happening?
  - `state` lock should prevent this because we are taking a write lock while freezing which does the atomic swap of memtables. Need validation.

- There are several places that you might first acquire a read lock on state, then drop it and acquire a write lock (these two operations might be in different functions but they happened sequentially due to one function calls the other). How does it differ from directly upgrading the read lock to a write lock? Is it necessary to upgrade instead of acquiring and dropping and what is the cost of doing the upgrade?
  - I think dropping allows other operations to go through but not sure.