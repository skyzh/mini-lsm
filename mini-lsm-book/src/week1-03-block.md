# Block

![Chapter Overview](./lsm-tutorial/week1-03-overview.svg)

In this chapter, you will:

* Implement SST block encoding.
* Implement SST block decoding and block iterator.

## Task 1: Block Builder

## Task 2: Block Iterator

## Test Your Understanding

* So `Block` is simply a vector of raw data and a vector of offsets. Can we change them to `Byte` and `Arc<[u16]>`, and change all the iterator interfaces to return `Byte` instead of `&[u8]`? What are the pros/cons?
* What is the endian of the numbers written into the blocks in your implementation?
* Is your implementation prune to a maliciously-built block? Will there be invalid memory access, or OOMs, if a user deliberately construct an invalid block?
* Do you love bubble tea? Why or why not?

{{#include copyright.md}}
