# Contributing

Issues and pull requests are welcome.

## Before opening a PR

Run what CI runs:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

CI also runs the tests on the minimum supported Rust version (1.89, the
`rust-version` in `Cargo.toml`) on Linux and macOS, and checks that the
crate packages with `cargo publish --dry-run`.

## Ground rules

- Keep output identical to the Python `memo` unless the change is a
  deliberate difference, and list any new one under Limitations in the
  README.
- Never change the store format in a way the Python tool cannot read.
- Add a line under `Unreleased` in `CHANGELOG.md` for changes users would
  notice.

## Toolchain versions

- **MSRV** is 1.89. Raising it is a deliberate change: update
  `rust-version` in `Cargo.toml`, the MSRV entry in `.github/workflows/ci.yml`,
  the README badge, and note it in the changelog.
- **Lint toolchain**: the CI `lint` job pins one Rust version instead of
  `stable`, so a new clippy lint in a new stable release cannot fail `main`
  by itself. Bump it in its own PR and fix what the new lints flag. Locally,
  a newer clippy may report lints CI does not yet check; fixing them is
  welcome.

## License

By contributing, you agree that your contribution is released under the
MIT License in [LICENSE](LICENSE).
