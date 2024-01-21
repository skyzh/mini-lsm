# Sorted String Table (SST)

![Chapter Overview](./lsm-tutorial/week1-04-overview.svg)

In this chapter, you will:

* Implement SST encoding and metadata encoding.
* Implement SST decoding and iterator.
  
## Task 1: SST Builder

## Task 2: SST Iterator

## Task 3: Block Cache

## Test Your Understanding

* What is the time complexity of seeking a key in the SST?
* Where does the cursor stop when you seek a non-existent key in your implementation?
* Is it possible (or necessary) to do in-place updates of SST files?
* An SST is usually large (i.e., 256MB). In this case, the cost of copying/expanding the `Vec` would be significant. Does your implementation allocate enough space for your SST builder in advance? How did you implement it?
* Looking at the `moka` block cache, why does it return `Arc<Error>` instead of the original `Error`?
* Does the usage of a block cache guarantee that there will be at most a fixed number of blocks in memory? For example, if you have a `moka` block cache of 4GB and block size of 4KB, will there be more than 4GB/4KB number of blocks in memory at the same time?
* Is it possible to store columnar data (i.e., a table of 100 integer columns) in an LSM engine? Is the current SST format still a good choice?
* Consider the case that the LSM engine is built on object store services (i.e., S3). How would you optimize/change the SST format/parameters and the block cache to make it suitable for such services?

We do not provide reference answers to the questions, and feel free to discuss about them in the Discord community.

## Bonus Tasks

* **Explore different SST encoding and layout.** For example, in the [Lethe](https://disc-projects.bu.edu/lethe/) paper, the author adds secondary key support to SST. Or you can use B+ Tree as the SST format instead of sorted blocks.
* **Index Blocks.**
* **Index Cache.**

{{#include copyright.md}}
