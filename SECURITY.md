# Security policy

## Supported versions

Only the latest release, and the `main` branch, get security fixes.

## Reporting a vulnerability

Please report it privately through GitHub:
[Report a vulnerability](https://github.com/chillerno1/optmem-rs/security/advisories/new)
(the Security tab of this repository). Do not open a public issue.

Include what you ran, what happened, and the version or commit. You should
get a first reply within a week. Once a fix is out, the advisory is
published with credit to you unless you ask otherwise.

## Scope

`memo` reads and writes files in one directory (`MEMORY_DIR`, by default
`~/.optmem/memory`) and takes no network input. Reports that matter most:
a crafted store or argument that makes `memo` write outside that directory,
corrupt existing records, or crash in a way that loses data.
