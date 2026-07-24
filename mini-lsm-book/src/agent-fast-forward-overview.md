<!--
  mini-lsm-book © 2022-2026 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Mini-LSM with Coding Agents (WIP)

This is a three-day guided track for students who intend to use a coding agent. The agent will write much of the code, but it must not silently design the system for you. You will reason about concrete examples, the agent will turn a few accepted decisions into a small code change, and tests will challenge your shared model.

The goal is not to finish with the fewest prompts. It is to finish able to explain, test, and change the system the agent helped you build.

| Guided day | Original course material | Outcome |
| --- | --- | --- |
| [Day 1](./week1-fast-forward.md) | Mini-LSM | A working storage engine with memtables, SSTs, reads, writes, and flushes. |
| Day 2 | Compaction and persistence | Coming later. |
| Day 3 | MVCC | Coming later. |

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

### 2. Start the Agent from `mini-lsm-starter`

Change into the starter directory before launching your coding agent:

```shell
cd mini-lsm-starter
pwd
# Start your coding agent here using the command for your tool.
```

The final component of `pwd` should be `mini-lsm-starter`. This matters because repository-aware agents discover the `AGENTS.md` in this directory and begin with the starter as their working scope.

Starting there is not a security sandbox: an agent can still traverse to a parent directory if instructed. The local `AGENTS.md` therefore prohibits reading, searching, diffing, or copying `../mini-lsm`, including attempts to reconstruct the solution through Git history or an online copy.

Do not open the whole repository as the agent's workspace if your tool lets you choose a directory. The agent may consult copied tests, starter interfaces, Rust documentation, and course chapters under `../mini-lsm-book/src/`.

### 3. Verify the Instructions Before Coding

Do not assume the tool discovered `AGENTS.md`. Make the first prompt a handshake that performs no implementation:

> Before editing anything, confirm that your working directory is `mini-lsm-starter` and read `./AGENTS.md`. Summarize its reference-solution boundary, test protections, and student-owned design protocol. Explain which choices require a stop and which mechanical coding choices do not. Tell me which local sources you may use, then stop without changing files.

If the response omits the reference-solution boundary, test protection, or one-decision-at-a-time stop, correct the agent before continuing. If the tool cannot load repository instructions automatically, paste `AGENTS.md` into its persistent project instructions.

## The Design-and-Test Loop

Begin a checkpoint with an ordinary capability request:

> Implement block format.

That prompt authorizes the learning process, not a one-shot patch. The agent should inspect allowed context and ask its first design question. It should not return a complete design, edit code, or run ahead to a passing suite.

Repeat this loop:

1. **Agent gives one short stop.** It starts with a concrete example, labels the question **Course rule** or **Your choice**, and asks you to choose or predict in plain English.
2. **Student reasons.** State a choice and why. A prediction is useful even when you are uncertain.
3. **Agent checks the reasoning.** It connects the answer to interfaces, prose, or tests. If the answer violates a constraint, it shows the evidence and asks again instead of silently overriding you.
4. **Agent records the choice.** The accepted answer enters a short decision ledger.
5. **Agent implements one slice.** Once enough decisions specify a coherent slice, it previews the files, behavior, and focused test and waits for your authorization.
6. **Tests produce evidence.** A coding mistake can be fixed directly. A failure that exposes an unsettled or incorrect design returns to the dialogue.
7. **Student reviews.** Inspect the diff and test output before authorizing the next slice.

The agent must stop on topics that affect behavior or understanding: representation, ordering, ownership, size and boundary accounting, seek behavior, errors, synchronization, and where an optimization belongs. Some answers are course rules to derive; others are genuine choices. It need not interrupt you over a local variable name, import order, formatting, or an obvious compiler-directed repair.

The distinction keeps the conversation educational without turning every keystroke into ceremony.

At any stop, you can answer with one of these commands:

- `simpler` — ask the same question with shorter sentences and less terminology;
- `example` — show the smallest concrete example that exposes the behavior;
- `hint` — reveal one useful fact, then let you try again; or
- `choose for me` — let the agent decide this one, explain why, and record it as delegated.

Here is the intended shape of a stop:

> **Course rule — Which value should be visible?**
>
> We write `cat = old`, freeze that memory table, then write `cat = new`.
>
> What should `get("cat")` return, and why?
>
> You can reply `simpler`, `example`, `hint`, or `choose for me`.

After you answer, the agent can name the general rule—here, newest-value precedence—and connect it to the implementation. A question should teach the terminology, not require you to decode it before you can begin.

## Keep a Decision Ledger

Ask the agent to maintain this table during each checkpoint:

| Decision or constraint | Student's conclusion | Invariant or evidence | Consequence |
| --- | --- | --- | --- |
| Example: exact-size block | Accept it | The limit is inclusive | Reject only when projected size is greater than the target. |

The ledger makes hidden assumptions reviewable. It also prevents a required course constraint from being presented as a free preference, and lets you distinguish a coding bug from code that faithfully implements a bad decision.

You can delegate a decision when it is not where you want to spend learning time:

> Choose this one for me, explain the tradeoff, and record it as delegated. Continue asking me about the remaining decisions.

Delegating one choice is not permission for the agent to decide the rest.

## Review a Code Slice

Before each edit, the agent should state:

- which accepted decisions determine the slice;
- which files and observable behavior will change; and
- which focused check it expects to exercise that behavior.

After the edit, require the exact command and result, then ask:

> Treat this slice as untrusted. Connect the changed behavior to the decision ledger and supplied tests. Identify one plausible bug that could still pass, propose the smallest adversarial check, and ask me to predict its outcome before adding it.

The agent should also point to one important changed line and ask: “What is this line trying to do, and what behavior might break if it changed?” The purpose is to connect a decision to code, not to quiz you on arbitrary syntax.

Do not let “all tests pass” end the review. Conversely, once the contract and adversarial checks are satisfied, continue to the next unresolved decision instead of inventing unrelated refactors.

When the workspace is prepared and the instruction handshake succeeds, continue to [Day 1 - Mini-LSM](./week1-fast-forward.md).

{{#include copyright.md}}
