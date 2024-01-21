# Leveled Compaction Strategy

![Chapter Overview](./lsm-tutorial/week2-01-overview.svg)

In this chapter, you will:

* Implement a leveled compaction strategy and simulate it on the compaction simulator.
* Incorporate leveled compaction strategy into the system.

## Test Your Understanding

* Finding a good key split point for compaction may potentially reduce the write amplification, or it does not matter at all?
* Imagine that a user was using tiered (universal) compaction before and wants to migrate to leveled compaction. What might be the challenges of this migration? And how to do the migration?
* What if the user wants to migrate from leveled compaction to tiered compaction?
* What needs to be done if a user not using compaction at all decides to migrate to leveled compaction?

We do not provide reference answers to the questions, and feel free to discuss about them in the Discord community.

{{#include copyright.md}}
