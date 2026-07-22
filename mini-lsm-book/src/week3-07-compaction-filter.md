<!--
  mini-lsm-book © 2022-2026 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Snack Time: Compaction Filters

Congratulations! The engine now supports multi-version transactions. This final chapter generalizes version garbage collection into a user-installed compaction filter.

By the end of this chapter, you will be able to:

* Apply a prefix filter without reclaiming versions that an active snapshot may still need.
* Remove the selected version and older versions of the same user key from one compaction output.
* Explain the intentionally undefined reads for a filtered key while reclamation is still propagating through the LSM tree.

For now, our compaction will simply retain the keys above the watermark and the latest version of the keys below the watermark. We can add some magic to the compaction process to help the user collect some unused data automatically as a background job.

Consider a case that the user uses Mini-LSM to store database tables. Each row in the table are prefixed with the table name. For example,

```
table1_key1 -> row
table1_key2 -> row
table1_key3 -> row
table2_key1 -> row
table2_key2 -> row
```

Now the user executes `DROP TABLE table1`. The engine will need to clean up all the data beginning with `table1`.

There are a lot of ways to achieve the goal. The user of Mini-LSM can scan all the keys beginning with `table1` and requests the engine to delete it. However, scanning a very large database might be slow, and it will generate the same number of delete tombstones as the existing keys. Therefore, scan-and-delete will not free up the space occupied by the dropped table -- instead, it will add more data to the engine and the space can only be reclaimed when the tombstones reach the bottom level of the engine.

Or, they can create column families (we will talk about this in *rest of your life* chapter). They store each table in a column family, which is a standalone LSM state, and directly remove the SST files corresponding to the column family when the user drop the table.

In this course, we implement a third approach: compaction filters. A filter can be installed at runtime. During later compactions, matching keys become eligible for removal without first writing one tombstone per row. A `prefix=table1` filter therefore reclaims table 1 incrementally as compaction visits its files.

## Before You Begin

A compaction filter is a logical deletion policy, not merely a byte predicate. It must interact correctly with MVCC garbage collection.

Keep these invariants in mind:

1. Versions above the watermark remain untouched, even when their user key matches the filter.
2. When the first version at or below the watermark matches, omit it and every older version of that user key from the current compaction output.
3. Non-matching keys follow the ordinary watermark and bottom-level tombstone rules.
4. Files outside the compaction task may still contain older matching versions. Reads in a filtered prefix are therefore intentionally undefined until reclamation has propagated.
5. Installing a filter does not synchronously free space; only compaction rewrites and removes the affected SSTs.

> **Predict before coding:** With watermark 5, a filtered key has `k@8=v8, k@5=v5, k@2=v2`. Which versions survive this compaction? What changes after the watermark advances to 8?

## Task 1: Compaction Filter

In this task, you will need to modify:

```
src/compact.rs
```

Iterate the filters in `LsmStorageInner::compaction_filters`. If the first version of a key at or below the watermark matches, omit it and ensure that older versions of the same key are skipped as well.

To copy and run the test cases:

```
cargo x copy-test --week 3 --day 7
cargo x scheck
```

You may assume that the user will not call `get` or scan within the filtered prefix. Such reads have undefined results because matching versions can disappear from some levels before others.

## Chapter Checkpoint

Compaction should now reclaim filtered prefixes only as quickly as the watermark and selected compaction tasks permit.

Verify these cases explicitly:

1. Hold an old snapshot, install a filter, and compact; versions needed by the snapshot must remain.
2. Drop the snapshot and compact again; the matching version at or below the new watermark and its older history should disappear.
3. Mix matching and non-matching keys, including tombstones, and confirm ordinary garbage collection still applies to the non-matching keys.
4. Apply the filter in a non-bottom compaction and identify any older matching versions that remain outside the task.

## Test Your Understanding

* Why is it unsafe to filter every matching version regardless of timestamp?
* After filtering the first version at or below the watermark, why must compaction also skip older versions of that user key?
* Why are reads inside the filtered prefix undefined before every relevant level has been compacted?
* How would you report progress for a `DROP TABLE` operation backed by an asynchronous compaction filter?
* What API or metadata would be needed to remove or supersede a previously installed filter safely?

We do not provide reference answers to these questions, so feel free to discuss them in the Discord community.

{{#include copyright.md}}
