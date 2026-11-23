<p align="center">
  <img src="assets/icons/transparent/orx-icon-transparent-256.png" alt="Orbic-Rayhunter icon" width="140" />
</p>

<h1 align="center">Orbic-Rayhunter</h1>

<p align="center">
  <img src="assets/branding/banner-1500x500.png" alt="A pocket hotspot inspecting nearby cell towers, one highlighted as suspicious" width="100%" />
</p>

<p align="center">
<a href="https://github.com/doublegate/Orbic-Rayhunter/actions/workflows/ci.yml"><img src="https://github.com/doublegate/Orbic-Rayhunter/actions/workflows/ci.yml/badge.svg" alt="CI"></a> <a href="https://github.com/doublegate/Orbic-Rayhunter/actions/workflows/docs.yml"><img src="https://github.com/doublegate/Orbic-Rayhunter/actions/workflows/docs.yml/badge.svg" alt="Docs"></a> <a href="https://github.com/doublegate/Orbic-Rayhunter/actions/workflows/security.yml"><img src="https://github.com/doublegate/Orbic-Rayhunter/actions/workflows/security.yml/badge.svg" alt="Security"></a> <a href="rust-toolchain.toml"><img src="https://img.shields.io/badge/rust-1.98.0-f97316.svg?logo=rust" alt="Rust 1.98.0"></a> <a href="Cargo.toml"><img src="https://img.shields.io/badge/unsafe__code-forbid-16a34a.svg" alt="unsafe_code: forbid"></a><br/>
<a href="#support-matrix"><img src="https://img.shields.io/badge/device-Orbic%20RC400L%20(MDM9207)-475569.svg" alt="Device: Orbic RC400L (MDM9207)"></a> <a href="docs/BUILD.md"><img src="https://img.shields.io/badge/target-armv7%20musleabihf-64748b.svg" alt="Target: armv7 musleabihf"></a> <a href="https://github.com/EFForg/rayhunter/releases/tag/v0.13.0"><img src="https://img.shields.io/badge/upstream-Rayhunter%20v0.13.0-0ea5e9.svg" alt="Upstream: Rayhunter v0.13.0"></a><br/>
<a href="LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg" alt="License: GPL-3.0-or-later"></a> <a href="#scope-and-legal"><img src="https://img.shields.io/badge/detection-passive%20%2F%20receive--only-22c55e.svg" alt="Detection: passive, receive-only"></a> <a href="PROJECT-STATUS.md"><img src="https://img.shields.io/badge/status-v0.6.0%20(Phase%204%3A%20orx%20backup)-f59e0b.svg" alt="Status: v0.6.0 (Phase 4: orx backup)"></a>
</p>

