# Mem Table and Merge Iterators

<!-- toc -->

In this part, you will need to modify:

* `src/iterators/merge_iterator.rs`
* `src/iterators/two_merge_iterator.rs`
* `src/mem_table.rs`

You can use `cargo x copy-test day3` to copy our provided test cases to the starter code directory. After you have
finished this part, use `cargo x scheck` to check the style and run all test cases. If you want to write your own
test cases, write a new module `#[cfg(test)] mod user_tests { /* your test cases */ }` in `table.rs`. Remember to remove
`#![allow(...)]` at the top of the modules you modified so that cargo clippy can actually check the styles.

This is the last part for the basic building blocks of an LSM tree. After implementing the merge iterators, we can
easily merge data from different part of the data structure (mem table + SST) and get an iterator over all data. And
in part 4, we will compose all these things together to make a real storage engine.

## Task 1 - Mem Table

## Task 2 - Mem Table Iterator

## Task 3 - Two-Merge Iterator

## Task 4 - Merge Iterator

## Extra Tasks
