### Test your understanding

#### LSM-specific

- What is the time complexity of seeking a key in the block?
  - It's O(log(n)) where n is the number of keys/entries in the block.

- Where does the cursor stop when you seek a non-existent key in your implementation?
  - It stops at the key greater than the non-existent key. If the seeked key is greatest,
    it will stop at the last element. 

- What is the endian of the numbers written into the blocks in your implementation?
  - Bytes `put_u16` stores data in Big endian format.

- Is your implementation prune to a maliciously-built block? Will there be invalid memory access, or OOMs, if a user deliberately construct an invalid block?
  - We check for buffer overflows and invalid offsets to counter these issues. 
  - We check for block sizes before adding to prevent OOMs.

- Can a block contain duplicated keys?
  - Well, a block can. But if the block is always constructed from Memtable and merge iterators, then the merge iterator
  will handle skipping the duplicate keys. 

- What happens if the user adds a key larger than the target block size?
  - We don't accept the write and send a `false` back to the user.

- Consider the case that the LSM engine is built on object store services (S3). How would you optimize/change the block format and parameters to make it suitable for such services?
  - The target block size will be larger for object store services as most objects are large. We can avoid syscall and network overhead with a
    larger block size.
  - Since object stores are likely to span multiple blocks for each object, maybe the offsets/data split has more overhead. However, we still need
    to handle small objects, so for now I'd keep the format as it is.
  - We may want to also implement some compression to reduce transfer sizes.  

- Do you love bubble tea? Why or why not?
  - Haha, I love the ones that are a little less sweet :-)

#### Rust-specific

- So Block is simply a vector of raw data and a vector of offsets. Can we change them to Byte and Arc<[u16]>, and change all the iterator interfaces to return Byte instead of &[u8]? (Assume that we use Byte::slice to return a slice of the block without copying.) What are the pros/cons?
  - <Unanswered>