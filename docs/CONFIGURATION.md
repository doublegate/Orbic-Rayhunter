# Configuration

Our device configuration is [device/config/config.orbic.toml](../device/config/config.orbic.toml),
installed as `/data/rayhunter/config.toml` by `orx install` ([INSTALL.md](INSTALL.md) step 4).
Every key exists in upstream v0.13.0; this project adds no keys until a patch introduces one, and
any such key is documented here in the same change.

Sources of upstream values: `daemon/src/config.rs` (`Config::default()`, applied through
`#[serde(default)]`, so a key missing from the file takes the code default) and
`dist/config.toml.in` (the template the upstream installer writes)
([ref-docs/rayhunter-upstream-overview.md](../ref-docs/rayhunter-upstream-overview.md) section 4).
The upstream installer does not overwrite an existing `config.toml` unless `--reset-config` is
passed; `orx install` replaces it deliberately and keeps `config.toml.prev`.

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
- New keys introduced by `[UPSTREAM]` patches (for example notification sinks in v1.1, T-OR-045..
  T-OR-050, or retention in v1.3, T-OR-065) must default to the current behavior when absent.
