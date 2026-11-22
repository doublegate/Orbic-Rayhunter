## Summary

<!-- What and why. Reference the ticket: T-OR-NNN -->

## Changes

-

## Checks run

- [ ] `cargo fmt --all --check` / `cargo clippy --all-targets -- -D warnings` / `cargo test`
- [ ] shellcheck on changed scripts; `bash scripts/agy-review-selftest.sh` if reviewer files changed
- [ ] `scripts/apply-patches.sh` (if `patches/` changed)
- [ ] Docs and `CHANGELOG.md` updated in this PR
- [ ] Tested on device (firmware build: ) / not applicable
- [ ] No identifiers, captures or firmware blobs added
