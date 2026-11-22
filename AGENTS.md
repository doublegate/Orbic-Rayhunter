# AGENTS.md - Orbic-Rayhunter

Guidance for coding agents working in this repository.

## What this is

Overlay project on EFF Rayhunter for the Orbic RC400L (`ORB400L_V1.2.0_BVZRT`, Qualcomm MDM9207,
armv7). Upstream is a submodule at `upstream/` pinned to a tag; changes to Rayhunter are patch
files in `patches/` listed in `patches/series`. Never edit `upstream/` directly.

## Commands

| Task | Command |
|---|---|
| Host crates gate | `cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test` |
| Scripts lint | `shellcheck scripts/*.sh` |
| Workflow lint | `actionlint` |
| Docs lint | `markdownlint-cli2 '**/*.md'` |
| Stage patched upstream | `scripts/apply-patches.sh` (into `build/rayhunter`) |
| Build device binaries | `scripts/build.sh devel` (into `dist/`) |
| Upstream update check | `scripts/sync-upstream.sh [<tag>]` |
| Device check | `cargo run -p orx-cli -- doctor` |

Toolchain: Rust 1.98.0 (matches `upstream/rust-toolchain.toml`).

## Device rules

- A verified, checksummed full backup must exist before any command that writes to the device.
- The host may have other ADB devices attached. Always address the Orbic explicitly
  (`adb -s <serial>`); never run an unqualified `adb` command.
- Receive-only. Never transmit on licensed spectrum; lab base stations only in a shielded or
  cabled setup.
- Never commit firmware, dumps, captures, IMSI/IMEI/ICCID, admin passwords or Wi-Fi keys.

## Docs discipline

`docs/` is the specification: update it in the same change as behavior. `ref-docs/` is an
immutable dated research corpus; corrections go in a new dated file. Roadmap tickets are
`T-OR-NNN`, minted in `ROADMAP.md`. Current state lives only in `PROJECT-STATUS.md`.

## Commits

Conventional Commits, one logical change each. Commit only when asked.
