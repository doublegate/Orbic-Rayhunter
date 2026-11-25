# Configuration

Our device configuration is [device/config/config.orbic.toml](../device/config/config.orbic.toml),
installed as `/data/rayhunter/config.toml` by `orx install` ([INSTALL.md](INSTALL.md) step 4); override it with `orx install --config <path>`.
Every key exists in upstream v0.13.0; this project adds no keys until a patch introduces one, and
any such key is documented here in the same change.

Sources of upstream values: `daemon/src/config.rs` (`Config::default()`, applied through
`#[serde(default)]`, so a key missing from the file takes the code default) and
`dist/config.toml.in` (the template the upstream installer writes)
([ref-docs/rayhunter-upstream-overview.md](../ref-docs/rayhunter-upstream-overview.md) section 4).
The upstream installer does not overwrite an existing `config.toml` unless `--reset-config` is
passed; `orx install` replaces it deliberately with our hardened config (the verified backup is the rollback, not a `.prev` copy).

Columns: **code default** = value when the key is absent; **upstream template** = value written by
`installer orbic`; **ours** = value in `config.orbic.toml` ("absent" means the code default
applies).

## Top-level keys

| Key | Code default | Upstream template | Ours | Why |
|---|---|---|---|---|
| `qmdl_store_path` | `/data/rayhunter/qmdl` | same | same | resolves through the `/data/rayhunter` symlink into `/data/rayhunter-data` |
| `port` | `8080` | `8080` | `8080` | unchanged; binds `0.0.0.0` with no authentication (bind restriction is T-OR-082) |
| `debug_mode` | `false` | `false` | `false` | `true` skips DIAG, display, keys, battery and Wi-Fi workers (PC runs only) |
| `device` | `orbic` | `orbic` (installer rewrites the commented line) | `orbic` | explicit, so the file does not depend on installer rewriting |
| `ui_level` | `1` | `1` | `1` | subtle status line: visible confirmation without drawing attention. Values: 0 invisible, 1 subtle line, 2 orca GIF, 3 EFF logo, 4 full-screen status |
| `colorblind_mode` | `false` | `false` | `false` | upstream default (green becomes blue when `true`) |
| `key_input_mode` | `0` | `0` | **`1`** | double-tapping the power button starts a new recording; useful in the field without a browser |
| `ntfy_url` | none | commented | commented | must be a private, unguessable topic; the URL is the only secret and the device needs internet (active SIM data or Wi-Fi client mode) |
| `enabled_notifications` | `["Warning","LowBattery"]` | same | same | `Update` omitted because update checks are off |
| `auto_check_updates` | **`true`** | `false` | `false` | set explicitly: the code default and the template disagree, so an older or partial config would silently enable GitHub polling. Updates go through our release process |
| `clock_sync_mode` | `2` | `2` | `2` | prompt on drift; offsets are in-memory only. 0 off, 1 autosync, 2 prompt |
| `min_space_to_start_recording_mb` | `1` | `1` | **`64`** | headroom so `/data` never fills completely; full `/data` wedged the daemon and the hotspot in #1192. Provisional until T-OR-068 measures growth and free space |
| `min_space_to_continue_recording_mb` | `1` | `1` | **`32`** | same reason; recording stops below this. Provisional (T-OR-068) |
| `gps_mode` | `0` | absent | absent | GPS fixes are precise location history ([ref-docs/legal-and-ethics.md](../ref-docs/legal-and-ethics.md) section 5.1); opt in deliberately. 0 disabled, 1 fixed, 2 API push |
| `gps_fixed_latitude` | none | absent | absent | only with `gps_mode = 1` (-90..90) |
| `gps_fixed_longitude` | none | absent | absent | only with `gps_mode = 1` (-180..180) |
| `wifi_enabled` | `false` | `false` | `false` | client mode made the AP and web UI unreachable on V1.2.0 in #1033; WPA3-SAE unsupported (#1177) |
| `wifi_ssid` | none | absent | absent | stored in `wpa_sta.conf`, managed by the web UI |
| `wifi_security` | none | absent | absent | `wpa_psk` or `sae`; stored in `wpa_sta.conf` |
| `wifi_password` | none | absent | absent | stored in `wpa_sta.conf`; never returned by the API |
| `dns_servers` | none (effective Quad9 `9.9.9.9`, `149.112.112.112`) | commented | absent | only used in Wi-Fi client mode |

## `[webdav]`

Config-file only (no web UI). The upload worker runs only when `url` is non-empty.

