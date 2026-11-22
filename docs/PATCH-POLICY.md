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

## Upstreaming

`[UPSTREAM]` patches are submitted as PRs to EFForg/rayhunter following its CONTRIBUTING.md.
Once merged upstream and released, the patch is removed from the series at the next rebase.
