<!--
  mini-lsm-book © 2022-2026 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Agent Fast Forward in 3 Days (WIP)

This is an alternative course track for students who intend to use a coding agent. Instead of spending seven chapters on each original course week, you will use one focused day to specify, generate, review, and challenge that week's system.

The agent may write most of the code. Your job is to define what correct means, constrain the work, inspect the result, and leave with a mental model you can use without the agent.

| Fast-forward day | Original course material | Outcome |
| --- | --- | --- |
| [Day 1](./week1-fast-forward.md) | Week 1: Mini-LSM | A working storage engine with memtables, SSTs, reads, writes, and flushes. |
| Day 2 | Week 2: Compaction and persistence | Coming later. |
| Day 3 | Week 3: MVCC | Coming later. |

The original chapters remain a reference library. This track changes the pacing and the student's role; it does not remove the need to understand ordering, representation, concurrency, and failure modes.

## Prepare the Repository and the Agent

Do all repository-wide preparation before beginning Day 1, then start the agent from the starter directory—not from the repository root.

### 1. Install the Toolchain and Course Tools

Install Rust with [rustup](https://rustup.rs) if it is not already available. Then clone the repository and install the tools used by the course:

```shell
git clone https://github.com/skyzh/mini-lsm
cd mini-lsm
cargo x install-tools
```

The repository pins its Rust toolchain in `rust-toolchain.toml`, so Cargo will select it automatically when Rust is managed by `rustup`.

If you already have the repository and tools, update your checkout as appropriate and begin from the repository root.

### 2. Copy the Complete Week 1 Test Suite

The normal course reveals tests one chapter at a time. Day 1 of the fast-forward track starts with the complete Week 1 acceptance suite:

```shell
for day in 1 2 3 4 5 6 7; do
  cargo x copy-test --week 1 --day "$day"
done
cargo x scheck
```

The initial check should fail because the starter contains unfinished code. Record the first failure; it gives you a reproducible baseline. Do not ask the agent to make this failure disappear by changing the tests.

### 3. Start the Agent from `mini-lsm-starter`

Change into the starter directory before launching your coding agent:

```shell
cd mini-lsm-starter
pwd
# Start your coding agent here using the command for your tool.
```

The final component of `pwd` should be `mini-lsm-starter`. This matters for two reasons:

1. repository-aware agents discover the `AGENTS.md` in this directory and apply its learning constraints; and
2. the agent begins with the starter as its working scope instead of treating the neighboring reference implementation as ordinary project context.

Starting in this directory is not a security sandbox: an agent can still traverse to a parent directory if instructed. The local `AGENTS.md` therefore explicitly prohibits reading, searching, diffing, or copying `../mini-lsm/`, including attempts to reconstruct the solution through Git history or an online copy.

Do not open the whole repository as the agent's workspace if your tool lets you choose a directory. Open `mini-lsm-starter`. The agent may consult the copied tests, starter interfaces, Rust documentation, and course chapters under `../mini-lsm-book/src/`.

### 4. Verify the Instructions Before Coding

Do not assume the tool discovered `AGENTS.md`. Make the first prompt a handshake that performs no implementation:

> Before editing anything, confirm that your working directory is `mini-lsm-starter` and read `./AGENTS.md`. Summarize its hard boundaries and working agreement. You must never inspect or copy the reference solution in `../mini-lsm`, directly or indirectly. Tell me which local sources you are allowed to use, then stop without changing files.

If the response omits the reference-solution boundary, test protection, or checkpoint stops, correct the agent before continuing. If the tool cannot load repository instructions automatically, paste the contents of `AGENTS.md` into its persistent project instructions.

## Prompt the Agent in Reviewable Steps

A useful prompt states the scope, invariant, evidence, and stopping point. “Implement everything and make the tests pass” gives the agent no reason to expose its assumptions and gives you no natural place to inspect them.

Use three kinds of prompts throughout the fast-forward track.

### Prompt 1: Ask for a Model, Not Code

Each day begins with a task-specific kickoff prompt. Ask the agent to map the system, state its invariants, divide the work into checkpoints, identify ambiguities, and ask you to predict a boundary case before it edits anything. The day page provides the concrete prompt.

Answer the prediction before asking the agent to evaluate it. This turns the first exchange into a check of your current model rather than a generated summary to skim.

### Prompt 2: Implement One Checkpoint

Use a fresh prompt for each checkpoint. You choose the checkpoint; the agent must not infer that passing one checkpoint authorizes it to begin the next:

> Implement only Checkpoint `<number and name>`. Before editing, restate the invariants for this checkpoint and list the files you expect to change. Keep the diff focused and do not modify supplied tests, public interfaces, or unrelated code.
>
> Run focused checks while working. When the checkpoint is implemented, stop and report the changed behavior, the exact commands and results, one remaining uncertainty, and one adversarial case that I should predict. Do not continue to the next checkpoint.

A checkpoint may require several internal iterations, but it should produce one coherent diff that you can review before the next subsystem depends on it.

### Prompt 3: Challenge the Result

After inspecting the diff and answering the agent's boundary question, ask for evidence rather than reassurance:

> Review this checkpoint as an untrusted contribution. Connect each changed behavior to an invariant and a supplied test. Identify one plausible bug that could still pass those tests, propose the smallest additional test or manual check that exposes it, and wait for my approval before adding that test. If you find a real problem, explain the failing invariant before changing the implementation.

Do not let “all tests pass” end the review. Conversely, do not ask the agent to invent speculative refactors once the checkpoint's contract and adversarial checks are satisfied.

Repeat Prompts 2 and 3 for each checkpoint. The checkpoint stops are where you catch a locally reasonable decision before it spreads across the system.

When the workspace is prepared and the instruction handshake succeeds, continue to [Day 1: Week 1 — Mini-LSM](./week1-fast-forward.md).

{{#include copyright.md}}
