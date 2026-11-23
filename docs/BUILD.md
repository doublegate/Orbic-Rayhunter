# Build

## Prerequisites

- rustup (the toolchains are installed from `rust-toolchain.toml` files: 1.98.0 for both this
  workspace and upstream v0.13.0).
- Node.js 22 and npm (upstream's `daemon/web` frontend is embedded in the daemon).
- For the `firmware` profile only: `docker` (or `podman`). `scripts/build.sh firmware` runs the
  cargo build inside the `messense/rust-musl-cross:armv7-musleabihf` container -- the same one
  upstream uses -- because the `pq-tls` backend compiles C and needs an `arm-linux-musleabihf`
  cross toolchain. The web UI is still built on the host, so Node.js is needed for both profiles.
  The `devel` profile needs only rustup + npm (pure Rust, no container).

Staging needs `rsync` (content-compared sync into `build/rayhunter`).

## Host tool

```sh
cargo build -p orx-cli            # target/debug/orx
cargo test
```

## Device binaries

```sh
git submodule update --init
scripts/build.sh devel            # firmware-devel profile, rustcrypto TLS (pure Rust)
scripts/build.sh firmware         # firmware profile, pq-tls (reproducible, in a container)
```

Outputs: `dist/rayhunter-daemon`, `dist/rootshell`, `dist/SHA256SUMS`.

Profiles come from `upstream/.cargo/config.toml` at the pinned tag:

| Profile | Optimization | Notes |
|---|---|---|
| `firmware-devel` | `opt-level = "s"`, no LTO | faster to build, slightly larger |
| `firmware` | `opt-level = "z"`, fat LTO, `codegen-units = 1`, `panic = "abort"`, stripped | release builds |

## Reproducibility

Phase 3 compared our unpatched `firmware` build against the upstream release binary of the same
tag. Result (2026-11-23): our container build reproduces upstream's release daemon to within
**384 bytes** (6,109,892 vs 6,109,508) -- same profile, toolchain, lockfile, pq-tls and
container -- but is **not** bit-identical (embedded absolute build paths differ). Full method,
hashes and interpretation are in [../re/notes/reproducible-build.md](../re/notes/reproducible-build.md).
Bit-for-bit identity would need `--remap-path-prefix`/`trim-paths` (an upstreamable follow-up).

Since v0.4.0, `release.yml` ships the production `firmware` daemon (~6.1 MB), not the
`firmware-devel` build (~16 MB) that v0.1.0-v0.3.0 shipped.
