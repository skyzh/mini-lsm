## Test Your Understanding
### Why doesn't the memtable provide a delete API?
TODO how skiplist deletion work? what is the complexity?

### Is it possible to use other data structures as the memtable in LSM? What are the pros/cons of using the skiplist?
Any sorted order data structure can be used, for example a tree map

### Why do we need a combination of state and state_lock? Can we only use state.read() and state.write()?
`state_lock` protects the lsm structure while mutating the lsm state. There could be other background activities that
race with the memtable operations (sst, compaction etc.)

### Why does the order to store and to probe the memtables matter? If a key appears in multiple memtables, which version should you return to the user?
Memtables are store in a time order. For the current memtable the key is overwritten each time. For the immutable ones,
the latest value is the one that is in in front of the lists.

### Is the memory layout of the memtable efficient / does it have good data locality? (Think of how Byte is implemented and stored in the skiplist...) What are the possible optimizations to make the memtable more efficient?
TODO

### So we are using parking_lot locks in this tutorial. Is its read-write lock a fair lock? What might happen to the readers trying to acquire the lock if there is one writer waiting for existing readers to stop?
TODO (cf locks book) 

### After freezing the memtable, is it possible that some threads still hold the old LSM state and wrote into these immutable memtables? How does your solution prevent it from happening?
This should not be possible. For writes we need to hold the read lock, and this cannot be done during the freezing because
the write lock is held. After the freeze the read lock guard should point to the new memtable. 

### There are several places that you might first acquire a read lock on state, then drop it and acquire a write lock (these two operations might be in different functions but they happened sequentially due to one function calls the other). How does it differ from directly upgrading the read lock to a write lock? Is it necessary to upgrade instead of acquiring and dropping and what is the cost of doing the upgrade?
Hmm
