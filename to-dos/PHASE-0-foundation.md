# Phase 0: Foundation (v0.1.0)

Goal: a buildable, lintable, documented overlay repository with upstream pinned, CI defined and
running, and the first release tagged. Tickets T-OR-001..T-OR-011 ([ROADMAP.md](../ROADMAP.md)).

## Sprint 0.1: Scaffold (done)

- [x] T-OR-001: GPL-3.0-or-later `LICENSE`, overlay layout (`crates/`, `patches/`, `device/`,
      `re/`, `backups/`, `docs/`, `ref-docs/`, `to-dos/`), upstream submodule at v0.13.0
- [x] T-OR-002: workflows `ci.yml`, `docs.yml`, `security.yml`, `release.yml`, `upstream-watch.yml`
- [x] T-OR-003: `orx doctor` (USB sysfs detection f626/f601/9008, TCP probes :80 and :8080) with
      unit tests on a fake sysfs tree and a local listener
- [x] T-OR-004: `scripts/apply-patches.sh`, `build.sh`, `sync-upstream.sh`, `gen-assets.sh`
- [x] T-OR-005: research corpus `ref-docs/*.md` (nine files, PDFs local only)
- [x] T-OR-006: GitHub repository `doublegate/Orbic-Rayhunter` created

Acceptance criteria: `cargo test` passes; `scripts/build.sh devel` produces `dist/` with
`SHA256SUMS`; `orx doctor` reports the unit in normal mode.

Verification:

```sh
cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test
shellcheck scripts/*.sh
actionlint
scripts/build.sh devel && (cd dist && sha256sum -c SHA256SUMS)
cargo run -p orx-cli -- doctor
```

### Research (REA/Ghidra/web)

- Upstream source tree at v0.13.0: `installer/src/orbic_network.rs`, `orbic.rs`, `util.rs`,
  `dist/scripts/misc-daemon` (read for the corpus; done).
- Web: EFForg/rayhunter issues and discussions, mjg59 posts, XDA reset thread, FCC ID
  2ABGH-RC400L (done; see `ref-docs/`).

## Sprint 0.2: Specification docs (done, uncommitted)

- [x] T-OR-007: `docs/DEVICE-PROFILE.md`, `INSTALL.md`, `BACKUP-AND-RECOVERY.md`,
      `CONFIGURATION.md`, `ANALYZERS.md`, `TESTING.md`, `PERFORMANCE.md`, `SECURITY-MODEL.md`
- [x] T-OR-007: `ROADMAP.md` with tickets T-OR-001..T-OR-105
- [x] T-OR-007: `PROJECT-STATUS.md` with the fixed skeleton
- [x] T-OR-007: `to-dos/` index and phase files 0-7
- [ ] T-OR-007: maintainer review; correct the installer naming in `docs/ARCHITECTURE.md`
      (`installer orbic` is the network installer, `orbic-usb` the USB one)

Acceptance criteria: markdownlint clean on the new files; every ticket referenced in `docs/` and
`to-dos/` exists in ROADMAP.md; facts cite a `ref-docs/` file or are marked TBD (on-device).

Verification:

```sh
markdownlint-cli2 "docs/**/*.md" "to-dos/**/*.md" ROADMAP.md PROJECT-STATUS.md
comm -23 <(grep -rhoE 'T-OR-[0-9]{3}' docs to-dos PROJECT-STATUS.md | sort -u) \
         <(grep -hoE 'T-OR-[0-9]{3}' ROADMAP.md | sort -u)    # must print nothing (bash)
```

### Research (REA/Ghidra/web)

- Cross-check contradictions between `ref-docs/` files (band list, firmware versions) and record
  corrections as new dated supplemental files, never in-place edits.

## Sprint 0.3: Publish

- [ ] T-OR-008: first commit (Conventional Commits) and push to `main`
- [ ] T-OR-009: branch protection on `main`: required checks from `ci`, `docs`, `security`; no
      force push
- [ ] T-OR-010: first CI run; open each job log and confirm it ran its real gate (no skipped
      steps, no `continue-on-error` on blocking jobs, submodule checked out, patch series applied,
      armv7 build produced artifacts)
- [ ] T-OR-011: CHANGELOG `[Unreleased]` to `[0.1.0] - YYYY-MM-DD`, annotated tag v0.1.0, release
      assets verified

Acceptance criteria: CI green on the tagged commit with every job's gate verified by reading its
log; release page has `orx`, `rayhunter-daemon`, `rootshell`, `SHA256SUMS`.

Verification:

```sh
git log --oneline -1
gh run list --branch main --limit 5
gh run view <run-id> --log | less          # read each job
gh api repos/doublegate/Orbic-Rayhunter/branches/main/protection
git tag -l v0.1.0 && gh release view v0.1.0
rel="$(mktemp -d)" && gh release download v0.1.0 -D "$rel" && (cd "$rel" && sha256sum -c SHA256SUMS)
```

### Research (REA/Ghidra/web)

- Pinned action versions in the workflows: confirm each pin resolves and is at least two weeks
  old; confirm the markdownlint and gitleaks versions CI installs are pinned.
