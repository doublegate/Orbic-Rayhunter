# ADR 0001: Overlay repository with upstream as a pinned submodule

- Status: accepted
- Date: 2026-11-22

## Context

The project modifies and extends EFF Rayhunter for one device while intending to send generic
improvements upstream. Upstream releases frequently (v0.11.0 to v0.13.0 between May and
September 2026). Options considered: a GitHub fork modified in place, an overlay with upstream
as a submodule plus a patch series, or a tooling-only repository with no code changes.

## Decision

Use an overlay repository. `upstream/` is a git submodule pinned to a release tag; every change
to Rayhunter is a `git format-patch` file in `patches/`, tagged `[UPSTREAM]` or `[LOCAL]` and
applied by `scripts/apply-patches.sh` into `build/rayhunter`. Project-owned code (host tool,
device config, docs) lives outside `upstream/`.

## Consequences

- Each upstream bump is an explicit, reviewable submodule move plus a patch rebase; a weekly
  workflow flags new tags.
- `[UPSTREAM]` patches map one-to-one to upstream PRs and disappear from the series once merged.
- Patch files are harder to review than ordinary diffs and conflicts surface at rebase time.
  Keeping patches small and ordered `[UPSTREAM]` before `[LOCAL]` limits this.
- CI must check out submodules and verify the series applies.
