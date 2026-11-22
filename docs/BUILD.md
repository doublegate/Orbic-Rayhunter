# Build

## Prerequisites

- rustup (the toolchains are installed from `rust-toolchain.toml` files: 1.98.0 for both this
  workspace and upstream v0.13.0).
- Node.js 22 and npm (upstream's `daemon/web` frontend is embedded in the daemon).
- For the `firmware` profile only: an `arm-linux-musleabihf` cross compiler (for example from
  musl.cc) or the `messense/rust-musl-cross:armv7-musleabihf` container, because the `pq-tls`
  backend compiles C. The `devel` profile needs only rustup.

## Host tool

```sh
cargo build -p orx-cli            # target/debug/orx
cargo test
```

## Device binaries

```sh
git submodule update --init
scripts/build.sh devel            # firmware-devel profile, rustcrypto TLS backend
scripts/build.sh firmware         # firmware profile, pq-tls backend (needs cross gcc)
```

Outputs: `dist/rayhunter-daemon`, `dist/rootshell`, `dist/SHA256SUMS`.

Profiles come from `upstream/.cargo/config.toml` at the pinned tag:

| Profile | Optimization | Notes |
|---|---|---|
| `firmware-devel` | `opt-level = "s"`, no LTO | faster to build, slightly larger |
| `firmware` | `opt-level = "z"`, fat LTO, `codegen-units = 1`, `panic = "abort"`, stripped | release builds |

## Reproducibility

Phase 3 compares our unpatched `firmware` build against the upstream release binary of the same
tag (size, section layout, and byte identity where the toolchain allows). Results are recorded
in [PERFORMANCE.md](PERFORMANCE.md) and the Phase 3 to-do.
