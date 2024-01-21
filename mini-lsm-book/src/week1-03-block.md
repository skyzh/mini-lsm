# Block

![Chapter Overview](./lsm-tutorial/week1-03-overview.svg)

In this chapter, you will:

* Implement SST block encoding.
* Implement SST block decoding and block iterator.

## Task 1: Block Builder

## Task 2: Block Iterator

## Test Your Understanding

* What is the time complexity of seeking a key in the block?
* Where does the cursor stop when you seek a non-existent key in your implementation?
* So `Block` is simply a vector of raw data and a vector of offsets. Can we change them to `Byte` and `Arc<[u16]>`, and change all the iterator interfaces to return `Byte` instead of `&[u8]`? (Assume that we use `Byte::slice` to return a slice of the block without copying.) What are the pros/cons?
* What is the endian of the numbers written into the blocks in your implementation?
* Is your implementation prune to a maliciously-built block? Will there be invalid memory access, or OOMs, if a user deliberately construct an invalid block?
* Can a block contain duplicated keys?
* What happens if the user adds a key larger than the target block size?
* Consider the case that the LSM engine is built on object store services (S3). How would you optimize/change the block format and parameters to make it suitable for such services?
* Do you love bubble tea? Why or why not?

We do not provide reference answers to the questions, and feel free to discuss about them in the Discord community.

## Bonus Tasks

* **Backward Iterators.**

{{#include copyright.md}}
