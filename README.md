# optmem-rs

[![CI](https://github.com/chillerno1/optmem-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/chillerno1/optmem-rs/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A permanent, append-only memory for AI agents, as one small Rust binary.

## What it is, and why

Coding agents forget everything when a session ends or its context is
compacted. OptMem gives an agent one memory that outlives every session:
the agent records one short line per thing worth keeping, reads its memory
at the start of each session, and searches it when it needs an old detail.

The memory never deletes anything, yet what the agent reads at startup
stays a fixed size. Old memories are folded into a binary tree of one-line
summaries that the agent writes itself, so a memory of ten thousand lines
still wakes in under a hundred.

This crate is a Rust port of the `memo` tool from
[VictorTaelin/OptMem](https://github.com/VictorTaelin/OptMem). It reads and
writes the same store and prints the same output, so both tools can share
one memory. It is a single binary with no runtime to install.

## Install

With a Rust toolchain (stable, edition 2024):

```sh
cargo install --git https://github.com/chillerno1/optmem-rs
```

This installs the `memo` binary into `~/.cargo/bin`. Or build from a clone:

```sh
git clone https://github.com/chillerno1/optmem-rs
cd optmem-rs
cargo build --release   # binary at target/release/memo
```

## Quick start

Try it on a throwaway memory:

```sh
export MEMORY_DIR="$(mktemp -d)/memory"
memo init
memo note "The staging deploy needs the VPN."
memo wake
memo recall 'deploy|vpn'
```

`init` creates the memory and prints a block of instructions to paste into
your agent's `AGENTS.md` (or `CLAUDE.md`). `note` prints `Saved as #0.`,
`wake` prints the memory and ends with `You are awake.`, and `recall`
prints:

```text
#0 2026-01-01 The staging deploy needs the VPN.
1 match.
```

with today's date. Drop the `export` line to use the default memory in
`~/.optmem/memory`.

## Commands

| Command | What it does |
| --- | --- |
| `memo init` | Create this memory; print the setup block for your agent. |
| `memo wake [part [T]]` | Read the memory. Long output is split into parts; `wake` says which command reads the next one. |
| `memo note "..."` | Record one memory: one line, at most `ENTRY_CHARS` bytes. |
| `memo nap [id "..."]` | With no arguments, show the next pending compression. With an id and a line, save that summary. |
| `memo recall <regex>` | Case-insensitive regex search over every memory ever recorded. |
| `memo zoom <lo>-<hi>` | Open a tree node, such as `0-15`, into its two halves. |
| `memo forget <lo>-<hi>` | Drop a bad summary; `nap` asks for it again. |
| `memo config [NAME=N]` | Show this memory's sizes, or change one. `NAME=` restores the default. |
| `memo import <file>` | Bulk-load lines of `YYYY-MM-DD <text>`, oldest first (for bootstrapping). |

Run `memo` with no arguments for the same list.

## Configuration

| Setting | Default | Meaning |
| --- | --- | --- |
| `MEMORY_DIR` (env) | `~/.optmem/memory` | Where the memory lives. |
| `WAKE_LINES` | 96 | How many lines `wake` prints. |
| `ENTRY_CHARS` | 280 | The longest one memory or summary may be, in bytes. |
| `PART_CHARS` | 20000 | Largest `wake` part, in bytes. |
| `PART_LINES` | 500 | Largest `wake` part, in lines. |

Sizes are stored per memory, in its `config` file. Change one with, for
example, `memo config WAKE_LINES=128`.

## How it works

- `LOG.txt` holds every memory as a fixed-width record, so memory `#i` is
  found by seeking. Nothing in it is ever rewritten.
- `TREE/<size>` files hold one-line summaries of aligned, power-of-two
  blocks of memories: `#0-1`, `#2-3`, then `#0-3`, and so on.
- `wake` tiles the whole history into at most `WAKE_LINES` blocks: recent
  memories appear as themselves, older ones as ever larger summaries.
- Summaries are written by the agent. When a block it needs is missing,
  `note`, `wake` and `nap` print the lines to compress (raw memories for
  small blocks, the two half summaries for larger ones) and the exact
  `memo nap <id> "<line>"` command to save the result.
- Readers take no lock. Writers serialize on a `flock` of `.lock` in the
  memory directory, the same lock the Python tool takes.

## Limitations

- Unix only (Linux, macOS). There is no Windows support.
- `recall` uses [fancy-regex](https://crates.io/crates/fancy-regex), so a
  few rare patterns read differently from Python's `re`.
- Digits in ids, dates and sizes are ASCII `0` to `9` only, and block ids
  past 2^128 - 1 are refused.
- Where the Python tool would print a traceback (a corrupt log record, a
  non-UTF-8 config file or argument), this one prints a one-line error and
  exits 1.
- The tool path shown in messages comes from `argv[0]`.

Otherwise, output matches the Python `memo` byte for byte.

## Contributing

Issues and pull requests are welcome. Before opening a PR, run what CI runs:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Keep output identical to the Python `memo` unless the change is a deliberate
difference, and list any new one under Limitations.

## Credits

OptMem was created by Victor Taelin:
<https://github.com/VictorTaelin/OptMem>. Its design, command set, store
format and the agent prompt text that `memo init` prints all come from his
original implementation. This repository is only a Rust reimplementation of
his `memo` tool. His repository had no license file when this port was
published, so rights in his original work stay with him. See
[NOTICE](NOTICE).

## License

The Rust code in this repository is released under the MIT License, see
[LICENSE](LICENSE). It does not cover Victor Taelin's original work.