Install, harden and extend [EFF Rayhunter](https://github.com/EFForg/rayhunter), a passive
IMSI-catcher / cell-site-simulator detector, on the **Orbic Speed RC400L** LTE hotspot.

This repository is an overlay on upstream Rayhunter, not a fork: upstream is pinned as a git
submodule, and this project adds a patch series, host tooling, device configuration, reverse
engineering notes and documentation on top. Generic improvements are written to be submitted
back to EFForg/rayhunter.

> Status: **v0.6.0 (Phase 4: `orx doctor` v2, device transport, and `orx backup` -- validated
> on the unit).** Next: `orx verify`/`pull-logs`/`install`/`uninstall`, then the v1.0.0 install. Use the upstream release and the
> [official Orbic guide](https://efforg.github.io/rayhunter/orbic.html) until v1.0.0. Current
> state: [PROJECT-STATUS.md](PROJECT-STATUS.md).

## Support matrix

| Device | Firmware | Upstream pin | Status |
|---|---|---|---|
| Orbic Speed RC400L (Verizon), Qualcomm MDM9207 | `ORB400L_V1.2.0_BVZRT` | v0.13.0 | in development |

Other firmware builds and other Rayhunter-supported devices are out of scope until v1.0.0.

## What the research established

Findings from the Phase 0 research pass ([ref-docs/](ref-docs/)) that shape every later phase:

- **There is no software unbrick path.** EDL (`05c6:9008`) is reachable only through a hardware
  test point under the LCD, and no working firehose loader is confirmed for this device. A
  verified, checksummed MTD backup taken before the first write is the only recovery asset, so
  it is a hard gate ([docs/BACKUP-AND-RECOVERY.md](docs/BACKUP-AND-RECOVERY.md)).
- **Upstream's default installer is network-based.** `installer orbic` logs in to the stock web
  admin and gains a root shell through a command injection in `SetRemoteAccessCfg`;
  `installer orbic-usb` (vendor USB request, ADB) is a fallback that has bricked units. No public
  report covers the current installer on `V1.2.0` specifically.
- **Upstream replaces the stock `/etc/init.d/misc-daemon`** for persistence and its uninstall is
  manual and incomplete; disk-full conditions have wedged the hotspot. Both are addressed by
  `orx backup` / `orx restore` and retention work in v0.5 to v0.9.
- **Verizon ships firmware up to `V1.3.2`.** OTA updates are blocked until their effect on the
  install path is measured.
- **Detection gaps exist upstream.** A 2026 study of a commercial fake base station (Devilray)
  shows IMSI catching via NAS Reject goes unflagged; null integrity (EIA0), authentication
  failure/reject and IMSI paging are not checked; measurement data never reaches analyzers.
  These drive the v1.2 analyzer work ([docs/ANALYZERS.md](docs/ANALYZERS.md)).
- **Captures may include user traffic.** Upstream subscribes to DIAG log code `0x11EB` (user IP
  traffic). Whether that reaches QMDL is checked on-device in Phase 1 before any capture is shared.

## Quick start (development)

```sh
git clone --recurse-submodules https://github.com/doublegate/Orbic-Rayhunter
cd Orbic-Rayhunter
cargo run -p orx-cli -- doctor     # detect the Orbic over USB, probe its web UIs
scripts/apply-patches.sh           # stage upstream + patches/ in build/rayhunter
scripts/build.sh devel             # armv7 musl daemon + rootshell into dist/
```

Building details: [docs/BUILD.md](docs/BUILD.md). Installation (from v1.0.0):
[docs/INSTALL.md](docs/INSTALL.md). Take a full backup first:
[docs/BACKUP-AND-RECOVERY.md](docs/BACKUP-AND-RECOVERY.md).

## Architecture

`upstream/` (Rayhunter, pinned tag) + `patches/` (ordered series, each tagged `[UPSTREAM]` or
`[LOCAL]`) is staged into `build/rayhunter` and cross-compiled for `armv7-unknown-linux-musleabihf`
with upstream's own toolchain and build profiles. `crates/orx-cli` is the host tool (`orx`):
device detection now, then backup, install, verify and log retrieval. `device/` holds on-device
configuration, `re/` holds reverse-engineering notes (never firmware blobs). See
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/images/architecture-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="docs/images/architecture-light.svg">
    <img alt="Host overlay build pipeline and orx CLI connected over USB or Wi-Fi to the on-device Rayhunter runtime" src="docs/images/architecture-dark.svg" width="100%">
  </picture>
</p>

### Build pipeline

```mermaid
flowchart LR
    U["upstream/<br/>EFForg/rayhunter @ v0.13.0"] -->|git archive| S["build/rayhunter"]
    P["patches/series<br/>[UPSTREAM] then [LOCAL]"] -->|git apply --check, apply| S
    S -->|"cargo build-daemon-firmware[-devel]<br/>armv7-unknown-linux-musleabihf"| D["dist/<br/>rayhunter-daemon, rootshell, SHA256SUMS"]
    D -->|orx install| O["Orbic RC400L<br/>/data/rayhunter"]
    C["device/config/config.orbic.toml"] -->|orx install| O
```

### Detection pipeline

<p align="center">
  <img src="docs/images/detection-pipeline.svg" alt="Modem DIAG log stream to QMDL recording, LTE RRC/NAS parser, analyzers, debounced warning, and web UI, display or ntfy" width="100%" />
</p>

The analyzers shown are upstream's default heuristics; planned additions (NAS reject, authentication
failure, EIA0, IMSI paging and others) are tracked in [docs/ANALYZERS.md](docs/ANALYZERS.md).

### Install safety gate

```mermaid
flowchart TD
    A["orx doctor<br/>device found, firmware recorded"] --> S["Root shell only, no writes<br/>(installer util orbic-shell)"]
    S --> B{"Verified full backup?<br/>MTD dumps + SHA-256 on host and off-repo"}
    B -- no --> C["orx backup"] --> B
    B -- yes --> D["Block OTA updates"]
    D --> E["Upstream network installer<br/>(installer orbic)"]
    E --> F["orx install: patched daemon + config"]
    F --> G{"orx verify<br/>:8080 up, recording, survives reboot"}
    G -- fail --> H["orx restore<br/>stock misc-daemon, remove daemon"]
    G -- pass --> I["Done"]
```

## Roadmap

<p align="center">
  <img src="docs/images/release-roadmap.svg" alt="Release timeline from v0.1.0 Foundation through v1.0.0 Baseline to v2.0.0 Enhanced" width="100%" />
</p>

Details, ticket IDs and exit criteria: [ROADMAP.md](ROADMAP.md).

## Repository layout

| Path | Contents |
|---|---|
| `upstream/` | EFForg/rayhunter submodule, pinned to `v0.13.0`; never edited |
| `patches/` | `series` plus `NNNN-*.patch` files tagged `[UPSTREAM]` or `[LOCAL]` |
| `crates/orx-cli/` | the `orx` host tool |
| `scripts/` | `apply-patches.sh`, `build.sh`, `sync-upstream.sh`, `gen-assets.sh` |
| `device/` | on-device configuration (`config/config.orbic.toml`) |
| `re/` | reverse-engineering notes (no firmware binaries) |
| `backups/` | local-only device backups (gitignored) |
| `assets/`, `docs/images/` | artwork, icons and diagrams |
| `docs/`, `ref-docs/`, `to-dos/` | specification, research corpus, phase plans |

## Documentation

| Topic | Document |
|---|---|
| Current state, risks, next steps | [PROJECT-STATUS.md](PROJECT-STATUS.md) |
| Releases v0.1.0 to v2.0.0, tickets T-OR-001 to T-OR-105 | [ROADMAP.md](ROADMAP.md) |
| Architecture and decisions | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), [docs/adr/](docs/adr/) |
| Device facts (measured vs TBD) | [docs/DEVICE-PROFILE.md](docs/DEVICE-PROFILE.md) |
| Backup, recovery, install | [docs/BACKUP-AND-RECOVERY.md](docs/BACKUP-AND-RECOVERY.md), [docs/INSTALL.md](docs/INSTALL.md) |
| Configuration and analyzers | [docs/CONFIGURATION.md](docs/CONFIGURATION.md), [docs/ANALYZERS.md](docs/ANALYZERS.md) |
| Build, patches, CI, testing, performance | [docs/BUILD.md](docs/BUILD.md), [docs/PATCH-POLICY.md](docs/PATCH-POLICY.md), [docs/CI.md](docs/CI.md), [docs/TESTING.md](docs/TESTING.md), [docs/PERFORMANCE.md](docs/PERFORMANCE.md) |
| Security model, RE method, releases | [docs/SECURITY-MODEL.md](docs/SECURITY-MODEL.md), [docs/RE-METHODOLOGY.md](docs/RE-METHODOLOGY.md), [docs/RELEASE-PROCESS.md](docs/RELEASE-PROCESS.md) |
| Research corpus | [ref-docs/](ref-docs/) (upstream, install, hardware, firmware, DIAG/QMDL, detection literature, legal) |
| Phase and sprint plans | [to-dos/](to-dos/) |

## Scope and legal

Rayhunter is receive-only: it reads the hotspot's own modem diagnostics and never transmits on
licensed spectrum. Captures can contain identifiers (IMSI, IMEI); treat them as personal data.
See [ref-docs/legal-and-ethics.md](ref-docs/legal-and-ethics.md). This is not legal advice.

Modifying the device can brick it. This project's procedures require a verified full backup
before any write.

## License

GPL-3.0-or-later, matching upstream Rayhunter (GPL-3.0). See [LICENSE](LICENSE). Rayhunter is a
project of the Electronic Frontier Foundation; this project is independent and not affiliated
with or endorsed by EFF or Orbic.
