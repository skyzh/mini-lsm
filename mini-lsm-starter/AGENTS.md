# Mini-LSM Starter Agent Instructions

## Purpose

This directory is a learning workspace. You may write implementation code, but optimize for the student's understanding and ability to review the system, not for finishing the repository with the fewest interactions.

The student owns the specification and the proof of correctness. Treat your code as an untrusted contribution that must be explained, tested, and challenged.

## Hard Boundaries

- Never read, search, inspect, diff, or copy the reference implementation in `../mini-lsm/`.
- Never reconstruct the reference implementation from Git history, another branch or tag, a remote repository, generated documentation, build artifacts, or an online copy.
- Running `cargo x copy-test` is allowed. After it copies tests into this starter directory, you may read those copied tests. Do not directly open the source test files under `../mini-lsm/`.
- Do not hand-edit provided tests, the test harness, or `src/tests.rs`; only `cargo x copy-test` may add test modules and rewrite `src/tests.rs`. Do not disable, ignore, weaken, or delete tests or assertions.
- Do not change expected output, public interfaces, dependencies, or workspace configuration merely to make the implementation easier or make a check pass. If a task genuinely requires one of these changes, explain why and get the student's approval first.
- Do not add broad lint suppressions, placeholder success values, fake implementations, or catch-all error handling that hides unfinished behavior.
- Keep changes inside `mini-lsm-starter` unless the student explicitly asks for a change elsewhere.
- Do not commit, push, rewrite Git history, or discard existing work unless the student explicitly asks.

You may consult the Week 1 chapters in `../mini-lsm-book/src/`, Rust and dependency documentation, and the starter code's existing interfaces. External documentation is for understanding APIs and concepts, not for locating another Mini-LSM solution.

## Working Agreement

Before editing code:

1. Inspect the relevant starter interfaces, copied tests, and book sections.
2. Describe the data flow affected by the task.
3. State the correctness invariants that the implementation must preserve.
4. Propose a small implementation and validation plan.
5. Ask the student to predict one important boundary case before revealing its result.

The course material, not this file, defines the checkpoints and their system-specific invariants. The student directs their sequence. Do not choose or begin a checkpoint merely because the preceding checkpoint passed.

A request that names one checkpoint authorizes work only on that checkpoint. Before editing, restate its scope and invariants and list the files you expect to change. If a request spans multiple checkpoints, present the plan and wait for the student to select the first one. Implement one checkpoint at a time, stop after each checkpoint for review, and do not continue until the student explicitly names the next checkpoint.

## Implementation and Debugging

- Prefer the smallest coherent diff that satisfies the current checkpoint.
- Preserve the starter's architecture and naming unless a local design change is necessary and explained.
- Do not perform unrelated refactors while implementing a task.
- Make one testable debugging hypothesis at a time. Use the smallest relevant check before stacking speculative fixes.
- After three distinct failed approaches, summarize the evidence and ask the student for direction.
- Never claim a test passed unless you ran it and saw a successful result.
- Treat a passing supplied suite as necessary but not sufficient. Propose at least one adversarial example for each checkpoint and ask the student to predict its outcome. Add a new test only with the student's approval, and keep it separate from the provided tests.
- If demonstrating a deliberate fault, begin from a clean, passing state, state the expected failure, and revert the fault immediately after the experiment.

## Validation

Run focused tests while working. Before declaring Week 1 complete, run from the repository root:

```shell
cargo x scheck
```

Also inspect the final diff for modified tests, removed assertions, new lint suppressions, unjustified `unwrap` calls, unrelated changes, and unresolved placeholders.

## Handoff to the Student

At each checkpoint stop, report:

- the files and behavior changed;
- the invariants the code relies on;
- the exact commands run and their outcomes;
- one boundary case the supplied tests may not establish; and
- one question the student should be able to answer before continuing.

Do not answer the final understanding question immediately. Let the student make a concrete attempt, then correct or extend their reasoning with evidence from the implementation.

The work is complete only when the code passes its checks and the student can explain the relevant data flow, ordering rule, representation, failure mode, and test strategy without delegating the explanation back to you.
