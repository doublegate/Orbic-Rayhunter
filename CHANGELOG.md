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

- Antigravity reviewer re-synced from template `1451f73`: agy runs in a bubblewrap jail
  (`AGY_ISOLATION=bwrap`) with an empty private `HOME` (no MCP servers, skills, hooks or
  repositories), read-only `/usr` and `/etc`, and a D-Bus proxy exposing only its own keyring
  item. Its network is a TLS-intercepting egress proxy that allows only exact Google hosts and
  rebuilds every request. Credential-shaped output is redacted before posting, and anything
  redaction cannot remove stops the post. agy's own `--sandbox` was verified not to confine
  reads.
- `.gitignore` excludes device backups, firmware images, captures (`*.qmdl`, `*.qmdl.gz`,
  `*.ndjson`, `*.pcap`), Ghidra projects, extracted firmware, `wpa_sta.conf`, `.env` files and
  downloaded papers whose licenses do not permit redistribution.
- Device identifiers are kept out of the repository; test fixtures use a synthetic USB serial.
- `upstream-watch.yml` validates upstream tag names against strict SemVer and passes them to
  the shell through `env` instead of expression interpolation; every `actions/checkout` sets
  `persist-credentials: false`.
- Antigravity reviewer re-synced from the hardened template: agy runs from a private sandbox
  directory outside the checkout (large-diff handoff included), with an allowlisted environment
  (`env -i` plus the session variables its keyring-held login needs, so no job token or
  workflow `env:` value reaches it under any name), and post-agy `git` calls ignore hooks. The interim
  `AGY_DIFF_MODE: inline` workaround is removed, and with it the unused `/.agy-review-work/`
  ignore line.

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
- CI caching: Rust caches saved only from `main` (restored by PRs and releases) for the host
  crates and the patched upstream's third-party dependencies, rustup toolchain plus armv7
  target, npm, the SHA-256-verified gitleaks binary and lychee link results. Pull-request runs cancel superseded runs; every job has a timeout;
  build artifacts are retained 14 days.
- `scripts/apply-patches.sh` stages upstream plus patches in a scratch directory and syncs it
  into `build/rayhunter` by content (`rsync --checksum --delete`), never touching `target/` or
  `node_modules/`: incremental local builds and restored CI caches survive `scripts/build.sh`,
  and only changed files get a fresh mtime so Cargo never reuses a stale patched artifact.
  Requires `rsync`.
- Non-PR workflow runs get a per-run concurrency group, so a pending run on `main` is never
  replaced (and its changes never left unvalidated) by a later push.
- `cargo-audit` runs the pinned `cargo-audit` 0.23.0 CLI (installed by `taiki-e/install-action`)
  with `--deny warnings` instead of `rustsec/audit-check`, whose latest release targets the
  deprecated Node.js 20 runtime and required `checks`/`issues` write permissions.
- The docs `links` job also runs on any deletion or rename, since local link targets are not
  only Markdown.
- Rust caches use a `shared-key` so `release.yml` restores the caches `ci.yml` saves.
- `docs.yml` gains a weekly link-rot run.
- `main` protection moved from a classic branch protection rule to the `main` repository ruleset:
  same required checks (now pinned to the GitHub Actions app), linear history, no force pushes or
  deletions, PRs with resolved threads and squash merges; repository admins keep bypass.

### Fixed

- `orx doctor` printed `serial=?` for the RC400L in normal mode, which exposes no `serial` sysfs
  attribute and carries the serial in its product string (`MDM9207-MTP _SN:<serial>`). It now
  falls back to the `_SN:` token. The token is accepted only directly after a word-start `_SN:`
  marker, as 1-64 ASCII alphanumerics; a real `serial` attribute still takes precedence. The
  product string is split once when the device is read (`split_product()`): `product` keeps only
  the model text and the serial lives only in `serial`, so neither the doctor output nor a `Debug`
  print carries the serial twice (T-OR-106). Measured on the unit during the T-OR-012
  host-side recon.
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
- The `review` job skips with a visible warning, instead of failing red, while the reviewer
  scripts are not yet on the default branch (the PR that installs it); any failure inside the
  reviewer still fails the job.
- `scripts/apply-patches.sh` canonicalizes the destination and patch directory: git ignores
  relative `GIT_CEILING_DIRECTORIES` entries, so a relative `$1` would again have skipped every
  patch. Tests now cover relative paths and assert that unchanged files keep their mtime.
- The patched-upstream Rust cache is no longer keyed on `patches/**` (rust-cache stores only
  third-party dependencies, so a patch edit discarded it for nothing); the lychee cache uses a
  per-run key and the scheduled run re-checks every link; scheduled and manual runs skip the
  unused full-history checkout; the exec-bit gate covers `tests/*.sh`.

- Scripts are committed with mode `100755`; the first CI run failed with `Permission denied`
  because the repository uses `core.fileMode=false`. CI now fails early if a script loses its
  executable bit.
