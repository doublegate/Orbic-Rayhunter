# Release Process

## Versioning

SemVer. Until v1.0.0, minor versions mark phase completions (see [ROADMAP.md](../ROADMAP.md)).
The project version is `workspace.package.version` in `Cargo.toml`; the upstream pin is part of
every release note.

## Checklist (fail closed)

1. Clean tree on `main`, CI green on the release commit (all jobs ran their real gate).
2. `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
3. `scripts/apply-patches.sh` and `scripts/build.sh firmware` succeed locally.
4. On-device verification on `ORB400L_V1.2.0_BVZRT` per [TESTING.md](TESTING.md) smoke list,
   including a reboot and an uninstall/restore rehearsal for releases that change the install.
5. `CHANGELOG.md`: rename `[Unreleased]` to `[X.Y.Z] - YYYY-MM-DD`; README support matrix and
   `PROJECT-STATUS.md` updated; version bumped in `Cargo.toml`.
6. Tag does not already exist (`git tag -l vX.Y.Z`, `gh release view vX.Y.Z`).
7. Annotated tag with summary, key changes, upstream pin, test evidence:
   `git tag -a vX.Y.Z -F notes.txt`, then push the tag; `release.yml` builds and attaches
   `orx`, `rayhunter-daemon`, `rootshell` and `SHA256SUMS`.
8. Verify the release page assets and checksums.

A published version is immutable: fix forward with a new patch version.

## Release notes (robust, comprehensive, technically detailed)

Every release's notes have two parts, assembled by `release.yml`:

1. **The CHANGELOG section for the version** (human-written). This must be comprehensive, not a
   one-liner: a short overview paragraph (what the release is, the upstream pin, whether it
   touches the device) followed by the full `Added` / `Changed` / `Fixed` / `Security` /
   `Removed` breakdown with enough technical detail that a reader understands each change and its
   rationale. Write it as you land the work (the `[Unreleased]` discipline), so the release is
   just a rename.
2. **An auto-generated technical appendix** (by `release.yml`): a release-asset table with each
   file's target/role, size and SHA-256; build provenance (tagged commit, upstream pin, Rust
   toolchain); and pointers to install/scope/backup/security docs and the full changelog. This
   guarantees the asset, checksum and provenance detail is present on every release without
   hand-assembly.

The annotated tag (step 7) carries its own standalone summary + key changes + upstream pin +
test evidence. Keep all three consistent. For a release that changes the install, also document
install steps, the platform/support matrix, known issues and upgrade/rollback notes in the
notes body.
