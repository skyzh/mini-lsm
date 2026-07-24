---
name: validate-coding-agent-track
description: Validate Mini-LSM coding-agent course tracks and checkpoint chapters for technical correctness, executable agent workflow, plain-English learning stops, dependency order, testability, and consistency with the starter interfaces and source chapters. Use when drafting or reviewing agent-fast-forward material, guided coding-agent lessons, related AGENTS.md protocols, or changes to the three-day Mini-LSM agent track.
---

# Validate Coding-Agent Track

Audit the material as instructions that a student and coding agent must actually execute. Check semantics against allowed repository evidence; do not review prose in isolation.

## Workflow

1. Identify the overview, day chapter, `mini-lsm-starter/AGENTS.md`, linked source chapters, and relevant starter interfaces or copied-test tooling.
2. Respect the starter boundary: never inspect or reconstruct the reference implementation in `mini-lsm/`. Read `mini-lsm-book/src/`, `mini-lsm-starter/`, `xtask/`, and copied tests when present.
3. Run `scripts/check_track.py` on the changed overview and day chapters. Resolve structural failures before the semantic audit.
4. Trace the learner workflow from setup through completion. Verify that every command has the correct working directory, every prerequisite precedes its consumer, and every named test or tool exists.
5. Audit each checkpoint using the criteria below.
6. Report findings by severity with exact file and line references. Separate confirmed defects from optional improvements.
7. Fix confirmed defects when authorized, rerun the script and `mdbook build`, then repeat the affected audit sections.

## Checkpoint Criteria

For every checkpoint, verify:

- **Boundary:** The starting state, intended outcome, permitted files, and stopping point form one coherent slice.
- **Coverage:** The material includes the consequential representation, ordering, ownership, boundary, error, concurrency, durability, and optimization decisions relevant to that slice.
- **Classification:** A required course behavior is labeled **Course rule** and derived from evidence. Only genuinely interchangeable implementations are labeled **Your choice**.
- **Question quality:** Stops begin with a small state, byte layout, crash point, or operation in plain English. Necessary terminology is introduced after the student can reason about the example.
- **Escape hatches:** The first stop supports `simpler`, `example`, `hint`, and `choose for me` without treating one delegated answer as blanket delegation.
- **Authorization:** The agent waits until the current slice is specified and authorized, then stops again after implementation and evidence.
- **Evidence:** Focused checks exist for the slice. Passing tests are not treated as the specification, and at least one plausible adversarial case asks for a prediction.
- **Teach-back:** Review connects one consequential line or comparison to the student's decision and asks what would break if it changed.
- **Completion:** The student must explain data flow, invariants, a failure mode, and a test strategy rather than merely obtain a passing suite.

## Cross-Checkpoint Audit

Check the whole day for:

- a default path that does not require unexplained architectural choices at the start;
- ordering that avoids implementing an obsolete intermediate format;
- consistent names, links, commands, and day outcomes across `SUMMARY.md` and the overview;
- explicit handling of concurrent state changes, crashes, or partial failure where relevant;
- no instructions to weaken tests, change public interfaces for convenience, or inspect the reference solution;
- enough specification for an agent to proceed without inventing system behavior, but no answer dump that removes the student's reasoning; and
- a realistic interaction budget: combine facts exposed by one scenario instead of manufacturing a stop for every function or local choice.

## Validation Commands

From the repository root, run:

```shell
python3 .agents/skills/validate-coding-agent-track/scripts/check_track.py \
  mini-lsm-book/src/agent-fast-forward-overview.md \
  mini-lsm-book/src/week1-fast-forward.md
(cd mini-lsm-book && mdbook build)
git diff --check
```

Replace or add day-chapter paths as needed. If `mdbook build` fails only because a serve command cannot bind a port, run the build subcommand directly as shown above.

## Report Format

Lead with a verdict: **ready**, **ready with improvements**, or **not ready**. Then list:

1. blocking correctness or execution defects;
2. learning-flow defects that could cause one-shotting, confusion, or fake choices;
3. optional refinements; and
4. commands run and their outcomes.

Do not claim readiness when an essential step depends on an unstated design decision or nonexistent command. Do not fail content merely because wording differs from earlier days when the same learning contract remains clear.
