# Compaction Implementation

![Chapter Overview](./lsm-tutorial/week2-01-overview.svg)

In this chapter, you will:

* Implement the compaction logic that combines some files and produces new files.
* Implement the logic to update the LSM states and manage SST files on the filesystem.
* Update LSM read path to incorporate the LSM levels.

## Test Your Understanding

* Is it correct that a key will take some storage space even if a user requests to delete it?
* Given that compaction takes a lot of write bandwidth and read bandwidth and may interfere with foreground operations, it is a good idea to postpone compaction when there are large write flow. It is even beneficial to stop/pause existing compaction tasks in this situation. What do you think of this idea? (Read the Slik paper!)

We do not provide reference answers to the questions, and feel free to discuss about them in the Discord community.

{{#include copyright.md}}
