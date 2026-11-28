# Patch Policy

All changes to Rayhunter live in `patches/` as a series applied onto the pinned `upstream/`.

## Format

- One logical change per patch, produced with `git format-patch` against the pinned tag so it
  carries author, date and message.
- File name: `NNNN-short-slug.patch` (four-digit order prefix).
- Subject prefix: `[UPSTREAM]` for generic changes intended for EFForg/rayhunter, `[LOCAL]` for
  Orbic-only changes. Body references the roadmap ticket (`T-OR-NNN`) and, once submitted, the
  upstream PR URL.
- Order in `patches/series` is the application order; `[UPSTREAM]` patches come before
  `[LOCAL]` ones so the upstreamable set stays independent of local changes.

## Authoring a patch

```sh
git -C upstream switch -c work/T-OR-NNN          # scratch branch inside the submodule
# apply the existing series first so you build on it
for p in $(grep -v '^#' patches/series); do git -C upstream am "../patches/$p"; done
# ...edit, test with upstream's own gates (cargo fmt/clippy/test, npm run lint/check/test)...
git -C upstream commit -s -m "[UPSTREAM] analysis: ..."
git -C upstream format-patch -1 -o ../patches/ --start-number NNNN
git -C upstream switch --detach v0.13.0 && git -C upstream branch -D work/T-OR-NNN
echo NNNN-slug.patch >> patches/series
scripts/apply-patches.sh
```

The submodule must end detached at the pinned tag with a clean tree; CI checks out the recorded
commit and fails if the series does not apply.

## Gates for a patch

- Applies cleanly (`scripts/apply-patches.sh`).
- Upstream's own checks pass in `build/rayhunter`: `cargo fmt --all --check`,
  `cargo clippy`, `cargo test`, and the `daemon/web` npm lint/check/test if web code changed.
- New analyzers carry golden QMDL test vectors (see [TESTING.md](TESTING.md)).

## Rebasing onto a new upstream tag

1. `scripts/sync-upstream.sh` shows pinned vs latest; `upstream-watch.yml` opens an issue weekly.
2. `scripts/sync-upstream.sh <tag>` checks out the tag and trial-applies the series.
3. For each failing patch: re-create it on the new tag (authoring procedure above), drop it if
   upstream merged the equivalent, and note that in `CHANGELOG.md`.
4. Commit the submodule move and the refreshed patches together as one change.

## Dependency forks

A fix that belongs in one of Rayhunter's dependencies (not in Rayhunter) goes to a fork of that
dependency, and a patch repoints `Cargo.toml` at it. Rules:

- Pin an exact commit with `rev = "<40-hex>"`. Cargo records no checksum for git dependencies, so
  the full commit hash is the only integrity check; never pin a branch or a short hash.
- Before pinning, audit the fork against the commit upstream had locked: only the intended
  commits, no `build.rs`, proc-macros, `unsafe`, network/filesystem/process use, or new runtime
  dependencies unless the change needs them. Record the result in the patch or PR description.
- The pin is temporary. Once the change merges upstream, point back at the upstream repository.

Current forks:

| Dependency | Fork commit | Patch | Ticket | Upstream status |
|---|---|---|---|---|
| `pycrate-rs` | `doublegate/pycrate-rs@77b1cc8` (branch `mobile-identity`; upstream pin `9e72e40` plus EFF's README commit `9ca5f54` and the change) | 0012 | T-OR-110 | not yet submitted |

## Upstreaming

`[UPSTREAM]` patches are submitted as PRs to EFForg/rayhunter (and dependency changes to the
matching EFForg repository) following its CONTRIBUTING.md. That guide asks contributors to raise
substantial features with the maintainers first, to keep a change under about 400 lines excluding
tests, and, for AI-assisted work, to disclose it and to write the description and comments
themselves. Submission is therefore done by the human maintainer of this project, not an agent.
Once merged upstream and released, the patch is removed from the series at the next rebase.
