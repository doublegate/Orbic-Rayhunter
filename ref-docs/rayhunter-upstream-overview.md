# Rayhunter Upstream Overview

- Retrieved: 2026-11-22
- Upstream ref: v0.13.0 (cd462a1)
- Upstream repository: <https://github.com/EFForg/rayhunter> (local read-only checkout: `./upstream`)
- Toolchain: Rust 1.98.0 pinned in `upstream/rust-toolchain.toml` (components `rustfmt`, `clippy`); Rust edition 2024

This document is a reference for the Orbic-Rayhunter project. It describes what upstream already
provides so that this project extends rather than duplicates it. Statements are sourced from the
v0.13.0 tree unless marked otherwise.

---

## 1. Workspace and crate responsibilities

Workspace members (`upstream/Cargo.toml`): `lib`, `daemon`, `check`, `rootshell`, `telcom-parser`,
`installer`, `installer-gui/src-tauri`. `installer-gui` is excluded from `default-members`
(experimental; extra OS dependencies). Resolver 2.

| Crate (path) | Package / binary | Runs on | Responsibility |
|---|---|---|---|
| `lib/` | `rayhunter` (library) | device and host | Core library. Modules: `diag` (DIAG protocol message types, `diaglog`), `diag_device` (opens `/dev/diag`, ioctls `DIAG_IOCTL_SWITCH_LOGGING`=7 and `DIAG_IOCTL_REMOTE_DEV`=32, retrieves log ID ranges, sets log masks; unix only), `hdlc` (HDLC framing), `qmdl` (QMDL reader/writer), `gsmtap` (+ `parser`, `mac`: DIAG log -> GSMTAP conversion), `pcap` (`GsmtapPcapWriter`, PCAP-NG output with GPS comments), `log_codes`, `plmn`, `clock`, `sim` (`generic_at`: reads SIM EF_HPLMNwAcT `6F62` for home PLMN; unix only), `analysis` (analyzer trait, harness and all heuristics), `util`. Defines `Device` enum: `orbic`, `tplink`, `tmobile`, `wingtech`, `pinephone`, `uz801`, `moxee`, and `DeviceMetadata { home_plmn }`. Re-exports `telcom_parser`. |
| `telcom-parser/` | `telcom-parser` | both | ASN.1-generated LTE RRC parser (`src/lte_rrc.rs`) built with the hampi `asn1-compiler`/`asn1-codecs` 0.7.1 from specs in `telcom-parser/specs`. |
| `daemon/` | `rayhunter-daemon` (bin), `gen_api` (bin, `apidocs` feature) | device | Long-running service: DIAG read loop, recording store, live analysis, axum HTTP server + embedded SvelteKit web UI (`daemon/web`), display driver, key input, battery monitor, ntfy notifications, update checker, WiFi client (`wifi-station` crate 0.10.1), GPS ingestion, WebDAV uploader. Features: `rustcrypto-tls` (default), `pq-tls` (release firmware builds), `apidocs` (utoipa OpenAPI generation). |
| `check/` | `rayhunter-check` | host (Linux/macOS/Windows) | Offline analyzer CLI over QMDL or PCAP files/directories. Flags: `-p/--path`, `-P/--pcapify` (convert QMDL to PCAP after analysis), `--show-skipped`, `-j/--json <file>`, `-q/--quiet`, `-d/--debug`. |
| `rootshell/` | `rootshell` | device (Orbic USB installer only) | Tiny setuid helper installed at `/bin/rootshell`: `setgroups([3003 AID_INET, 3004 AID_NET_RAW])` then `exec /bin/bash` as uid/gid 0 with passed args. |
| `installer/` | `installer` (bin + lib) | host | Per-device installers and utilities; embeds `rayhunter-daemon`, `rootshell`, `dist/config.toml.in`, `dist/scripts/rayhunter_daemon`, `dist/scripts/misc-daemon`, and optionally `wpa_supplicant`/`wpa_cli`/`iw` (from `tools/build-wpa-supplicant/out`) at build time via `build.rs` env vars `FILE_*`. See `rayhunter-install-orbic.md`. |
| `installer-gui/` | Tauri app | host | Experimental GUI wrapping the installer library (`installer::run_with_callback`); MVP frontend landed in v0.13.0 (#1116, #1124). |

Other top-level items: `dist/` (config template and init scripts), `doc/` (mdBook, `book.toml`),
`scripts/` (`build-dev.sh`, `install-dev.sh`, `build-wpa-supplicant.sh`, `set-versions.sh`),
`make.sh` (dev loop: build web, `cargo build-daemon-firmware-devel`, `adb push`, reboot via
rootshell), `docker_make.sh`, `tools/`.

---

## 2. Data flow

```text
Qualcomm modem (MDM9207 on RC400L)
   | /dev/diag  (ioctl switch to memory-device logging, set log masks)
   v
lib::diag_device::DiagDevice  -- HDLC-deframed MessagesContainer stream
   v
daemon::diag (DiagTask::process_container)         only DataType::UserSpace containers
   |-- disk-space check every DISK_CHECK_BYTES_INTERVAL
   |-- QmdlWriter.write_container -> <store>/<ts>.qmdl  (or .qmdl.gz, compressed QMDL since v0.12.0 #970)
   |-- manifest.toml entry size update
   |-- AnalysisWriter.analyze_container -> lib::analysis Harness
   |        DIAG log -> GSMTAP -> InformationElement (LTE RRC / NAS / ...) -> each enabled Analyzer
   |        -> <store>/<ts>.ndjson  (line 1 ReportMetadata, then AnalysisRow lines)
   |-- max EventType > Informational -> Notification(Warning, debounce 5 min) + DisplayState::WarningDetected
   v
HTTP (axum, 0.0.0.0:<port>)
   |-- /api/qmdl/{name}      raw QMDL download
   |-- /api/pcap/{name}      PCAP-NG generated on demand from QMDL (+ <ts>-gps.ndjson as packet comments)
   |-- /api/zip/{name}       QMDL + PCAP + ndjson (ndjson added v0.13.0 #1100)
   |-- /api/analysis-report  ndjson report
   v
Web UI (SvelteKit, embedded static files) / rayhunter-check (offline re-analysis)
```

Key points:

- PCAP is not stored; it is rendered from QMDL on request (`daemon/src/pcap.rs`).
- Store file kinds (`daemon/src/qmdl_store.rs`): `<entry>.qmdl` or `<entry>.qmdl.gz`,
  `<entry>.ndjson` (analysis), `<entry>-gps.ndjson` (GPS). Index file: `<store>/manifest.toml`.
  `RecordingStore::recover` rebuilds the manifest from `<timestamp>.qmdl[.gz]` files if it is lost.
- Re-analysis of past recordings with current analyzers: `POST /api/analysis/{name}` (UI
  "reanalyze" button since v0.6.0).
- Report format version: `REPORT_VERSION = 2` (`lib/src/analysis/analyzer.rs`); each analyzer
  also carries its own version, recorded in `ReportMetadata.analyzers`.
- Severity enum `EventType`: `Informational`, `Low`, `Medium`, `High`. Informational events never
  change the display or trigger warning notifications.

---

## 3. Daemon runtime on the Orbic

### 3.1 Paths

| Path | Purpose |
|---|---|
| `/data/rayhunter` | Symlink (installer v0.13.0 network path) to the real data directory, default `/data/rayhunter-data` (override `--data-dir`). Older installs: a real directory. |
| `/data/rayhunter/rayhunter-daemon` | Daemon binary (armv7 musl, static). |
| `/data/rayhunter/config.toml` | Configuration (TOML). |
| `/data/rayhunter/qmdl/` | Recording store (`qmdl_store_path`), contains `manifest.toml`. |
| `/data/rayhunter/rayhunter.log` | stdout/stderr of the daemon (`RUST_LOG=info`). Viewable via `GET /api/log`. |
| `/data/rayhunter/bin/` | Bundled `wpa_supplicant`, `wpa_cli`, `iw` (only if not already on device); `resolve_bin` looks here for `wpa_supplicant`, `iw`, `udhcpc`. |
| `/data/rayhunter/scripts/` | Created by installer (scripts dir). |
| `/data/rayhunter/wpa_sta.conf` | WiFi client credentials (never returned via API). |
| `/data/rayhunter/udhcpc-hook.sh`, `/data/rayhunter/dhcp_lease` | WiFi client DHCP. |
| `/data/rayhunter/crash-logs/` | WiFi driver (`wlan.ko`) crash snapshots (dmesg, /proc/modules, ip addr, ps). |
| `/etc/init.d/rayhunter_daemon` | Init script (from `dist/scripts/rayhunter_daemon`). |
| `/etc/init.d/misc-daemon` | Replaced by installer with `dist/scripts/misc-daemon`, which starts `rayhunter_daemon` at boot. |
| `/tmp/rayhunter.pid` | PID file (start-stop-daemon). |
| `/bin/rootshell` | Only for `orbic-usb` installs. |
| `/dev/diag`, `/dev/fb0` (128x128 RGB565), `/dev/input/event0` | Modem DIAG, display framebuffer, power-button events. |
| `/sys/kernel/chg_info/level`, `/sys/kernel/chg_info/chg_en` | Orbic battery level (coarse buckets 1..5 -> 10/25/50/75/100 percent) and charging state. |

### 3.2 Init and persistence

`dist/scripts/rayhunter_daemon` (`start|stop|restart`):

```sh
start-stop-daemon -S -b --make-pidfile --pidfile /tmp/rayhunter.pid \
  --startas /bin/sh -- -c "RUST_LOG=info exec /data/rayhunter/rayhunter-daemon /data/rayhunter/config.toml > /data/rayhunter/rayhunter.log 2>&1"
```

A `#RAYHUNTER-PRESTART` marker line may be replaced by device-specific commands (e.g. SD mount on
TP-Link). There is no supervisor/respawn: if the daemon exits, it stays down until reboot or manual
restart.

`dist/scripts/misc-daemon` replaces the stock Orbic `misc-daemon` init script. On `start` it waits
up to about 10 s for the `modem` subsystem in `/sys/bus/msm_subsys/devices/*/state` to read
`ONLINE`, then starts (if present) `init_qcom_audio`, `/sbin/reboot-daemon`, `start_atfwd_daemon`,
`rayhunter_daemon`, `start_stop_qti_ppp_le`, `start_loc_launcher`. This is the boot-persistence
hook: the stock firmware calls `misc-daemon` at boot, and the replacement calls `rayhunter_daemon`.
Risk to note: the replacement is a fixed copy of a stock script; if a firmware version ships a
different stock `misc-daemon`, its extra steps are lost.

### 3.3 Network

- HTTP server binds `0.0.0.0:<port>`, default port `8080` (`daemon/src/main.rs::run_server`).
  On Orbic the UI is at `http://192.168.1.1:8080` over the hotspot WiFi, or via USB with
  `adb forward tcp:8080 tcp:8080` (USB/ADB installs only).
- No authentication on the HTTP API. Anyone on the hotspot LAN (or the WiFi-client LAN when
  client mode is enabled) can read recordings and change config.
- `debug_mode = true` skips DIAG, display, key input, battery and WiFi workers (for PC runs).

### 3.4 Background workers started by `run_with_config`

DIAG read thread; display (`display::orbic::update_ui` for Orbic and Moxee, via
`generic_framebuffer`); key input (if `key_input_mode = 1`); update checker (if
`auto_check_updates`); analysis thread (re-analysis queue); shutdown/restart handler (config save
restarts the daemon in-process via cancellation tokens); battery notification worker; ntfy
notification worker; WiFi client (`wifi_station::run_wifi_client`, with `wlan.ko` crash watchdog,
up to 5 recovery attempts); WebDAV upload worker (if `webdav.url` non-empty); HTTP server.

---

## 4. Configuration (`config.toml`)

Source of truth: `daemon/src/config.rs` (`Config::default()`, `#[serde(default)]`, so any missing
key takes the code default). `dist/config.toml.in` is the template written by the installer, with
`#device = "orbic"` rewritten to `device = "<type>"`. The installer does not overwrite an existing
config unless `--reset-config` is passed (behavior since v0.10.0, #780).

| Key | Type | Code default | Template (`config.toml.in`) | Notes |
|---|---|---|---|---|
| `qmdl_store_path` | string | `/data/rayhunter/qmdl` | same | |
| `port` | u16 | `8080` | same | |
| `debug_mode` | bool | `false` | same | |
| `device` | enum | `orbic` | set by installer | `orbic`, `tplink`, `tmobile`, `wingtech`, `pinephone`, `uz801`, `moxee` |
| `ui_level` | u8 | `1` (Subtle) | `1` | 0 Invisible, 1 Subtle (colored top line), 2 Demo (orca GIF), 3 EFF logo, 4 High Visibility (full screen, v0.10.0), 128 TransFlag (undocumented) |
| `colorblind_mode` | bool | `false` | same | green -> blue |
| `key_input_mode` | u8 | `0` | `0` | 1 = double-tap power button restarts recording |
| `ntfy_url` | string? | none | commented | any URL accepting POST body |
| `enabled_notifications` | list | `["Warning","LowBattery"]` | same | also `Update` |
| `auto_check_updates` | bool | `true` | `false` | Discrepancy: fresh installs get `false` from the template; a config missing the key gets `true`. |
| `clock_sync_mode` | u8 | `2` (Prompt) | `2` | 0 Off, 1 Autosync (in-memory offset only, > 30 s drift), 2 Prompt |
| `min_space_to_start_recording_mb` | u64 | `1` | `1` | |
| `min_space_to_continue_recording_mb` | u64 | `1` | `1` | recording stops automatically below this |
| `gps_mode` | u8 | `0` | absent | 0 Disabled, 1 Fixed, 2 Api (`POST /api/gps`) |
| `gps_fixed_latitude` / `gps_fixed_longitude` | f64? | none | absent | -90..90 / -180..180 |
| `wifi_enabled` | bool | `false` | `false` | WiFi client mode (Orbic, Moxee, UZ801, TMOHS1, Wingtech) |
| `wifi_ssid`, `wifi_security`, `wifi_password` | | none | absent | read from `wpa_sta.conf`; password never exposed; `wifi_security` = `wpa_psk` or `sae` |
| `dns_servers` | list? | none (effective Quad9 `9.9.9.9`, `149.112.112.112`) | commented | |
| `[webdav]` | table | `url=""` | commented | `url`, `username`, `password`, `upload_timeout_secs=300`, `poll_interval_secs=3600`, `min_age_secs=86400`, `delete_on_upload=false`; config-file only, no UI |
| `[analyzers]` | table | see section 5 | same as code | per-analyzer booleans |

---

## 5. Analyzers / heuristics

Implemented in `upstream/lib/src/analysis/`, enabled by `[analyzers]` (`AnalyzerConfig`). Shared
modules: `analyzer.rs` (trait `Analyzer` with `get_name`, `get_description`, `get_version`,
`analyze_information_element`; `Harness`; `EventType`; report/row types), `information_element.rs`
(decoded IE enum over LTE RRC / NAS etc.), `util.rs`.

| Config key | Source file | Analyzer name (code) | Code version | Default | Detects |
|---|---|---|---|---|---|
| `imsi_requested` | `imsi_requested.rs` | Identity (IMSI or IMEI) requested in suspicious manner | 5 (doc heading says v3) | on | NAS Identity Request for IMSI/IMEI in a suspicious sequence: new cell -> identity request -> no authentication -> release; also identity request after auth, without connection, or without auth following. Also emits informational events on every identity disclosure. v0.12.0 fixed roaming false positives (#1014); v0.13.0 reads home PLMN from the SIM (#1101). |
| `connection_redirect_2g_downgrade` | `connection_redirect_downgrade.rs` | Connection Release/Redirected Carrier 2G Downgrade | 1 | on | RRCConnectionRelease with redirected carrier info pointing to GERAN (2G). v0.13.0 fixed a commented-out RRCConnectionRelease classifier (#1149/#1150). |
| `lte_sib6_and_7_downgrade` | `priority_2g_downgrade.rs` | LTE SIB 6/7 Downgrade | 2 | on | SIB6/SIB7 advertising 2G/3G neighbors with higher reselection priority than LTE. v0.10.0 fixed European false positives. |
| `null_cipher` | `null_cipher.rs` | Null Cipher | 1 | on | RRC security mode command selecting EEA0. |
| `nas_null_cipher` | `nas_null_cipher.rs` | NAS Null Cipher Requested | 1 | on | NAS security mode command selecting EEA0. |
| `incomplete_sib` | `incomplete_sib.rs` | Incomplete SIB | 2 | on | SIB1 scheduling info lacking a full chain of follow-up SIBs (fake cells often send only SIB1 + one). Downgraded to informational severity in v0.7.0 (#619). |
| `diagnostic_analyzer` | `diagnostic.rs` | Diagnostic detector for messages which might lead to IMSI exposure | 1 | on | Informational tower connect/disconnect and IMSI-exposure-relevant messages; noisy, informational only. |
| `no_nas_messages` | `no_nas_messages.rs` | No NAS Messages | 1 | off | Once per recording: 5 minutes of DIAG traffic (recording timestamps) with zero NAS messages; indicates missing/inactive SIM. New in v0.13.0 (#1132). Separately the web UI warns if no DIAG message arrives for 5 minutes. |
| `test_analyzer` | `test_analyzer.rs` | Test Analyzer | 1 | off | Alerts on every SIB1; installation smoke test only. |

Upstream does not currently ship analyzers for, e.g., 5G NR/NSA, cell-ID/TAC anomaly tracking
across recordings, timing-advance or signal-strength anomalies, or paging-based detections. v0.12.0
added serving/neighbor cell measurements and MAC RACH timing-advance message parsing (#1074), which
is raw material for such analyzers but no heuristic consumes it yet (verify before building on it).

---

## 6. Notifications

Implemented in `daemon/src/notifications.rs`, `daemon/src/battery/mod.rs`, `daemon/src/update.rs`,
`daemon/src/diag.rs`.

- Transport: HTTP `POST <ntfy_url>` with the message as the plain-text body, 10 s timeout
  (`DEFAULT_NOTIFICATION_TIMEOUT`; timeout added in v0.11.0 #979). No ntfy-specific headers
  (title, priority, tags) and no auth headers are set; any endpoint that accepts a POST body works.
  The topic in the URL is the only secret.
- Types (`NotificationType`): `Warning`, `LowBattery`, `Update`. Filtered by
  `enabled_notifications`.
- Per-type state with debounce, retry and failure counting; queue is an mpsc channel of 10.
- Triggers:
  - Heuristic event above Informational: "Rayhunter has detected a {severity} severity event",
    debounce 5 minutes.
  - Disk space low (warning threshold): debounce 30 s, sent once per recording; critically low:
    recording stopped plus notification. Both use type `Warning`.
  - Low battery: level <= 10 percent while unplugged (`LOW_BATTERY_LEVEL = 10`); re-arms after
    plugged in and above 10. On Orbic the level is coarse (10/25/50/75/100).
  - Update available: GitHub `releases/latest` polled every 6 h when `auto_check_updates` is on;
    pre-release tags are ignored.
- Test: `POST /api/test-notification` (UI "Test" button since v0.10.0).
- Delivery requires the device to have internet: an active SIM data plan, or WiFi client mode
  (v0.11.0).

---

## 7. HTTP API

Router in `daemon/src/main.rs::get_router`. OpenAPI/Swagger UI is generated with utoipa
(`apidocs` feature, `daemon/src/bin/gen_api.rs`) and published in the mdBook under `api-docs/`
(`doc/api-docs.md`, `doc/swagger-ui.html`). Endpoints are "subject to change".

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/pcap/{name}` | PCAP-NG for a recording (generated from QMDL) |
| GET | `/api/qmdl/{name}` | Raw QMDL |
| GET | `/api/zip/{name}` | ZIP of QMDL + PCAP + ndjson |
| GET | `/api/system-stats` | Disk, memory, runtime metadata, battery |
| GET | `/api/update-status` | Update checker status |
| GET | `/api/qmdl-manifest` | Recording list + current entry |
| GET | `/api/log` | Daemon log |
| POST | `/api/start-recording` | Start new recording |
| POST | `/api/stop-recording` | Stop recording |
| POST | `/api/delete-recording/{name}` | Delete a recording |
| POST | `/api/delete-all-recordings` | Delete all |
| GET | `/api/analysis-report/{name}` | ndjson analysis report |
| GET | `/api/analysis` | Re-analysis queue status |
| POST | `/api/analysis/{name}` | Queue re-analysis |
| GET / POST | `/api/config` | Read / write config (POST restarts daemon logic) |
| POST | `/api/test-notification` | Send test notification |
| GET | `/api/wifi-status` | WiFi client status |
| POST | `/api/wifi-scan` | Scan for networks |
| GET | `/api/time` | Device time |
| POST | `/api/time-offset` | Set in-memory clock offset |
| POST | `/api/debug/display-state` | Force display state (debug) |
| GET / POST | `/api/gps` | Latest GPS fix / submit fix (POST only when `gps_mode = 2`) |
| GET | `/` | Redirect to `/index.html` |
| GET | `/{*path}` | Static web UI |

---

## 8. Display and input (Orbic)

- `/dev/fb0`, hardcoded 128x128, RGB565 little-endian (`daemon/src/display/orbic.rs`, TODO notes
  dimensions are not probed).
- States: `Recording` (green, or blue in colorblind mode), `Paused` (white), `WarningDetected`
  (red). Rendering per `ui_level`.
- Power button: `/dev/input/event0`, debounced; double-tap restarts recording when
  `key_input_mode = 1`.

---

## 9. Build profiles and aliases (`upstream/.cargo/config.toml`)

| Alias | Expands to |
|---|---|
| `build-daemon-firmware` | `build -p rayhunter-daemon --bin rayhunter-daemon --target armv7-unknown-linux-musleabihf --profile firmware --no-default-features --features pq-tls` (needs an arm-linux-musleabihf C cross toolchain, e.g. musl.cc or `messense/rust-musl-cross:armv7-musleabihf`, which CI uses) |
| `build-daemon-firmware-devel` | `build -p rayhunter-daemon --bin rayhunter-daemon --target armv7-unknown-linux-musleabihf --profile firmware-devel` (rustcrypto TLS, Rust toolchain only, slightly larger) |
| `build-rootshell-firmware` | `build -p rootshell --bin rootshell --target armv7-unknown-linux-musleabihf --profile firmware` |
| `build-rootshell-firmware-devel` | same with `--profile firmware-devel` |

Profiles: `release` (`debug="limited"`, `lto="fat"`, `opt-level="z"`, `strip="debuginfo"`);
`firmware-devel` (inherits release, `opt-level="s"`, `lto=false`); `firmware` (inherits release,
`strip=true`, `codegen-units=1`, `panic="abort"`, `debug=false`).

Targets: all musl/darwin targets link with `rust-lld` and `+crt-static`;
`armv7-unknown-linux-gnueabihf` uses `arm-linux-gnueabihf-gcc`. x86_64 macOS rust-lld is disabled
(linker crash).

The installer `build.rs` selects the daemon/rootshell profile via `FIRMWARE_PROFILE` (defaults to
`firmware` for release builds) and reads binaries from
`target/armv7-unknown-linux-musleabihf/<profile>/`; wpa tools from
`tools/build-wpa-supplicant/out`, embedding empty values if absent (wifi tools then skipped).

Dev scripts: `scripts/build-dev.sh` then `scripts/install-dev.sh orbic`; frontend hot reload with
`API_TARGET=http://192.168.1.1:8080 npm run dev` in `daemon/web`.

---

## 10. CI (`upstream/.github/workflows`)

`main.yml` (push and pull_request; also `workflow_call` from release):

- `files_changed`: path filter producing outputs (`docs_changed`, `code_changed`,
  `daemon_changed`, `daemon_needed`, `rootshell_needed`, `installer_changed`,
  `installer_build`, `installer_gui_changed`, `web_changed`).
- `mdbook_test` / `mdbook_build`: `mdbook test`, `mdbook build`, Pages artifact.
- `check_and_test` (ubuntu): `cargo fmt --all --check`; `npm install && npm run build -w daemon/web && cargo check --verbose`; `cargo test --verbose`; `cargo clippy --verbose` (no `-D warnings` on the command line; host target only, so armv7-only code paths are not clippy-checked here).
- `installer_gui_check` (macOS): `cargo check`/`clippy -p installer-gui`.
- `test_daemon_frontend`: `npm run lint|check|test -w daemon/web`.
- `test_installer_frontend`: lint/check for `installer-gui`.
- `windows_installer_check_and_test` (windows-latest).
- `build_rayhunter_check`: matrix x86_64-linux-musl, armv7-linux-musleabi, aarch64-linux-musl,
  aarch64-apple-darwin, x86_64-apple-darwin, x86_64-pc-windows-gnu.
- `build_rootshell`: armv7-unknown-linux-musleabihf, profile firmware.
- `build_wpa_supplicant`: `gcc-arm-linux-gnueabihf` + `scripts/build-wpa-supplicant.sh`.
- `build_rayhunter`: inside `messense/rust-musl-cross:armv7-musleabihf`, `cargo build-daemon-firmware`.
- `build_rust_installer`: same 6-target matrix, downloads daemon/rootshell/wpa artifacts first.
- `build_installer_gui_linux` (and other GUI builds).

All jobs use `rustup toolchain install` against the pinned toolchain (#1127) and
`Swatinem/rust-cache@v2`.

`release.yml` (tag push): `check_version_same` (all Cargo.toml versions equal; see
`scripts/set-versions.sh`), calls `main.yml`, then `release` downloads artifacts and publishes
per-platform `rayhunter-vX.Y.Z-<platform>.zip` plus `.sha256` (platforms `linux-x64`,
`linux-aarch64`, `linux-armv7`, `macos-intel`, `macos-arm`, `windows-x86_64`). Release on tag is
automatic since v0.12.0 (#1066).

---

## 11. Release history (from `gh release list` / `gh release view`)

| Tag | Date (UTC) | Notable changes |
|---|---|---|
| v0.13.0 | 2026-09-16 | No-NAS analyzer + no-diag UI alert (#1132); home PLMN from SIM for `imsi_requested` (#1101); clock-drift off/autosync/prompt (#1108); ndjson in ZIP (#1100); installer-gui MVP (#1116, #1124); pinned Rust toolchain in CI (#1127); RRCConnectionRelease classifier fix (#1150); TP-Link M7200 docs, M7350 v10 warning. |
| v0.12.0 | 2026-08-03 | Compressed QMDL (#970); gsmtap/diag refactor (#1077); serving/neighbor cell measurements and MAC RACH timing advance (#1074); roaming false-positive fix (#1014); `rayhunter-check` JSON output (#1088); EARFCN overflow fix (#1069); release workflow on tags (#1066). |
| v0.11.2 / v0.11.1 | 2026-05-28 / 05-12 | Patch releases. |
| v0.11.0 | 2026-05-12 | WiFi client mode (Orbic/Moxee first); WebDAV upload; orbic-network connection pooling disabled (#902); network stability for Moxee/Orbic/Wingtech/TMobile installers (#949); notification timeout (#979); config form modal. |
| v0.10.0 – v0.10.2 | 2026-02-04 .. 02-24 | Priority 2G downgrade false-positive fix (EU); High Visibility display mode; no-SIM warning; clock fix prompt; notification Test button; installer no longer overwrites config by default (#780); fix daemon continuing when DIAG init fails (#829). |
| v0.9.0 | 2026-01-06 | Installer as library (#691); `util orbic-shell` (#707); parser fuzzing fixes (#724); deku 0.20. |
| v0.8.0 | 2025-11-04 | Network installer becomes default `orbic`; old one renamed `orbic-usb` (#675); TP-Link installer fixes. |
| v0.7.0 / v0.7.1 | 2025-09-24 / 10-08 | Low-battery ntfy notifications (#603); battery level in UI (#566); log view in UI (#575); warnings-only filter; incomplete SIB downgraded to informational (#619). |
| v0.6.0 / v0.6.1 | 2025-08-21 / 08-25 | `orbic-network` installer added (#531, #548); ntfy notifications (#429); reanalyze button (#508); test heuristic; UZ801 support; severity in UI/display (#513). |
| v0.5.0 / v0.5.1 | 2025-07-25 / 08-04 | On-device NAS parser (#422); IMSI Requested v2, Incomplete SIB, NAS Null Cipher; heuristic versions; PinePhone, TMOHS1; unified daemon binary for all devices (#460). |
| v0.4.0 | 2025-06-24 | RRC parser fixed (null cipher reliable); config editing in web UI; ZIP download; Wingtech CT2MHS01; analyzers toggleable in config (#382). |
| v0.3.0 – v0.3.4 | 2025-05-16 .. 06-06 | Rust installer replaces shell script (#291); TP-Link M7350; new web UI; mdBook docs. |
| v0.2.x | 2024-11 .. 2025-04 | Shell-script (`install.sh`, ADB) era. |

---

## 12. Implications for Orbic-Rayhunter (do not duplicate)

Already upstream: ntfy-style push notifications (warning, low battery, update, disk space), WiFi
client mode for internet backhaul, WebDAV upload, GPS tagging (fixed or API push), five display
modes plus colorblind mode, double-tap button restart, clock-drift sync, compressed QMDL, offline
re-analysis (`rayhunter-check`, UI reanalyze), nine analyzers, update checker, Swagger API docs.

Gaps observed in v0.13.0 that an extension could target (verify against upstream issues first):
no HTTP authentication; no ntfy headers (title/priority/tags) or alternative channels (e.g.
Matrix, Signal, SMS, webhook JSON); no daemon respawn/watchdog beyond WiFi; coarse Orbic battery
reading; hardcoded framebuffer geometry; template/code disagreement on `auto_check_updates`;
WebDAV configurable only by file; no automated uninstall.

---

## Sources

- `upstream/Cargo.toml`, `upstream/rust-toolchain.toml`, `upstream/.cargo/config.toml`
- `upstream/dist/config.toml.in`, `upstream/dist/scripts/rayhunter_daemon`, `upstream/dist/scripts/misc-daemon`
- `upstream/daemon/src/main.rs`, `config.rs`, `diag.rs`, `notifications.rs`, `update.rs`, `qmdl_store.rs`, `pcap.rs`, `gps.rs`, `stats.rs`, `key_input.rs`, `battery/mod.rs`, `battery/orbic.rs`, `display/mod.rs`, `display/orbic.rs`, `daemon/Cargo.toml`
- `upstream/lib/src/lib.rs`, `upstream/lib/src/diag_device.rs`, `upstream/lib/src/analysis/*.rs`
- `upstream/check/src/main.rs`, `upstream/rootshell/src/main.rs`, `upstream/telcom-parser/Cargo.toml`
- `upstream/installer/build.rs`, `upstream/installer/src/lib.rs`, `upstream/installer/src/connection.rs`
- `upstream/doc/configuration.md`, `heuristics.md`, `api-docs.md`, `installing-from-source.md`, `faq.md`, `using-rayhunter.md`
- `upstream/.github/workflows/main.yml`, `upstream/.github/workflows/release.yml`, `upstream/make.sh`
- `gh release list -R EFForg/rayhunter -L 30`; `gh release view <tag> -R EFForg/rayhunter` for v0.3.0, v0.4.0, v0.5.0, v0.6.0, v0.7.0, v0.8.0, v0.9.0, v0.10.0, v0.11.0, v0.12.0, v0.13.0
- <https://github.com/EFForg/rayhunter/releases>
- <https://efforg.github.io/rayhunter/>