| Key | Code default | Upstream template | Ours | Why |
|---|---|---|---|---|
| `url` | `""` | commented | absent | off: uploads need backhaul and send captures (IMSI, location) to a third-party server; host-side pulls are preferred (T-OR-038, T-OR-089) |
| `username` | none | commented | absent | HTTP basic auth; a password without a username is rejected |
| `password` | none | commented | absent | stored in clear text in `config.toml`, which the API can read and write ([SECURITY-MODEL.md](SECURITY-MODEL.md), T-OR-087) |
| `upload_timeout_secs` | `300` | commented | absent | |
| `poll_interval_secs` | `3600` | commented | absent | |
| `min_age_secs` | `86400` | commented | absent | |
| `delete_on_upload` | `false` | commented | absent | |

## `[analyzers]`

Identical to upstream in v1.0.0. Descriptions and planned additions:
[ANALYZERS.md](ANALYZERS.md).

| Key | Code default | Upstream template | Ours | Why |
|---|---|---|---|---|
| `imsi_requested` | `true` | `true` | `true` | causal identity-exposure heuristic |
| `connection_redirect_2g_downgrade` | `true` | `true` | `true` | |
| `lte_sib6_and_7_downgrade` | `true` | `true` | `true` | |
| `null_cipher` | `true` | `true` | `true` | |
| `nas_null_cipher` | `true` | `true` | `true` | |
| `incomplete_sib` | `true` | `true` | `true` | informational only |
| `test_analyzer` | `false` | `false` | `false` | alerts on every SIB1; enable only for the smoke test in [TESTING.md](TESTING.md) |
| `diagnostic_analyzer` | `true` | `true` | `true` | informational only |
| `no_nas_messages` | `false` | `false` | `false` | upstream default; candidate to enable once our SIM state is stable, since it detects a missing or inactive SIM |

## Changing configuration

- Edit `device/config/config.orbic.toml` and this file in the same change; mark any deviation from
  upstream with an `ORX:` comment in the TOML.
- On the device, the web UI's config form writes `config.toml` through `POST /api/config` and
  restarts the daemon logic in-process. Anyone on the hotspot LAN can do the same, because the API
  is unauthenticated ([SECURITY-MODEL.md](SECURITY-MODEL.md)).
- New keys introduced by `[UPSTREAM]` patches (for example notification sinks in v1.2, T-OR-045..
  T-OR-050, or retention in v1.3, T-OR-065) must default to the current behavior when absent.

## Alerting (ntfy)

The daemon has built-in alerting: when `ntfy_url` is set it POSTs a notification on analyzer
detections (`Warning`) and low battery (`LowBattery`), with per-type debounce and
exponential-backoff retry. To enable it, uncomment `ntfy_url` in `config.orbic.toml` before install,
or set it in the Rayhunter web UI on a running device.

From the host:

```sh
orx alert status [--ip <addr>]                      # show the configured ntfy_url + enabled types (v1.1.0)
orx alert test   [--ip <addr>]                      # ask the daemon to send a test notification (v1.1.0)
orx alert set --ntfy-url <url> [--apply] [--backup-dir <dir>]  # write the ntfy_url (v1.1.1, T-OR-107)
orx alert set --clear [--apply] [--backup-dir <dir>]          # remove the ntfy_url (disable delivery)
```

`orx alert test` is outward-facing (the device POSTs to your `ntfy_url`); it reports clearly when no
URL is set.

`orx alert set` (v1.1.1, T-OR-107) writes `ntfy_url` without the web UI. It is a dry-run preview
unless `--apply`, because the write restarts the daemon (briefly interrupting the active recording).
It does not reconstruct the config: `POST /api/config` takes the full config and `GET /api/config`
redacts `wifi_password` to `null`, so `set` round-trips the exact object the GET returned and
replaces only the `ntfy_url` key -- byte-for-byte what the web UI's config form POSTs when you change
the notification URL. The daemon strips the Wi-Fi fields before writing `config.toml` (they live in
`wpa_sta.conf`), so the write cannot clobber the Wi-Fi password; `set` still warns when Wi-Fi client
mode is active, since the restart re-applies `wpa_sta.conf`. The URL is validated as an http(s) URL
with a host before anything is sent.

Backup gate: like `orx purge` (whose verified local pull copy is the undo), `set --apply` is gated
on an operation-appropriate restore path rather than the firmware-grade full-device backup that
`install`/`uninstall` require -- a config write cannot brick the device and is reversible. Before it
POSTs, `set` writes the exact prior `/api/config` body to `--backup-dir` (default the current
directory) as `orx-config-before-<addr>-<epoch>.json`, verifies it reads back byte-identical and
reports its SHA-256; if that fails it aborts without touching the device. Re-POSTing that snapshot
restores the config.

The daemon's additional sink types (webhook auth/headers, MQTT, Gotify) and a persistent retry queue
are `[UPSTREAM]` work scheduled for v1.2.0 (T-OR-045..050).
