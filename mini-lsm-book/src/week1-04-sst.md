# Sorted String Table (SST)

![Chapter Overview](./lsm-tutorial/week1-04-overview.svg)

In this chapter, you will:

* Implement SST encoding and metadata encoding.
* Implement SST decoding and iterator.
  
## Task 1: SST Builder

## Task 2: SST Iterator

## Task 3: Block Cache

## Test Your Understanding

* An SST is usually large (i.e., 256MB). In this case, the cost of copying/expanding the `Vec` would be significant. Does your implementation allocate enough space for your SST builder in advance? How did you implement it?
* Looking at the `moka` block cache, why does it return `Arc<Error>` instead of the original `Error`?

{{#include copyright.md}}
