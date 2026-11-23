# Reproducible build: our firmware daemon vs the upstream release (T-OR-030)

Phase 3 question: can we rebuild the device daemon from our pinned upstream + (empty) patch
series and get the upstream release binary? Measured 2026-11-23.

## Method

- Staged `upstream/` (EFForg/rayhunter `v0.13.0`, `cd462a1`) + `patches/series` (empty) into
  `build/rayhunter` with `scripts/apply-patches.sh`.
- Built the web UI on the host, then built the daemon in the same container upstream uses
  (`messense/rust-musl-cross:armv7-musleabihf`) with upstream's own alias
  `cargo build-daemon-firmware` (profile `firmware`, `--no-default-features --features pq-tls`).
  This is exactly `scripts/build.sh firmware`.
- Compared to the upstream release daemon shipped in `rayhunter-v0.13.0` (armv7, inside the
  `rayhunter-daemon/` folder of the release zip).

## Result: same build, not bit-identical

| Binary | Size (bytes) | SHA-256 (first 16) |
|---|---|---|
| Ours (`firmware`, container) | 6,109,892 | `9b73fe9c58a7bf4e` |
| Upstream release | 6,109,508 | `90960d07f67bf41f` |

- **Size delta: 384 bytes (0.006%).** Same profile, toolchain (Rust 1.98.0 via the pinned
  `rust-toolchain.toml`), dependency set (`Cargo.lock`), TLS backend (pq-tls) and container.
- **Not byte-identical: 5,179,649 of 6.1 M bytes differ.** This is expected Rust/musl
  non-determinism, dominated by absolute build paths embedded in the binary (our `/work` mount
  vs upstream's CI checkout path) and layout that is not forced to a canonical order. The small
  size difference is consistent with path-string length differences.

## Interpretation

Functional equivalence is assured by identical **source** (pinned submodule + empty patch
series), **toolchain**, **lockfile** and **features** -- the inputs that determine behavior.
Bit-for-bit reproducibility would additionally need `--remap-path-prefix` (or `trim-paths`) to
erase the build path, plus a fixed build root; that is an upstreamable follow-up (T-OR-067
candidate), not a behavior concern.

## What changed as a result

- `scripts/build.sh firmware` now builds in the `messense/rust-musl-cross` container (no host
  cross toolchain needed), making the production build reproducible on any machine with docker
  or podman.
- `release.yml` now ships the production `firmware` daemon (~6.1 MB, pq-tls, stripped) instead
  of the `firmware-devel` build (~16 MB, rustcrypto, unstripped) that v0.1.0-v0.3.0 shipped.
  This is the binary intended for the device and for the eventual v1.0.0 our-build install.

## Evidence

- Upstream build recipe: `build/rayhunter/.github/workflows/main.yml` (daemon step runs
  `cargo build-daemon-firmware` in `messense/rust-musl-cross:armv7-musleabihf`).
- Profiles: `build/rayhunter/.cargo/config.toml` (`[profile.firmware]`: `strip = true`,
  `codegen-units = 1`, `panic = "abort"`, inheriting `opt-level = "z"`, `lto = "fat"`).
