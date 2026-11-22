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
