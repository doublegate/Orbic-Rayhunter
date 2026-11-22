# ADR 0002: License the project GPL-3.0-or-later

- Status: accepted
- Date: 2026-11-22

## Context

Upstream Rayhunter is GPL-3.0. This project distributes binaries built from upstream plus our
patches, and patches are derivative works of upstream files.

## Decision

License the whole repository GPL-3.0-or-later and ship the GPL-3.0 text from upstream as
`LICENSE`. Release assets built from upstream are accompanied by the corresponding source: the
tagged commit of this repository, which pins the exact upstream commit and patch series.

## Consequences

- Contributions must be GPL-compatible. Third-party code under GPL-incompatible terms cannot be
  vendored.
- `[UPSTREAM]` patches can be contributed to EFF under upstream's license without relicensing.
- Proprietary firmware from the device is never redistributed (it is not ours to license), which
  is also enforced by `.gitignore` and the RE rules.
