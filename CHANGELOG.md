# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Phase 0 (foundation) toward v0.1.0. Nothing here installs anything on a device yet.

### Added

- Repository scaffold: GPL-3.0-or-later license (ADR 0002), overlay layout (ADR 0001), upstream
  EFForg/rayhunter submodule pinned to `v0.13.0` (`cd462a1`), Rust 1.98.0 toolchain pin matching
  upstream.
- `orx` host tool (`crates/orx-cli`, `unsafe_code = "forbid"`, no dependencies) with
  `orx doctor [--ip <addr>]`: Qualcomm USB discovery through sysfs, classifying `05c6:f626`
  (stock RNDIS + CDC), `05c6:f601` (debug ADB + DIAG + AT) and `05c6:9008` (EDL), plus TCP
  reachability of the stock web UI (`:80`) and Rayhunter (`:8080`). Exit 1 when no device and no
  stock UI are found, 2 on usage or sysfs errors. Five unit tests.
- Build tooling: `scripts/apply-patches.sh` (exports the committed submodule tree with
  `git archive` and applies `patches/series` with `git apply --check` first, never touching
  `upstream/`), `scripts/build.sh devel|firmware` (upstream web frontend plus upstream cargo
  aliases for `armv7-unknown-linux-musleabihf`, artifacts and `SHA256SUMS` in `dist/`),
  `scripts/sync-upstream.sh [<tag>]` (pinned vs latest, trial patch apply),
  `scripts/gen-assets.sh` (derived icons, favicons, banners and SVG renders).
- Device configuration template `device/config/config.orbic.toml` (upstream keys only; double-tap
  recording and larger free-space floors as documented deviations).
- CI, with every action pinned to a commit SHA: `ci.yml` (host crate fmt, clippy `-D warnings`,
  tests; shellcheck; patch series apply plus armv7 daemon and rootshell build with artifact
  upload), `docs.yml` (markdownlint, lychee), `security.yml` (gitleaks 8.30.1, cargo-audit,
  CodeQL for Actions and Rust), `upstream-watch.yml` (weekly new-tag issue with trial patch
  apply), `release.yml` (tag-driven build, checksums, notes from this file, never duplicates a
  release). Dependabot for Actions and Cargo; the submodule is moved deliberately.
- Community files: README, CONTRIBUTING, SECURITY (private vulnerability reporting),
  CODE_OF_CONDUCT (Contributor Covenant 2.1), issue forms, PR template, CODEOWNERS, AGENTS.md and
  CLAUDE.md.
- Research corpus `ref-docs/`: upstream overview, Orbic install procedures, community install
  reports, RC400L hardware, firmware V1.2.0, Qualcomm DIAG/QMDL, related tools, LTE IMSI-catcher
  detection literature with a ranked analyzer backlog, legal and ethics notes, and a dated
  reconciliation supplement for five cross-document conflicts. `ref-docs/pdfs/README.md` lists 20
  locally downloaded papers with source and stated license.
- Specification `docs/`: architecture, device profile, install, backup and recovery,
  configuration, analyzers, testing, performance, security model, patch policy, build,
  reverse-engineering methodology, release process, ADRs 0001-0002.
- Planning: `ROADMAP.md` (v0.1.0 to v2.0.0, tickets T-OR-001 to T-OR-105), `PROJECT-STATUS.md`,
  and eight phase plans in `to-dos/`.
- Artwork: icon, logo, hero banner and social card generated with `agy`; four SVG diagrams
  (architecture dark and light, detection pipeline, release roadmap); README header, badges,
  research findings, layout and documentation index, and Mermaid build-pipeline and install
  safety-gate diagrams.

### Security

- `.gitignore` excludes device backups, firmware images, captures (`*.qmdl`, `*.qmdl.gz`,
  `*.ndjson`, `*.pcap`), Ghidra projects, extracted firmware, `wpa_sta.conf`, `.env` files and
  downloaded papers whose licenses do not permit redistribution.
