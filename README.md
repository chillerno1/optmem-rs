# optmem-rs

OptMem in pure Rust: a permanent, append-only memory for AI agents.

An agent records one short line per thing worth keeping (`memo note`), reads
its memory at the start of every session (`memo wake`), and searches it
(`memo recall`). Old memories are folded into a binary tree of one-line
summaries, so `wake` stays within a fixed size budget however long the
memory grows, and `memo zoom` opens any summary back down to the raw lines.

This crate is a Rust port of the `memo` script from
[VictorTaelin/OptMem](https://github.com/VictorTaelin/OptMem). It reads and
writes the same store and prints the same output, so the two can share a
memory. It builds to a single static-ish binary with no runtime to install.

## Install

```sh
cargo install --git https://github.com/chillerno1/optmem-rs
```

This puts `memo` in `~/.cargo/bin`. Unix only (Linux, macOS).

## Usage

```sh
memo init                         # create the memory; prints a block for your agent's AGENTS.md
memo note "the deploy needs VPN"  # record one memory
memo wake                         # read the memory; run first in every session
memo recall 'deploy|vpn'          # case-insensitive regex search over every memory
memo zoom 0-15                    # open a summary node into its two halves
memo nap                          # do pending compressions (the agent writes the summaries)
memo config                       # show or change the size budgets
```

Memories live in `~/.optmem/memory`, or in `$MEMORY_DIR` if set. Run `memo`
with no arguments for the full command list.

## Differences from the Python `memo`

Output matches the Python tool byte for byte on stdout, stderr and exit code
for every input a working store produces. Where it deliberately differs:

- A corrupt log record, a non-UTF-8 config file or argument gives a one-line
  error and exit 1 instead of a traceback.
- Digits are ASCII `0` to `9` only, and numbers have a ceiling (block ids
  past 2^128 - 1 are "not a block").
- `recall` uses [fancy-regex](https://crates.io/crates/fancy-regex), so rare
  patterns read differently from Python's `re`.
- The tool path printed in messages comes from argv[0].
- Writes use `F_FULLFSYNC` on macOS. There is no Windows support.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Credits

OptMem, its design, command set, store format and the agent prompt text
that `memo init` prints, are by Victor Taelin:
<https://github.com/VictorTaelin/OptMem>. This repo is only a Rust
reimplementation of his `memo` tool. His repository carries no license file
at the time of writing, so rights in his original work and text stay with
him. See [NOTICE](NOTICE).

## License

The Rust code in this repo is MIT licensed, see [LICENSE](LICENSE). That
license does not extend to Taelin's original work described above.
