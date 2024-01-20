# Simple Compaction Strategy

![Chapter Overview](./lsm-tutorial/week2-01-overview.svg)

In this chapter, you will:

* Implement a simple leveled compaction strategy and simulate it on the compaction simulator.
* Start compaction as a background task and implement a compaction trigger in the system.

## Test Your Understanding

* Is it correct that a key will only be purged from the LSM tree if the user requests to delete it and it has been compacted in the bottom-most level?
* Is it a good strategy to periodically do a full compaction on the LSM tree? Why or why not?
* Actively choosing some old files/levels to compact even if they do not violate the level amplifier would be a good choice, is it true? (Look at the Lethe paper!)
* If the storage device can achieve a sustainable 1GB/s write throughput and the write amplification of the LSM tree is 10x, how much throughput can the user get from the LSM key-value interfaces?

* What is your favorite boba shop in your city? (If you answered yes in week 1 day 3...)

We do not provide reference answers to the questions, and feel free to discuss about them in the Discord community.

{{#include copyright.md}}
