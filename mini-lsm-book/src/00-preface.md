# Preface

![Tutorial Overview](lsm-tutorial/00-full-overview.svg)

In this tutorial, you will learn how to build a simple LSM-Tree storage engine in the Rust programming language.

## What is LSM, and Why LSM?

Log-structured merge tree is a data structure to maintain key-value pairs. This data structure is widely used in
distributed database systems like [TiDB](https://www.pingcap.com) and [CockroachDB](https://www.cockroachlabs.com) as
their underlying storage engine. [RocksDB](http://rocksdb.org), based on [LevelDB](https://github.com/google/leveldb),
is an implementation of LSM-Tree storage engine. It provides a wide range of key-value access functionalities and is
used in a lot of production systems.

Generally speaking, LSM Tree is an append-friendly data structure. It is more intuitive to compare LSM to other
key-value data structure like RB-Tree and B-Tree. For RB-Tree and B-Tree, all data operations are in-place. That is to
say, when you update the value corresponding to the key, the value will be overwritten at its original memory or disk
space. But in an LSM Tree, all write operations, i.e., insertions, updates, deletions, are performed in somewhere else.
These operations will be batched into SST (sorted string table) files and be written to the disk. Once written to the
disk, the file will not be changed. These operations are applied lazily on disk with a special task called compaction.
The compaction job will merge multiple SST files and remove unused data.

This architectural design makes LSM tree easy to work with.

1. Data are immutable on persistent storage, which means that it is easier to offload the background tasks (compaction)
   to remote servers. It is also feasible to directly store and serve data from cloud-native storage systems like S3.
2. An LSM tree can balance between read, write and space amplification by changing the compaction algorithm. The data
   structure itself is super versatile and can be optimized for different workloads.

In this tutorial, we will learn how to build an LSM-Tree-based storage engine in the Rust programming language.

## Prerequisites

* You should know the basics of the Rust programming language. Reading [the Rust book](https://doc.rust-lang.org/book/)
  is enough.
* You should know the basic concepts of key-value storage engines, i.e., why we need somehow complex design to achieve
  persistence. If you have no experience with database systems and storage systems before, you can implement Bitcask
  in [PingCAP Talent Plan](https://github.com/pingcap/talent-plan/tree/master/courses/rust/projects/project-2).
* Knowing the basics of an LSM tree is not a requirement but we recommend you to read something about it, e.g., the 
  overall idea of LevelDB. This would familiarize you with concepts like mutable and immutable mem-tables, SST,
  compaction, WAL, etc.

## What should you expect from this tutorial...

After learning this course, you should have a deep understanding of how a LSM-based storage system works, gain hands-on experience of designing such systems, and apply what you have learned in your study and career. You will understand the design tradeoffs in such storage systems and find optimal ways to design a LSM-based storage system to meet your workload requirements/goals. This is a very in-depth tutorial that covers all the important implementation details and design choices of modern storage systems (i.e., RocksDB) based on the author's experience in several LSM-like storage systems, and you will be able to directly apply what you have learned in both industry and academia.

### Structure

The tutorial is a large course that is split into several parts (weeks). Each week usually has seven chapters, and each of the chapter can be finished within 2-3 hours. The first six chapters of each part will instruct you to build a working system, and the last chapter of each week will be a *snack time* chapter that implements some easy things over what you have built in the previous six days. In each chapter, there will be required tasks, *check you understanding* questions, and bonus tasks.

### Testing

We provide full test suite and some cli tools for you to validate if your solution is correct. Note that the test suite is not exhaustive, and your solution might not be 100% correct after passing all test cases. You might need to fix earlier bugs when implementing later parts of the system. We recommend you to think thoroughly about your implementation, especially when there are multi-thread operations and race conditions.

### Solution

We have a solution that implements all the functionalities as required in the tutorial in the mini-lsm main repo. At the same time, we also have a mini-lsm solution checkpoint repo where each commit corresponds to a chapter in the tutorial. 

Keeping such checkpoint repo up-to-date to the mini-lsm tutorial is hard because each bug fix or new feature will need to go through all commits (or checkpoints). Therefore, this repo might not be using the latest starter code or incorporating the latest features from the mini-lsm tutorial.

**TL;DR: We do not guarantee the solution checkpoint repo contains a correct solution, passes all tests, or has the correct doc comments.** For a correct implementation and the solution after implementing all things, please take a look at the solution in the main repo instead. [https://github.com/skyzh/mini-lsm/tree/main/mini-lsm](https://github.com/skyzh/mini-lsm/tree/main/mini-lsm).

If you are stuck at some part of the tutorial or do not know where to implement a functionality, you can refer to this repo for help. You may compare the diff between commits to know what has been changed. Some functions in the mini-lsm tutorial might be changed multiple times throughout the chapters, and you can know what exactly are expected to be implemented for each chapter in this repo.

You may access the solution checkpoint repo at [https://github.com/skyzh/mini-lsm-solution-checkpoint](https://github.com/skyzh/mini-lsm-solution-checkpoint).

### Feedbacks

Your feedback is greatly appreciated. We have rewritten the whole course from scratch in 2024 based on the feedbacks from the students. We hope you can share your learning experience and help us continuously improve the tutorial. Welcome to the [Discord community](https://skyzh.dev/join/discord) and share your experience.

The long story of why we rewrote it: The tutorial was originally planned as a general guidance that students start from an empty directory and implement whatever they want based on the specification we had. We had minimal tests that checks if the behavior is correct. However, the original tutorial is too open-ended that caused huge obstacles with the learning experience. As students do not have an overview of the whole system beforehand and the instructions are kind of vague, sometimes it is hard for the students to know why a design decision is made and what they need to achieve a goal. And some part of the course is too compact that it is impossible to deliver expected contents within just one chapter. Therefore, we completely redesigned the course to have a easier learning curve and clearer learning goals. The original one-week tutorial is now split into two weeks (first week on storage format, and second week on deep-dive compaction), with an extra part on MVCC. We hope you find this course interesting and helpful in your study and career. We would like to thank everyone who commented in [Feedback after coding day 1](https://github.com/skyzh/mini-lsm/issues/11) and [Hello, when is the next update plan for the tutorial?](https://github.com/skyzh/mini-lsm/issues/7) -- your feedback greatly helped us improve the course.

### License

The source code of this course is licensed under Apache 2.0, while the author owns the full copyright of the tutorial itself (markdown files + figures).

### Will this tutorial be free forever?

Yes! Everything publicly available now will be free forever and will receive lifetime updates and bug fixes. Meanwhile, we might provide paid code review and office hour services in the future. For the DLC part (*rest of your life* chapters), we do not have plans to finish them as of 2024, and have not decided whether they will be public available or not.

## Community

You may join skyzh's Discord server and study with the mini-lsm community.

[![Join skyzh's Discord Server](https://dcbadge.vercel.app/api/server/ZgXzxpua3H)](https://skyzh.dev/join/discord)

## Get Started

Now, you may go ahead and get an overview of the LSM structure in [Mini-LSM Course Overview](./00-overview.md).

## About the Author

As of writing (at the beginning of 2024), Chi obtained his master's degree in Computer Science from Carnegie Mellon University and his bachelor's degree from Shanghai Jiao Tong University. He has been working on a variety of database systems including [TiKV][db1], [AgateDB][db2], [TerarkDB][db3], [RisingWave][db4], and [Neon][db5]. Since 2022, he worked as a teaching assistant for [CMU's Database Systems course](https://15445.courses.cs.cmu) for three semesters on the BusTub educational system, where he added a lot of new features and more challenges to the course (check out the re-designed [query execution](https://15445.courses.cs.cmu.edu/fall2022/project3/) project and the super challenging [multi-version concurrency control](https://15445.courses.cs.cmu.edu/fall2023/project4/) project). Besides working on the BusTub educational system, he is also a maintainer of the [RisingLight](https://github.com/risinglightdb/risinglight) educational database system. Chi is interested in exploring how the Rust programming language can fit in the database world. Check out his previous tutorial on building a vectorized expression framework [type-exercise-in-rust](https://github.com/skyzh/type-exercise-in-rust) and on building a vector database [write-you-a-vector-db](https://github.com/skyzh/write-you-a-vector-db) if you are also interested in that topic.

[db1]: https://github.com/tikv/tikv
[db2]: https://github.com/tikv/agatedb
[db3]: https://github.com/bytedance/terarkdb
[db4]: https://github.com/risingwavelabs/risingwave
[db5]: https://github.com/neondatabase/neon

{{#include copyright.md}}
