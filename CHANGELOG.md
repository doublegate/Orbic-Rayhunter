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
