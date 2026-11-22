# Architecture

Specification of how Orbic-Rayhunter is put together. Upstream internals are summarized in
[ref-docs/rayhunter-upstream-overview.md](../ref-docs/rayhunter-upstream-overview.md).

## Components

```text
                        host (Linux)                                     Orbic RC400L (armv7, MDM9207)
 +---------------------------------------------------+        +------------------------------------------+
 | upstream/  (EFForg/rayhunter @ pinned tag)        |        | /data/rayhunter/                         |
 |      |  git archive                               |        |   rayhunter-daemon  <- patched build     |
 |      v                                            |  USB   |   config.toml       <- device/config/    |
 | build/rayhunter  <- patches/series applied        | RNDIS  |   qmdl/             recordings + ndjson  |
 |      |  cargo (upstream toolchain + profiles)     | / ADB  | /etc/init.d/misc-daemon (replaced; stock |
 |                       copy backed up)    |
 |      v                                            | <----> | /etc/init.d/rayhunter_daemon             |
 | dist/ rayhunter-daemon, rootshell, SHA256SUMS     |        |                                          |
 |                                                   |        | /dev/diag -> daemon -> analyzers         |
 | orx (crates/orx-cli): doctor, backup, install,    |        |   -> web UI :8080, ntfy, WebDAV upload   |
 |      verify, pull-logs, restore                   |        +------------------------------------------+
 +---------------------------------------------------+
```

## Responsibilities

| Unit | Owns | Does not own |
|---|---|---|
| `upstream/` | Rayhunter source at an exact tag | anything Orbic-Rayhunter-specific |
| `patches/` | every change to Rayhunter, as an ordered series | build orchestration |
| `scripts/` | staging, building and upstream sync | device interaction |
| `crates/orx-cli` | all host-to-device interaction (detection, backup, install, verify, logs) | reimplementing upstream's installer: it calls it |
| `device/` | files placed on the device beyond the daemon | firmware images |
| `re/` | reverse-engineering findings | firmware binaries |

Dependency direction is one-way: `orx-cli` -> (`dist/` artifacts, upstream `installer`) ->
device. `scripts/` never talks to the device. Patches never depend on `orx-cli`.

## Build pipeline

1. `scripts/apply-patches.sh`: `git archive` of the submodule's committed tree into a scratch
   directory, then each `patches/series` entry with `git apply --check` first, then a
   content-compared sync (`rsync --checksum --delete`) into `build/rayhunter` that leaves
   `target/` and `node_modules/` alone. Only files whose content changed are rewritten (with a
   fresh mtime), so Cargo rebuilds exactly what changed. The submodule working tree is never
   modified.
2. `scripts/build.sh devel|firmware`: upstream's `rust-toolchain.toml`, `npm run build -w
   daemon/web`, then upstream's cargo aliases (`build-daemon-firmware[-devel]`,
   `build-rootshell-firmware[-devel]`) for `armv7-unknown-linux-musleabihf`.
3. Artifacts and `SHA256SUMS` in `dist/`. CI uploads them; `release.yml` attaches them to tags.

## Install path

v1.0.0 reuses upstream's installer to obtain root and install: `installer orbic` (network: web
admin login, then a root shell via the `SetRemoteAccessCfg` injection; `orbic-network` is an alias)
is the primary path, and `installer orbic-usb` (vendor USB request, ADB, `/bin/rootshell`) is a
documented fallback only, because it has bricked units. Then `orx install` replaces the daemon with the `dist/` build and the config
with `device/config/config.orbic.toml`. `orx` wraps these steps with a mandatory backup check
and post-install verification. Details: [INSTALL.md](INSTALL.md).

## Extension points for v1.x

- Analyzers: new heuristics are upstream-style analyzers added by `[UPSTREAM]` patches under
  `lib/src/analysis/`, enabled through the existing `[analyzers]` config table.
- Notifications: upstream ships ntfy; additional sinks extend the same notification path.
- Host pipeline: `orx pull-logs` and later aggregation stay host-side to keep the device lean.

Decisions: [adr/0001-overlay-submodule.md](adr/0001-overlay-submodule.md),
[adr/0002-gpl-licensing.md](adr/0002-gpl-licensing.md).
