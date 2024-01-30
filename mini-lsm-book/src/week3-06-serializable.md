# Serializable Snapshot Isolation

Now, we are going to add a conflict detection algorithm at the transaction commit time, so as to make the engine serializable.

## Task 1: Track Read Set in Get and Write Set

## Task 2: Track Read Set in Scan

## Task 3: Serializable Verification

## Test Your Understanding

* If you have some experience with building a relational database, you may think about the following question: assume that we build a database based on Mini-LSM where we store each row in the relation table as a key-value pair (key: primary key, value: serialized row) and enable serializable verification, does the database system directly gain ANSI serializable isolation level capability? Why or why not?

We do not provide reference answers to the questions, and feel free to discuss about them in the Discord community.

## Bonus Tasks

* **Read-Only Transactions.** With serializable enabled, we will need to keep track of the read set for a transaction.
* **Precision/Predicate Locking.** The read set can be maintained using a range instead of a single key. This would be useful when a user scans the full key space.

{{#include copyright.md}}
