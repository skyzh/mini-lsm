<!--
  mini-lsm-book © 2022-2025 by Alex Chi Z is licensed under CC BY-NC-SA 4.0
-->

# Environment Setup

The starter code and reference solution is available at [https://github.com/skyzh/mini-lsm](https://github.com/skyzh/mini-lsm).

## Install Rust

See [https://rustup.rs](https://rustup.rs) for more information.

## Clone the repo

```
git clone https://github.com/skyzh/mini-lsm
```

## Starter code

```
cd mini-lsm/mini-lsm-starter
code .
```

## Install Tools

You will need the latest stable Rust to compile this project. The minimum requirement is `1.74`.

```
cargo x install-tools
```

## Run tests

```
cargo x copy-test --week 1 --day 1
cargo x scheck
```

Now, you can go ahead and start [Week 1: Mini-LSM](./week1-overview.md).

{{#include copyright.md}}
