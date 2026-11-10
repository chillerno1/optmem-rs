# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `memo`, a Rust port of the `memo` tool from
  [VictorTaelin/OptMem](https://github.com/VictorTaelin/OptMem): `init`,
  `wake`, `note`, `nap`, `recall`, `zoom`, `forget`, `config` and `import`,
  reading and writing the same store as the Python tool.
- Crate metadata for crates.io, MSRV 1.89, CI on Linux and macOS,
  Dependabot, and contributor and security docs.
- `MEMORY_WAKE_LINES`: an environment variable that sets `WAKE_LINES` for
  one process, ahead of the memory's `config`, so a short-lived agent can
  wake with a smaller memory context than other sessions on the same
  memory. `memo config` notes when it is set.

[Unreleased]: https://github.com/chillerno1/optmem-rs/commits/main
