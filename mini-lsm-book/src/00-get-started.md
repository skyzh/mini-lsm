<!--
  mini-lsm-book © 2022-2026 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Environment Setup

The starter code and reference solution are available in the [Mini-LSM repository](https://github.com/skyzh/mini-lsm).

## Install Rust

See [https://rustup.rs](https://rustup.rs) for more information.

## Clone the Repository

```
git clone https://github.com/skyzh/mini-lsm
```

## Open the Starter Code

```
cd mini-lsm/mini-lsm-starter
code .
```

## Install Tools

The repository pins the required Rust toolchain in `rust-toolchain.toml`. If you use `rustup`, Cargo will select and install that toolchain automatically.

```
cargo x install-tools
```

## Check the Starter and Reveal the First Tests

From the repository root, first confirm that the untouched starter compiles:

```
cargo check -p mini-lsm-starter --lib
```

This command should pass. Then copy the Week 1 Day 1 tests and record the initial red gate:

```
cargo x copy-test --week 1 --day 1
cargo x scheck
```

`cargo x scheck` should exit with status 1 because the six copied Day 1 tests fail at the starter's unimplemented boundaries. These
failures are the expected starting evidence, not a setup failure. Chapter 1 turns this gate green.

You are now ready to begin [Week 1: Mini-LSM](./week1-overview.md).

{{#include copyright.md}}
