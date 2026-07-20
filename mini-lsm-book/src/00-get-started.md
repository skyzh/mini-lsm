<!--
  mini-lsm-book © 2022-2025 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
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

## Run the Tests

```
cargo x copy-test --week 1 --day 1
cargo x scheck
```

You are now ready to begin [Week 1: Mini-LSM](./week1-overview.md).

{{#include copyright.md}}