- Device identifiers are kept out of the repository; test fixtures use a synthetic USB serial.
- `upstream-watch.yml` validates upstream tag names against strict SemVer and passes them to
  the shell through `env` instead of expression interpolation; every `actions/checkout` sets
  `persist-credentials: false`.

### Added (CI)

- Antigravity PR reviewer (`antigravity-review.yml`, `scripts/agy-review*.sh`, `scripts/_agy_*.sh`,
  `.github/AGY-REVIEWER.md`), vendored from the `antigravity-pr-review` template: reviews every
  same-repository PR on open, reopen and push, and on `/agy-review`, using the self-hosted `agy`
  runner and the `agy` CLI's OAuth session. Project review rules in `.github/agy-review.md`.
  Fork PR workflows now require approval for all outside contributors.
- The reviewer's offline self-test runs in the `shellcheck` CI job.

### Changed

- CI is path-aware: `ci`, `docs` and `security` each begin with a `detect changes` job
  (`dorny/paths-filter`) and skip jobs whose inputs did not change, using job-level `if:` so
  required checks still report as passing. `gitleaks` always runs; scheduled and manual runs
  run every job. The detection jobs (`detect changes (ci|docs|security)`) are themselves
  required checks, so a detection failure blocks the merge instead of silently skipping every
  gated check (fail closed). See `docs/CI.md`.
- CI caching: Rust caches saved only from `main` (restored by PRs and releases), patched-upstream
  cache keyed on the patch series, rustup toolchain plus armv7 target, npm, the gitleaks binary
  and lychee link results. Pull-request runs cancel superseded runs; every job has a timeout;
  build artifacts are retained 14 days.
- `scripts/apply-patches.sh` stages upstream plus patches in a scratch directory and syncs it
  into `build/rayhunter` by content (`rsync --checksum --delete`), never touching `target/` or
  `node_modules/`: incremental local builds and restored CI caches survive `scripts/build.sh`,
  and only changed files get a fresh mtime so Cargo never reuses a stale patched artifact.
  Requires `rsync`.
- Non-PR workflow runs get a per-run concurrency group, so a pending run on `main` is never
  replaced (and its changes never left unvalidated) by a later push.
- The docs `links` job also runs on any deletion or rename, since local link targets are not
  only Markdown.
- Rust caches use a `shared-key` so `release.yml` restores the caches `ci.yml` saves.
- `docs.yml` gains a weekly link-rot run.
- `main` protection moved from a classic branch protection rule to the `main` repository ruleset:
  same required checks (now pinned to the GitHub Actions app), linear history, no force pushes or
  deletions, PRs with resolved threads and squash merges; repository admins keep bypass.

### Fixed

- `scripts/apply-patches.sh` silently skipped every patch when staging inside this repository
  (the default `build/` layout): `git apply` discovered the superproject's `.git` and ignored
  patched paths outside its subdirectory while exiting 0. It now runs with
  `GIT_CEILING_DIRECTORIES` so it acts as plain `patch(1)`. Found by the Antigravity review; the
  series was still empty, so no build was affected. New regression test `tests/apply-patches.sh`
  (patch applied in and outside the repo, removal restores upstream with a fresh mtime,
  `target/` survives, missing and non-applying patches fail) runs in CI before every build.
- Change-detection jobs fetch full history, so on a push `dorny/paths-filter` can diff against
  `github.event.before` instead of treating every file as added in a shallow clone.
- `markdownlint` and `links` now cover every Markdown file, including `.github/` and the
  subdirectory READMEs; `lychee.toml` changes trigger only `links`; CodeQL also runs on
  `Cargo.lock` and `rust-toolchain.toml` changes.
- The armv7 build installs the upstream toolchain and target before `rust-cache` probes `rustc`;
  `release.yml` restores the rustup cache and no longer restores the dev-profile host cache.
- The reviewer's first-install bootstrap steps are removed (CodeQL `actions/untrusted-checkout`).

- Scripts are committed with mode `100755`; the first CI run failed with `Permission denied`
  because the repository uses `core.fileMode=false`. CI now fails early if a script loses its
  executable bit.
