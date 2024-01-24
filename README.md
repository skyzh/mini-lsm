# LSM in a Week

[![CI (main)](https://github.com/skyzh/mini-lsm/actions/workflows/main.yml/badge.svg)](https://github.com/skyzh/mini-lsm/actions/workflows/main.yml)

Build a simple key-value storage engine in a week!

## Tutorial

The tutorial is available at [https://skyzh.github.io/mini-lsm](https://skyzh.github.io/mini-lsm). You can use the provided starter
code to kick off your project, and follow the tutorial to implement the LSM tree.

## Community

You may join skyzh's Discord server and study with the mini-lsm community.

[![Join skyzh's Discord Server](https://dcbadge.vercel.app/api/server/ZgXzxpua3H)](https://skyzh.dev/join/discord)

## Development

```
cargo x install-tools
cargo x check
cargo x book
```

If you changed public API in the reference solution, you might also need to synchronize it to the starter crate.
To do this, use `cargo x sync`.

## Structure

* mini-lsm: the final solution code
* mini-lsm-starter: the starter code
* mini-lsm-book: the tutorial

We have another repo mini-lsm-solution-checkpoint at [https://github.com/skyzh/mini-lsm-solution-checkpoint](https://github.com/skyzh/mini-lsm-solution-checkpoint). In this repo, each commit corresponds to a chapter in the tutorial. We will not update the solution checkpoint very often.

## Demo

You can run the reference solution by yourself to gain an overview of the system before you start.

```
cargo run --bin mini-lsm-cli-ref
```

And we have a compaction simulator to experiment with your compaction algorithm implementation,

```
cargo run --bin compaction-simulator-ref
```

## License

The Mini-LSM starter code and solution are under Apache 2.0 license. The author reserves the full copyright of the tutorial materials (markdown files and figures).
