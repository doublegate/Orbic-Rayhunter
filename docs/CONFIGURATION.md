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
| `ntfy_headers` | `false` | commented | absent | v1.2.0 (patch 0001): `true` adds ntfy `Title`/`Priority`/`Tags` from the notification type; `false` keeps a plain POST (for a generic webhook receiver) |
| `ntfy_auth` | none | commented | absent | v1.2.0 (patch 0002): `Authorization` header value (`Bearer ...`/`Basic ...`) for a protected ntfy endpoint. Secret: redacted from `GET /api/config`, dropped if `ntfy_url` changes |
| `webhook_url` | none | commented | absent | v1.2.0 (patch 0003): generic webhook; receives JSON `{source,type,message,timestamp}` (no identifiers) in addition to ntfy. Secret (may embed a token): redacted from `GET /api/config` |
| `gotify_url` | none | commented | absent | v1.2.0 (patch 0004): Gotify server base URL; the daemon POSTs to `<gotify_url>/message` |
| `gotify_token` | none | commented | absent | v1.2.0 (patch 0004): Gotify application token (`X-Gotify-Key`). Secret: redacted from `GET /api/config`, dropped if `gotify_url` changes |
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
| `nas_reject_identity` | `false` | `false` | `false` | v1.2.0 (patch 0007, T-OR-052): flags an Attach/TAU/Service Reject with an identity-forcing EMM cause (#9 Medium / #10 Low) -- the Devilray NAS-Reject IMSI-catching blind spot. Keys on the cause alone (does not confirm the reject was unprotected or that an IMSI re-attach followed). Off until T-OR-061 calibrates false positives: #9/#10 also occur benignly (MME restart, GUTI/MME-pool mismatch, idle-timer implicit detach) |
| `auth_failure` | `false` | `false` | `false` | v1.2.0 (patch 0008, T-OR-053): flags a NAS Authentication Failure (#20 MAC failure High / #21 synch Medium / #26 Medium) or an Authentication Reject (High) -- a false base station that cannot produce a valid AUTN. Off until T-OR-061 calibrates: #21 fires on a genuine SQN resync or SIM swap |

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
directory, written `0600`; snapshots are gitignored) as `orx-config-before-<addr>-<epoch>.json`,
verifies it reads back byte-identical and reports its SHA-256; if that fails it aborts without
touching the device.

Restore limitation: the snapshot is the body `GET /api/config` returns, which **redacts the secret
keys** (`ntfy_auth`, `gotify_token`, `webhook_url`; see below). Re-POSTing it restores everything
except those secrets, and changing `ntfy_url`/`gotify_url` additionally drops the matching stored
credential. So if you have notification secrets configured, re-supply them after a restore, or keep a
copy of the device's `config.toml` as the full backup.

### Notification sinks (v1.2.0, daemon patches)

v1.2.0 extends the daemon's single-ntfy notifier into several independent sinks (`[UPSTREAM]`
`patches/0001-0004`). Each is opt-in via the config keys above and off by default, so an absent key
keeps the historical behavior:

- **ntfy header enrichment** (`ntfy_headers`): titled/prioritized/tagged ntfy notifications.
- **ntfy endpoint auth** (`ntfy_auth`): Bearer/Basic `Authorization` header for a protected topic.
- **Generic webhook** (`webhook_url`): a JSON `{source, type, message, timestamp}` POST, no identifiers.
- **Gotify** (`gotify_url` + `gotify_token`): Gotify `{title, message, priority}` to `<url>/message`.

Secret keys (`ntfy_auth`, `gotify_token`, `webhook_url`) are redacted from `GET /api/config` and
preserved across a config round-trip that omits them; a credential is dropped if its endpoint URL
changes, so the unauthenticated config API cannot repoint a URL and harvest the stored secret.
An **empty or whitespace-only** value for any of these secret keys **clears** the stored secret
(it is treated as "unset", never persisted as inert whitespace).

A sink is **disabled** (skipped by both the daemon's notification worker and
`POST /api/test-notification`) when its URL is absent, empty, or whitespace-only. For `gotify_url`
this also covers a value that normalizes to empty after stripping the query/fragment and trailing
slash (for example `"/"` or `"?token=x"`), so such a value does not produce a POST to a bare
`/message`.

Hardening (patch 0005) changes delivery behavior, so note:

- **No HTTP redirects.** The notification client does not follow 3xx redirects (so a custom
  credential header such as `X-Gotify-Key` cannot be forwarded to a redirect target). A sink URL
  that relies on an `http:` -> `https:` upgrade or a proxy 30x will fail rather than be followed --
  configure the final `https` URL directly.
- **`gotify_url` is a base URL.** Any query/fragment is dropped before `/message` is appended, and
  `GET /api/config` returns it without a query, so a token must go in `gotify_token` (redacted), not
  the URL.
- **`POST /api/test-notification`** sends a test to every configured sink and reports per-sink
  results (it no longer tests only ntfy). The test uses the `Warning` notification type (webhook
  `type: "Warning"`, Gotify priority 8, ntfy high priority), so a receiver whose rules key on the
  `Warning` type/priority will treat a connectivity test like a live alert -- distinguish it by the
  message body (`"Test notification from Rayhunter"`).

MQTT (T-OR-048) and a persistent retry queue (T-OR-050) are deferred past v1.2.0.
