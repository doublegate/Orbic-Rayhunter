## [Unreleased]

### Added (toward v1.2.0 — daemon notification sinks, `[UPSTREAM]` patches)

- **ntfy header enrichment** (T-OR-045, patch 0001): opt-in `ntfy_headers` sends ntfy
  `Title`/`Priority`/`Tags` derived from the notification type; default-off keeps today's plain POST.
- **ntfy endpoint auth** (T-OR-046, patch 0002): optional `ntfy_auth` Authorization header
  (Bearer/Basic) for a protected topic. Secret: redacted from `GET /api/config`, preserved across a
  config round-trip that omits it, and dropped if `ntfy_url` changes (so the unauthenticated config
  API cannot repoint the URL and harvest the token).
- **Generic webhook sink** (T-OR-047, patch 0003): optional `webhook_url` receives a JSON body
  `{source, type, message, timestamp}` with no subscriber identifiers, independently of ntfy. The
  notification worker now delivers to multiple sinks, each debounced/retried independently.
  `webhook_url` is treated as a secret (it may embed a token): redacted + preserved. Transport
  errors are URL-stripped before logging so a private ntfy topic cannot leak into the daemon log.
- **Gotify sink** (T-OR-049, patch 0004): optional `gotify_url`/`gotify_token` POST Gotify JSON
  `{title, message, priority}` to `<gotify_url>/message` with an `X-Gotify-Key` header. `gotify_token`
  redacted + preserved like `ntfy_auth`.

### Security / hardening (patch 0005, PR #16 review follow-up)

- Notification HTTP client **disables redirects** (`reqwest::redirect::Policy::none()`): reqwest
  strips `Authorization` on a cross-origin 30x but not a custom `X-Gotify-Key`, so a redirecting
  proxy/captive portal could otherwise harvest the Gotify token. Credential header values are marked
  sensitive.
- `gotify_message_url` **drops any query/fragment** so a `?token=` cannot leak through the unredacted
  `gotify_url` in `GET /api/config` (the token belongs in the redacted `gotify_token`).
- Empty-string secrets no longer send a blank header; a header value with invalid bytes fails closed
  (the request is not sent unauthenticated); webhook payload `timestamp` is the detection time, not
  the retry time; sinks are **delivered concurrently** per tick so a hung sink cannot block the
  others; `POST /api/test-notification` now tests every configured sink and reports per-sink results.
- Review follow-up refinements: a single `redirect_safe_client()` the worker and `test_notification`
  both use (the no-redirect policy lives in one place); whitespace-only `webhook_url` cleared like the
  other secrets; a slash-/query-only `gotify_url` that normalizes to empty is skipped; the
  `gotify_url` normalize-and-resolve logic is one tested `resolve_gotify` helper.
- **`orx-cli` hardening** (`orx alert set`): `validate_ntfy_url` rejects embedded `user:pass@`
  userinfo (which leaked when the URL was printed) and checks the authority shape (non-empty host, a
  bracketed IPv6 literal, an optional numeric non-zero port, and `\` as a path delimiter -- it is a
  shape check, not full WHATWG host validation);
  the config snapshot (`pull::write_atomic`) now writes via an O_EXCL, mode-`0600` temp with a unique
  name and removes the partial temp on error; `.gitignore` ignores `orx-config-before-*.json`.

These are `patches/0001-0005` on the pinned upstream v0.13.0 daemon, validated on upstream's own gates
(`cargo fmt`, `clippy -D warnings`, 120 tests from `cargo test -p rayhunter-daemon`, `daemon/web`
check + tests). MQTT (T-OR-048) and a
persistent retry queue (T-OR-050) are deferred. On-device validation is batched with the analyzers phase.

### Added (toward v1.2.0 — analyzer phase)

- **Analyzer test harness** (T-OR-051, `[UPSTREAM]` patch 0006): shared `#[cfg(test)]` helpers in
  `lib/src/analysis/test_util` — `packet_time` (deterministic timestamps), `nas_plain` (build a plain
  LTE NAS `InformationElement` from a raw payload), and `collect_events` (run an analyzer over a
  sequence and gather its `Event`s for a golden comparison) — the foundation the detectors
  (T-OR-052+) build on. The per-detector named vector builders grow with each analyzer, and the
  capture scrubber (sanitizing a real QMDL for committed vectors) is deferred until on-device captures
  exist (T-OR-061). Validated on `cargo test -p rayhunter` (lib).
- **NAS Reject (identity-forcing) detector** (T-OR-052, `[UPSTREAM]` patch 0007, config key
  `nas_reject_identity`): flags an Attach/TAU/Service Reject carrying an identity-forcing EMM
  cause (#9 "UE identity cannot be derived" -> Medium, #10 "implicitly detached" -> Low) -- the
  NAS-Reject IMSI-catching path (the Devilray "identity reveal" blind spot that the Identity-Request
  analyzer misses). It keys on the cause alone and does not (yet) confirm the reject was
  integrity-unprotected or that an IMSI re-attach followed -- the NAS security-header type is not
  preserved through the decrypted-NAS log, and the Attach Request's EPS mobile identity is not decoded
  by the current parser. **Off by default**: #9/#10 also occur benignly (MME restart, GUTI/MME-pool
  mismatch, idle-timer implicit detach), so it needs T-OR-061 on-device false-positive calibration.
- **Authentication failure / reject detector** (T-OR-053, `[UPSTREAM]` patch 0008, config key
  `auth_failure`): flags a NAS Authentication Failure (EMM cause #20 MAC failure -> High, #21 synch
  failure -> Low, #26 -> Medium) or an Authentication Reject (-> High). A false base station with
  no access to the subscriber's home keys cannot produce a valid AUTN, so the USIM rejects the
  challenge with an Authentication Failure (#20 is the direct signature, near-impossible benignly on
  a home network); an Authentication Reject is sent by a network that cannot validate the UE or
  injected by a rogue base station to invalidate the USIM. Stateless. **Off by default**: #21 also
  fires on a genuine SQN resync or a
  SIM swap, and an inactive/unprovisioned SIM draws a legitimate Authentication Reject on every
  attach/auth retry (stateful suppression and dedup are tracked as T-OR-109), so it needs T-OR-061
  calibration.
- **Null integrity (EIA0) detector** (T-OR-055, `[UPSTREAM]` patch 0009, config key `null_integrity`):
  flags a NAS Security Mode Command that selects the null integrity algorithm EIA0 (-> High). EIA0
  leaves NAS signalling unauthenticated, so the network cannot be verified and signalling can be
  forged -- a stronger false-base-station indicator than the null ciphering (EEA0) the existing
  `nas_null_cipher` analyzer covers. Stateless. **Off by default**: EIA0 is legitimate for an
  unauthenticated emergency-services attach (accepted as a known false positive until T-OR-111 adds
  emergency-attach suppression), so it needs T-OR-061 calibration. (NAS half only; the RRC Security
  Mode Command and no-SMC-before-DRB parts are deferred to T-OR-111 pending RRC test vectors.)

## [1.1.1] - 2026-11-25

### Added

- `orx alert set --ntfy-url <url>|--clear [--apply] [--ip <addr>]` (T-OR-107): write or clear the
  daemon's `ntfy_url` from the host, so alerting can be turned on without the Rayhunter web UI.
  It is a dry-run preview unless `--apply` (the write restarts the daemon, briefly interrupting the
  active recording). The URL is validated as an http(s) URL with a host before anything is sent.
- `--backup-dir <dir>` for `orx alert set` (default the current directory).
- `Json::as_bool` on the std-only JSON helper.

### How it stays safe

- Device-write backup gate (per AGENTS.md priority 1): before `--apply` POSTs, `set` snapshots the
  exact prior `/api/config` body to `--backup-dir` as `orx-config-before-<addr>-<epoch>.json`,
  verifies it reads back byte-identical and records its SHA-256, aborting without touching the device
  if that fails. Like
  `orx purge`'s verified pull copy, this local snapshot is the operation-appropriate restore path
  (re-POST it to revert) -- a config write is reversible and cannot brick the device, so it does not
  need the firmware-grade full-device backup that `install`/`uninstall` require.

- `POST /api/config` takes the FULL config and `GET /api/config` redacts `wifi_password` to `null`,
  so `set` does not reconstruct a config: it round-trips the exact object the GET returned and
  replaces only the `ntfy_url` key. This is byte-for-byte what the upstream web UI's config form
  POSTs when you change the notification URL (it, too, sends `wifi_password: null`). The daemon's
  `set_config` strips the Wi-Fi fields before writing `config.toml` (they live in `wpa_sta.conf`),
  so the write cannot clobber a password that `config.toml` never held. `set` refuses to write back
  a body that does not look like Rayhunter's config, and warns when Wi-Fi client mode is active.
- Validated live on the unit (ORB400L_V1.2.0_BVZRT): apply pointed `ntfy_url` at a loopback
  self-test URL, a full before/after config diff showed **only** `ntfy_url` changed (every other
  key, including the nested `webdav`/`analyzers` objects and the null `wifi_password`, preserved
  byte-for-byte), and `--clear --apply` returned the config identical to the baseline.

### Fixed

- The v1.1.0 notes referred to the host-side config write as "T-OR-046"; that ID is the upstream
  notification-auth sink in [ROADMAP.md](ROADMAP.md). The `orx alert set` ticket is **T-OR-107**.

## [1.1.0] - 2026-11-25

### Added

- `orx alert status|test [--ip <addr>]` (T-OR-044): host-side enablement of the daemon's BUILT-IN
  ntfy alerting. Upstream v0.13.0 already sends a `Warning` notification on analyzer detections (and
  `LowBattery`) to its configured `ntfy_url`, with per-type debounce and exponential-backoff retry.
  `orx alert status` reads `/api/config` and shows the current `ntfy_url` and enabled notification
  types (device strings sanitized before printing); `orx alert test` asks the daemon to POST a test
  notification (reporting clearly when no `ntfy_url` is set). Validated live: the unit reports no
  `ntfy_url` yet with `Warning, LowBattery` enabled -- alerting is one URL away.
- A std-only JSON serializer (`Json::to_json`/`set`) and `pull::http_post_json`, groundwork for the
  host-side config write (`orx alert set`, T-OR-046/v1.1.1).

### Notes

- Setting `ntfy_url` from the host restarts the daemon and the `/api/config` round-trip redacts
  `wifi_password`, so `orx alert set` is deferred to v1.1.1 (validated as a careful read-modify-write
  on-device). For now set `ntfy_url` in the Rayhunter web UI or in `config.orbic.toml` at install.
- The daemon's alerting SINK extensions (webhook headers/auth, MQTT, Gotify, persistent queue;
  T-OR-045..050, all upstream) move to v1.2.0 per the enable-now/patch-after decision.

## [1.0.6] - 2026-11-25

### Added

- `orx rescue [--ip <addr>]` (T-OR-041): a READ-ONLY diagnostic over the `:24` root shell for when
  the daemon's `:8080` API is unresponsive (e.g. `/data` filled up). It reports free space on
  `/data` and the stored recordings (oldest first, with sizes), reading `qmdl_store_path` from the
  device config. It fails closed if `/data` is not mounted or the store is missing, lists the store
  directory directly (no shell glob, so an unbounded file count cannot exceed `ARG_MAX`), accepts
  only numeric `<timestamp>.qmdl.gz` names, and rejects duplicate names. Device output is parsed
  defensively. The `:24` shell is not reboot-persistent (re-open it first; see INSTALL.md section 5).
  Validated live on the unit.
- Confirmed on the unit that `qmdl_size_bytes` in the daemon manifest is the COMPRESSED on-disk size
  (recordings are stored as `<name>.qmdl.gz`; `1795442375.qmdl.gz` is exactly 869535 bytes).

### Notes

- Deleting recordings from `orx rescue` is deliberately deferred (T-OR-041b): doing it safely means
  stopping the daemon, removing files AND reconciling the daemon's `manifest.toml`, restarting, and
  verifying recovery -- a destructive path that must be validated on a genuinely wedged device. For
  now `orx rescue` reports; the manual free-space-and-recover steps are in INSTALL.md section 9.

## [1.0.5] - 2026-11-25

### Added

- `orx purge [--apply --pulled <dir>|--force] [--ip <addr>]` (T-OR-040): deletes COMPLETED recordings from the device via
  `POST /api/delete-recording/{name}`, and NEVER the active recording. Dry-run by default -- it lists
  the recordings that would be deleted, notes the active one is kept, and recommends running
  `orx pull-logs` first since captures hold real identifiers; `--apply` requires a safety gate: `--pulled <dir>` deletes only recordings whose `<name>.qmdl` is
  present in `<dir>`, matches its `PULL-MANIFEST.txt` SHA-256, AND whose current manifest size equals
  the size `orx pull-logs` recorded for the completed recording (so a stale/partial pull cannot
  authorize deleting a larger completed recording), while
  `--force` is an explicit unguarded override; `--apply` alone is refused. The active recording is
  always excluded (even if it appears in `entries`). Fails closed on a missing `entries` array or a
  malformed `current_entry`, and reports a `403` (daemon debug mode) clearly.
  Adds a std-only `http_post` helper. The dry run was validated live; the delete path is unit-tested
  against a loopback server (the device's real captures were not deleted).

## [1.0.4] - 2026-11-25

### Added

- `orx check [--ip <addr>]` (T-OR-037): read-only device health report. Queries the daemon's
  `:8080` API (`/api/system-stats`, `/api/qmdl-manifest`) for the rayhunter version, OS/arch,
  disk/memory/battery and recording status, and probes ports -- the daemon port (`:8080`) should be
  open, while the root shell (`:24`) and installer transfer port (`:8081`) should be closed on a
  settled device. It **fails closed**: an unreachable/unparsable API, a daemon with no (or empty)
  rayhunter version, a manifest with no `entries` array, and an exposed root shell or installer
  port are all failures. Port posture uses a precise probe: a security port counts as closed only
  when the connection is actively refused -- a timeout/filtered/unreachable result is indeterminate
  and fails closed rather than reporting a reassuring "closed". Untrusted device strings are sanitized of control characters before
  printing (no ANSI/forged-line injection from a compromised device).
  Prints an `[ok]/[warn]/[fail]` summary and exits non-zero on any failure. Reuses
  the std-only JSON reader and HTTP client; no new dependencies. Validated live (device healthy).

## [1.0.3] - 2026-11-25

### Added

- `orx pull-logs --to <dir>` (T-OR-038): downloads each recording's QMDL
  (`/api/qmdl/{name}`) and analysis report (`/api/analysis-report/{name}`, NDJSON, best-effort)
  from the device's `:8080` API to a host directory, writes a `PULL-MANIFEST.txt` of SHA-256 sums,
  and skips completed recordings already pulled unchanged (tracked in a `.orx-pull-state` file).
  Streams each download to disk (bounded) so a large capture never buffers in memory, and validates
  device-supplied recording names before using them in a path or URL. The manifest parse fails
  closed (an `entries` array is required), downloads use HTTP/1.1, decode chunked (requiring the terminating zero-chunk) and verify Content-Length, file hashing is incremental, and downloads carry a typed error so only a
  genuine 404 treats the analysis report as absent (any transport/5xx/truncation error fails closed),
  only fully-pulled completed recordings are recorded as skippable, PULL-MANIFEST.txt reflects every file held locally (so a purged recording's
  entry is kept) and both it and .orx-pull-state are written atomically. Validated live on the unit (4 recordings pulled, then 3 skipped on re-run).
- A minimal std-only JSON reader (`json`) for the daemon's API responses, keeping orx-cli
  dependency-free.

### Notes

- Completion is read from the manifest's `entries` vs `current_entry` position, not `stop_reason`:
  this daemon (v0.13.0) leaves `stop_reason` null on finished entries too.

## [1.0.2] - 2026-11-25

More `orx install` hardening: deploy our hardened config, and a free-space preflight.

### Added

- `orx install` now deploys our hardened `device/config/config.orbic.toml` by default (T-OR-036b)
  instead of upstream's bare template -- so the device gets `key_input_mode = 1` (double-tap to
  record), `min_space_to_start/continue = 64/32 MiB` of `/data` headroom, `wifi_enabled = false`,
  `auto_check_updates = false` and the full analyzer set. Override with `--config <path>`;
  `device = "orbic"` is always enforced.
- Free-space preflight before any device write (T-OR-036c): on `--apply`, install runs `df` on
  `/data` and refuses if it lacks room for the daemon + config plus an 8 MiB slack, so a nearly-full
  `/data` cannot wedge the daemon and hotspot mid-write (#1192). The `df` row is validated against
  the mountpoint (fail closed on malformed output), tolerating a wrapped filesystem name. The
  few-KiB `/etc/init.d` writes are not preflighted (a full rootfs simply fails the atomic write),
  so `uninstall` is never blocked.

## [1.0.1] - 2026-11-25

Install-safety hardening and a test-flake fix. No device-facing behaviour change beyond the new
pre-install guards.

### Added

- `orx install` now guards the staged daemon before any device write (T-OR-036a): it verifies the
  daemon against a `SHA256SUMS` beside it -- refusing a corrupted or swapped artifact, and refusing
  a **missing** `SHA256SUMS` unless `--allow-unverified` -- and refuses an **unstripped** daemon
  (the ~16 MB devel build left where the ~6 MB firmware release was meant) unless
  `--allow-unstripped`. The checks run on the exact bytes that will be pushed.

### Fixed

- `probe::tcp_open_detects_listener_and_closed_port` was flaky under parallel test runs (a
  concurrent test could reuse the just-freed ephemeral port); it now retries with fresh ports and
  is deterministic.

## [1.0.0] - 2026-11-23

First stable release: a documented, reproducible, reversible install of our Rayhunter build,
**validated on the real Orbic RC400L** (`ORB400L_V1.2.0_BVZRT`). The core `orx` host tooling is
in place -- `doctor`, `backup`, `verify`, `install`, `uninstall` -- all zero-dependency and
unit-tested (64 orx-cli tests), with every device write gated on a verified rootfs backup.
(`orx pull-logs`/`check`/`purge` remain as Phase 4 follow-ups.)

### Added

- **On-device install of our build (T-OR-022 / T-OR-036).** `orx install --apply` deployed our
  reproducible firmware daemon (`scripts/build.sh firmware`, 6,109,892 B) to the unit, replacing
  upstream v0.13.0. Each file's device-side SHA-256 was verified against our build before the
  atomic `mv`; after reboot the hooked `misc-daemon` auto-started the daemon (reboot-persistent)
  and an active capture was confirmed via `/api/qmdl-manifest`.

### Fixed

- **File transfer to the device is now lossless (critical).** The busybox `nc -ll -e /bin/sh`
  control channel drops bytes when its pipe to the shell backs up, so the previous `cat > file`
  bulk write silently corrupted binaries (a 4 KiB payload arrived as ~3 KiB, independent of
  encoding; the 6 MB daemon failed its hash check). `TcpShell::put` now uses a one-shot device-side
  `nc -l` listener (the data path is socket -> file, no shell pipe), the mechanism upstream's
  installer uses -- verified byte-exact on the unit and by a loopback unit test. `send_file` still
  verifies the device-side hash afterward, so any residual race fails closed.

### Notes

- The unauthenticated root shell on TCP 24 used by `orx` to deploy is NOT reboot-persistent on this
  unit (it is the pre-Rayhunter listener, not part of the install), so it is absent after an install
  reboot; re-establish it before `orx uninstall`. Hardening it off entirely is Phase 6
  (T-OR-081/082).

## [0.8.0] - 2026-11-23

Adds `orx install` and `orx uninstall` -- the device-write operations that complete the orx tooling. Both are DRY-RUN by default (print the exact step plan) and `--apply` executes; both refuse unless `verify::gate()` passes for the backup directory, so a device write is impossible without a verified rootfs backup to roll back to. The deployed file set and paths mirror upstream's network installer and the install-footprint RE, but deploy OUR firmware build; uninstall restores the true stock misc-daemon extracted from the verified backup's `root.tar` and removes our files (leaving recordings). orx-cli stays zero-dependency; 58 unit tests, dry-run validated on the unit against the verified Sprint 1.3 backup.

### Added

- `orx install --dir <backup> [--dist <d>] [--daemon <f>] [--apply]` (T-OR-036): deploys the
  daemon, the rendered `config.toml` (device = orbic, robustly set in the root table), the
  `rayhunter_daemon` init, and the hooked `misc-daemon`, then reboots -- the upstream
  network-installer footprint with our build. Dry-run unless `--apply`; always backup-gated. Every
  file is written ATOMICALLY (send to
  `<path>.orx-new`, which the device re-hashes, then `chmod` + `mv`), so a dropped connection or
  failed chmod can never leave a half-written/non-executable init script, and `mv`-over-a-running
  binary avoids `ETXTBSY`. The daemon and init scripts are validated first (daemon = 32-bit LE ARM ELF;
  scripts = `#!` shebang, no CRLF/NUL, valid UTF-8; the init references the daemon path and the
  misc-daemon references `rayhunter_daemon`). A failed transfer cleans up its `.orx-new` temp.
- `orx uninstall --dir <backup> [--apply]` (T-OR-039): restores the TRUE stock `misc-daemon`
  extracted from the verified backup's `root.tar` (NOT copied from the device, which may already
  carry the hooked script), atomically, then removes our files (leaving `/data/rayhunter-data`
  recordings); dry-run unless `--apply`; backup-gated.

## [0.7.0] - 2026-11-23

Adds `orx verify` and the shared backup-gate, so a backup can be proven as a rollback before a device write. Also the first release routed through a pull request (CI, docs, security and the Antigravity reviewer all gate it), and it carries the agy-review fixes below. orx-cli stays zero-dependency.

### Added

- `orx verify --dir <dir>` (T-OR-035, Sprint 4.3): re-reads a backup directory's `MANIFEST.txt`
  and re-hashes each `<fs>.tar`, reporting ok / missing / too-small / size-mismatch / hash-mismatch
  per filesystem, then a gate line. The shared `verify::gate()` (the Rust counterpart of
  `orbic-ota-block.py`'s `_backup_gate`) requires rootfs `/` to be `ok`/`ok-live` AND its archive
  to verify, and is what the upcoming `orx install`/`uninstall` will call before any device write.
  Manifest parsing reads tokens by key, so both the Python and Rust manifest layouts verify. No
  device access; fully unit-tested (43 orx-cli tests). It fails closed throughout: `parse_manifest`
  errors on a malformed row (< 3 fields, a non-`key=value` token, an empty key, a non-`u64` `size=`,
  a non-64-hex `sha256=`, a duplicate `size=`/`sha256=`, or any control character);
  a row without a valid `size=` + 64-hex `sha256=` is rejected unchecked; a recorded state other
  than `ok`/`ok-live` fails the entry; a duplicate rootfs `/` row is ambiguous; an archive that is
  a symlink or non-regular file is rejected (checked before open and re-checked on the open handle,
  portably -- no arch-specific flag); and `orx verify` + `verify::gate()` share one gate policy over
  verdicts computed once (the ~87 MB rootfs is hashed once). Hardening from the PR's Codex +
  Antigravity review.

### Fixed

Address all eight findings from an adversarial `agy` (Antigravity) review of the v0.1.0-v0.6.0
changes that were pushed straight to `main` without a PR review:

- **Backup write-gate now verifies the archive, not just the manifest text** (`orbic-ota-block.py`
  `_backup_gate`): the `root.tar` beside the manifest must exist, clear the 1 KiB floor, and match
  the `size=`/`sha256=` the manifest records; tokens are parsed by key so both the Python and Rust
  manifest layouts are accepted. A stale, hand-edited, or deleted archive no longer passes.
- **`orx backup` emits a gate-compatible `ok-live`** (was `ok-live (tar exit N)`, whose trailing
  text made every live-rootfs manifest fail the gate's last-token check).
- **A failed `orx backup` no longer leaves a gate-passable `MANIFEST.txt`**: on a target failure it
  removes any stale `MANIFEST.txt` and writes `MANIFEST.incomplete.txt`, so a partial backup cannot
  unlock device writes.
- **`orbic-ota-block.py --apply` always persists the undo manifest**, even on a mid-loop failure
  (the device is never mutated without an undo record), refuses to overwrite an existing
  `.orx-disabled` saved binary, and `--undo` refuses to clobber a binary already present at the
  original path and warns on a restored-hash mismatch.
- **`backup::unescape` no longer corrupts non-ASCII mountpoints** (decodes bytes once via
  `from_utf8_lossy` instead of casting each UTF-8 byte to `char`; octal predicate is `0-7`).
- **`scripts/build.sh firmware` cleans up its multi-GB temp target dir** on exit (the container
  chowns it back to the host user so the trap can remove it).
- **`release.yml` matches the CHANGELOG header literally** (`index()`), not as a regex whose dots
  and `+` could match unintended versions.
- **`probe::json_str_field` matches a key, not a same-named value**, and refuses `\uXXXX` rather
  than emitting a bogus literal; **`transport` `read_until` errors on the 64 MiB cap** instead of
  returning a misleading "no marker".

All fixes carry unit tests (34 orx-cli tests + the Python gate self-test); every gate green.

## [0.6.0] - 2026-11-23

Adds `orx backup`, the first `orx` device operation and the write-gate every later command depends on. It ports the proven Python backup to native Rust on the Sprint 4.1 transport (streaming tar + inline SHA-256), and is validated end-to-end on the device. orx-cli stays zero-dependency; 33 unit tests.

### Added

- `orx backup` (T-OR-034, Sprint 4.2): archives every writable device filesystem over the root
  shell with `tar`, streamed to a `0600` `<fs>.tar` on the host while hashing inline, and writes
  a `MANIFEST.txt` in the exact format the write-gate reads. Each archive's state is `ok`, or
  `ok-live` (only with `--allow-live`, for a writable fs whose tar exits non-zero); a sub-1 KiB
  result is rejected so a failed tar cannot masquerade as a backup. The device-correct busybox
  `tar` invocation (`-C <mp> <--exclude> -cf - . 2>/dev/null`, no `--ignore-failed-read`) and the
  nested-mount excludes are unit-tested against the fake shell, and the command was validated
  end-to-end on the unit: root.tar 86.7 MB, data/usrdata/cache all `ok`, the inline SHA-256
  matching an independent `sha256sum`.
  Output files are written to an `O_EXCL` temp (fresh `0600`, no symlink follow) and renamed
  over the target, so a backup of device data (which can contain IMEI/IMSI) is never written
  through a planted symlink (flagged by the automated security review).

- `transport`: a streaming `Shell::run_to(command, sink)` that writes a command's output to a
  writer as it arrives (never buffering), for a `tar`-of-filesystem that would not fit in memory.
  Its core is a unit-tested `MarkerSplitter` that separates the payload from the trailing
  exit-status marker correctly across any chunk boundary (including a marker split mid-sequence),
  so the end marker can never corrupt a streamed backup. Foundation for `orx backup`/`pull-logs`.

## [0.5.0] - 2026-11-23

First `orx` tooling release (Sprint 4.1). `orx doctor` gains `--json`, a reported Rayhunter version and documented exit codes, and a tested, dependency-free device transport layer (shell + checksummed file transfer) lands as the foundation for the backup/install/pull-logs/uninstall commands in later Phase 4 sprints. orx-cli remains zero-dependency; 23 unit tests, all device I/O behind a trait with an in-memory fake.

### Added

- `orx-cli` `transport` module (T-OR-033, Sprint 4.1): a `Shell` trait (run a command with an
  exit-code marker, transfer a file) over the device's TCP-24 root shell, mirroring the proven
  Python tools' wire protocol. The verified transfer (`send_file`) accepts a file only when the
  device's own `sha256sum` matches the bytes sent, and rejects unsafe remote paths before
  touching the device. A `TcpShell` real implementation and an in-memory `FakeShell` let every
  caller be unit-tested with no hardware; the failure paths (hash mismatch, dropped write, unsafe
  path, read timeout) and the real `TcpShell` framing are covered (23 orx-cli tests).
- `orx-cli` `sha256` module: a std-only, streaming SHA-256 (no dependency added) for verifying
  device transfers against a device-side `sha256sum`, validated against the FIPS 180-4 known
  vectors (incl. the one-million-'a' case). Foundation for the Sprint 4.1 transport (T-OR-033).
- Hardening (from the automated security review of doctor v2): `probe::http_get` now enforces a
  single total deadline across all reads (a per-read timeout let a slow-drip peer stall
  indefinitely), and `orx doctor` validates the device-reported Rayhunter version against a
  strict `[0-9A-Za-z.+-]{1,64}` token before printing it (a compromised daemon or local MITM
  could otherwise inject terminal escape sequences).
- `orx doctor` v2 (T-OR-032, Sprint 4.1): `--json` machine-readable output, the running
  Rayhunter version read from `/api/system-stats` when `:8080` answers, and documented exit
  codes (0 found, 1 nothing found, 2 usage/host error). A minimal std-only HTTP/1.0 client and a
  dependency-free JSON string-field reader were added to `probe` (orx-cli stays zero-dependency),
  both unit-tested with a loopback fake server. `rayhunter` reachability now also counts toward a
  success exit.

## [0.4.0] - 2026-11-23

Makes our build produce the **production** daemon and records the reproducibility result. `release.yml` now ships the `firmware`-profile daemon (pq-tls, stripped, opt-level z, fat LTO -- ~6.1 MB), the device binary, instead of the `firmware-devel` build (~16 MB) that v0.1.0-v0.3.0 shipped. `scripts/build.sh firmware` builds it reproducibly in the same `messense/rust-musl-cross` container upstream uses, so it needs only docker/podman + npm, no host cross toolchain. Measured: our build reproduces the upstream release daemon to within 384 bytes (same profile/toolchain/lockfile/pq-tls); not bit-identical (embedded build paths differ).

### Changed

- `release.yml` builds and ships the production `firmware` daemon + rootshell (was
  `firmware-devel`). The release notes' build-provenance section now records the daemon profile
  and the reproducibility result.
- `scripts/build.sh firmware` now runs the cargo build inside the
  `messense/rust-musl-cross:armv7-musleabihf` container (no host `arm-linux-musleabihf` cross
  toolchain required); the web UI is still built on the host. The target dir is a scratch path
  outside the tree so the container's root-owned artifacts never land in `build/rayhunter`.

### Added

- `re/notes/reproducible-build.md` (T-OR-030): method, hashes and interpretation of the
  our-build-vs-upstream-release daemon comparison. `docs/BUILD.md` reproducibility section
  updated to match.

## [0.3.0] - 2026-11-23

Installs stock upstream Rayhunter v0.13.0 on the device and validates the end-to-end capture path. The network installer deploys the daemon, config, `rayhunter_daemon` init and the hooked `misc-daemon` exactly as the install-footprint RE predicted; after a reboot the daemon records and the Phase 1 OTA block persists. Validated on the inactive built-in SIM without an active plan -- `rayhunter-check` analyzes a live capture clean, confirming capture + offline analysis work on this unit. Upstream pin: EFForg/rayhunter `v0.13.0` (`cd462a1`). The install is the reversible footprint from Phase 1's RE; the Phase 1 backup is the rollback.

### Added

- Phase 2: upstream Rayhunter v0.13.0 installed and validated on the device (T-OR-021/T-OR-022).
  The network installer deployed the daemon + `config.toml` + `rayhunter_daemon` init + the
  hooked `misc-daemon`, as the install-footprint RE predicted; after reboot the web UI/API answer
  on :8080, the daemon records, and the OTA-block renames persisted. A live QMDL (1282 messages)
  analyzed clean by `rayhunter-check` (0 warnings), with an `EMM Attach Reject` confirming the
  inactive SIM reaches towers -- end-to-end capture + analysis without an active plan.

## [0.2.0] - 2026-11-23

Completes Phase 1: on-device reconnaissance plus the reversible OTA-update block. The carrier updater (`oma_dm`/`dmclient`/`mdm-daemon`) is reverse-engineered and reversibly disabled; the 0x11EB user-IP-traffic privacy question is answered from the upstream source; and the no-active-service path is established (an inactive SIM captures broadcast control-plane, enough to validate the capture path; analyzer accuracy work runs offline with `rayhunter-check`). Builds on v0.1.0 (foundation + recon + verified backup). Upstream pin: EFForg/rayhunter `v0.13.0` (`cd462a1`). Nothing in this release modifies the device beyond the reversible OTA block, which is backup-gated and has an undo manifest.

### Added

- `scripts/orbic-ota-block.py`: reversibly disables the on-demand OTA/FOTA updater
  (`oma_dm`/`dmclient`/`mdm-daemon`) by renaming them to `.orx-disabled`. The first tool that
  writes to the device: a fixed target allowlist (no arbitrary paths), a verified-backup gate,
  an undo manifest and `--undo`, and an audit line. Status (read-only) is the default; `--apply`
  requires `--backup-manifest`. Applied on this unit (T-OR-018).
- `re/notes/ota-mechanism.md` and `re/notes/install-footprint.md`: read-only RE of the backed-up
  firmware (T-OR-020). Auto-FOTA is off on this unit, there is no boot-time update daemon, and
  the on-demand updater is triggered only by the stock UI's `goahead` `/goform/Fota*` actions or
  a carrier-pushed OMA-DM session. The upstream install footprint is a `misc-daemon` hook plus
  files under `/data`, fully file-level reversible.
- `ref-docs/running-without-active-service-2026-11-23.md`: how to run Rayhunter without an active
  plan. An inactive SIM (the built-in Verizon SIM here) captures broadcast control-plane only
  (SIB1/Paging) -- enough for the broadcast heuristics and to validate the capture path -- while
  connection-based heuristics and user-plane capture need an active data session. Analyzer
  accuracy work proceeds offline with `rayhunter-check` + golden QMDL vectors.
- T-OR-019 (0x11EB privacy check) answered from the upstream source: it subscribes to
  `LOG_DATA_PROTOCOL_LOGGING_C` ("User IP traffic"), so an active plan would log user IP packets
  to QMDL; the empirical on-device test is deferred (no active plan), the mitigation (T-OR-029)
  justified. Recorded in docs/DEVICE-PROFILE.md and docs/TESTING.md.

### Changed

- `scripts/orbic-ui-read.py` refuses all read-shaped `Fota*` endpoints (not only
  `FotaGetVersionInfo`), since each drives the device OTA flow.
- `docs/DEVICE-PROFILE.md` and `docs/BACKUP-AND-RECOVERY.md` record the measured OTA mechanism
  and the refined, evidence-based OTA-block plan (T-OR-018).

## [0.1.0] - 2026-11-23

Foundation plus Phase 1 on-device reconnaissance and a verified pre-write backup. The repository,
CI, docs and the research corpus are in place; the host `orx` tool and the read-only device tools
(`orbic-shell.py`, `orbic-ui-read.py`, `orbic-backup.py`) are shipped and self-tested; the
RC400L's layout is measured and every writable filesystem is backed up and verified. Nothing in
this release installs or modifies anything on the device. Upstream pin: EFForg/rayhunter
`v0.13.0` (`cd462a1`).

### Added

- `scripts/orbic-ui-read.py`: a read-only client for the stock GoAhead web UI. It logs in with
  the same encoding as upstream's installer (verified against four reference vectors from
  upstream's own `encode_password`), calls only an allowlist of read endpoints, and **refuses**
  `FotaGetVersionInfo` (which would ask the device to check for an OTA update). The admin
  password is read from a mode-600 file, never argv, and identifier fields are length-redacted
  on stdout.
- `scripts/orbic-shell.py`: drives the upstream root shell (busybox `nc` on TCP 24) one command
  at a time with per-command markers and exit status. It is read-only by policy, decided entirely
  on shlex TOKENS (so quoting cannot disguise a program or flag): a fail-closed allowlist of pure
  readers, with every arbitrary-execution program (`awk`, `command`, `find`, `sed`, `dd`,
  `nanddump`, `mount`) excluded, control characters and command substitution rejected, and output
  redirects validated per token. No recon run can write flash before the Sprint 1.3 backup, and
  there is no write-enable override. `scripts/recon/` holds the inventory command lists.
- `scripts/orbic-backup.py`: the Sprint 1.3 backup tool (T-OR-015..T-OR-017). It archives each
  mounted `ubifs` filesystem with `tar` on the device, streamed to the host over the port-24 root
  shell (the device firewall blocks a connect-back), and verifies each as a complete, readable tar
  with a two-pass SHA-256 (byte-identical for a quiescent fs, readable-only for a live one like
  `/data`), promoting it to a mode-0600 `<label>.tar` only on success. **Not** a raw NAND image:
  `nanddump`/`dd` of `/dev/mtd*` deadlocks this device (raw MTD contends with the live UBI and
  wedges the NAND), and tar through the VFS is both safe and the correct rollback for a Rayhunter
  install, which only changes the mounted volumes.
- Phase 1 Sprint 1.2 on-device recon (T-OR-013/T-OR-014) recorded in docs/DEVICE-PROFILE.md:
  kernel 3.18.48 and command line, the 17-partition MTD table and UBI layout, mounts and free
  space, the available backup tools (`nanddump`, `nandwrite`, `flash_erase`, `ubinfo`,
  `sha256sum`), listening sockets, the absence of a persistent FOTA daemon, and the stock
  `misc-daemon` SHA-256 baseline. The one device change from opening the shell (`telnetd` 1->0
  in the remote-access config) is noted; identifiers are redacted.
- Repository scaffold: GPL-3.0-or-later license (ADR 0002), overlay layout (ADR 0001), upstream
  EFForg/rayhunter submodule pinned to `v0.13.0` (`cd462a1`), Rust 1.98.0 toolchain pin matching
  upstream.
- `orx` host tool (`crates/orx-cli`, `unsafe_code = "forbid"`, no dependencies) with
  `orx doctor [--ip <addr>]`: Qualcomm USB discovery through sysfs, classifying `05c6:f626`
  (stock RNDIS + CDC), `05c6:f601` (debug ADB + DIAG + AT) and `05c6:9008` (EDL), plus TCP
  reachability of the stock web UI (`:80`) and Rayhunter (`:8080`). Exit 1 when no device and no
  stock UI are found, 2 on usage or sysfs errors. Five unit tests.
- Build tooling: `scripts/apply-patches.sh` (exports the committed submodule tree with
  `git archive` and applies `patches/series` with `git apply --check` first, never touching
  `upstream/`), `scripts/build.sh devel|firmware` (upstream web frontend plus upstream cargo
  aliases for `armv7-unknown-linux-musleabihf`, artifacts and `SHA256SUMS` in `dist/`),
  `scripts/sync-upstream.sh [<tag>]` (pinned vs latest, trial patch apply),
  `scripts/gen-assets.sh` (derived icons, favicons, banners and SVG renders).
- Device configuration template `device/config/config.orbic.toml` (upstream keys only; double-tap
  recording and larger free-space floors as documented deviations).
- CI, with every action pinned to a commit SHA: `ci.yml` (host crate fmt, clippy `-D warnings`,
  tests; shellcheck; patch series apply plus armv7 daemon and rootshell build with artifact
  upload), `docs.yml` (markdownlint, lychee), `security.yml` (gitleaks 8.30.1, cargo-audit,
  CodeQL for Actions and Rust), `upstream-watch.yml` (weekly new-tag issue with trial patch
  apply), `release.yml` (tag-driven build, checksums, notes from this file, never duplicates a
  release). Dependabot for Actions and Cargo; the submodule is moved deliberately.
- Community files: README, CONTRIBUTING, SECURITY (private vulnerability reporting),
  CODE_OF_CONDUCT (Contributor Covenant 2.1), issue forms, PR template, CODEOWNERS, AGENTS.md and
  CLAUDE.md.
- Research corpus `ref-docs/`: upstream overview, Orbic install procedures, community install
  reports, RC400L hardware, firmware V1.2.0, Qualcomm DIAG/QMDL, related tools, LTE IMSI-catcher
  detection literature with a ranked analyzer backlog, legal and ethics notes, and a dated
  reconciliation supplement for five cross-document conflicts. `ref-docs/pdfs/README.md` lists 20
  locally downloaded papers with source and stated license.
- Specification `docs/`: architecture, device profile, install, backup and recovery,
  configuration, analyzers, testing, performance, security model, patch policy, build,
  reverse-engineering methodology, release process, ADRs 0001-0002.
- Planning: `ROADMAP.md` (v0.1.0 to v2.0.0, tickets T-OR-001 to T-OR-105), `PROJECT-STATUS.md`,
  and eight phase plans in `to-dos/`.
- Artwork: icon, logo, hero banner and social card generated with `agy`; four SVG diagrams
  (architecture dark and light, detection pipeline, release roadmap); README header, badges,
  research findings, layout and documentation index, and Mermaid build-pipeline and install
  safety-gate diagrams.

### Security

- Antigravity reviewer re-synced from template `1451f73`: agy runs in a bubblewrap jail
  (`AGY_ISOLATION=bwrap`) with an empty private `HOME` (no MCP servers, skills, hooks or
  repositories), read-only `/usr` and `/etc`, and a D-Bus proxy exposing only its own keyring
  item. Its network is a TLS-intercepting egress proxy that allows only exact Google hosts and
  rebuilds every request. Credential-shaped output is redacted before posting, and anything
  redaction cannot remove stops the post. agy's own `--sandbox` was verified not to confine
  reads.
- `.gitignore` excludes device backups, firmware images, captures (`*.qmdl`, `*.qmdl.gz`,
  `*.ndjson`, `*.pcap`), Ghidra projects, extracted firmware, `wpa_sta.conf`, `.env` files and
  downloaded papers whose licenses do not permit redistribution.
- Device identifiers are kept out of the repository; test fixtures use a synthetic USB serial.
- `upstream-watch.yml` validates upstream tag names against strict SemVer and passes them to
  the shell through `env` instead of expression interpolation; every `actions/checkout` sets
  `persist-credentials: false`.
- Antigravity reviewer re-synced from the hardened template: agy runs from a private sandbox
  directory outside the checkout (large-diff handoff included), with an allowlisted environment
  (`env -i` plus the session variables its keyring-held login needs, so no job token or
  workflow `env:` value reaches it under any name), and post-agy `git` calls ignore hooks. The interim
  `AGY_DIFF_MODE: inline` workaround is removed, and with it the unused `/.agy-review-work/`
  ignore line.

### Added (CI)

- Antigravity PR reviewer (`antigravity-review.yml`, `scripts/agy-review*.sh`, `scripts/_agy_*.sh`,
  `.github/AGY-REVIEWER.md`), vendored from the `antigravity-pr-review` template: reviews every
  same-repository PR on open, reopen and push, and on `/agy-review`, using the self-hosted `agy`
  runner and the `agy` CLI's OAuth session. Project review rules in `.github/agy-review.md`.
  Fork PR workflows now require approval for all outside contributors.
- The reviewer's offline self-test runs in the `shellcheck` CI job.

### Changed

- CI is path-aware: `ci`, `docs` and `security` each begin with a `detect changes` job
  (`dorny/paths-filter`) and skip jobs whose inputs did not change, using job-level `if:` so
  required checks still report as passing. `gitleaks` always runs; scheduled and manual runs
  run every job. The detection jobs (`detect changes (ci|docs|security)`) are themselves
  required checks, so a detection failure blocks the merge instead of silently skipping every
  gated check (fail closed). See `docs/CI.md`.
- CI caching: Rust caches saved only from `main` (restored by PRs and releases) for the host
  crates and the patched upstream's third-party dependencies, rustup toolchain plus armv7
  target, npm, the SHA-256-verified gitleaks binary and lychee link results. Pull-request runs cancel superseded runs; every job has a timeout;
  build artifacts are retained 14 days.
- `scripts/apply-patches.sh` stages upstream plus patches in a scratch directory and syncs it
  into `build/rayhunter` by content (`rsync --checksum --delete`), never touching `target/` or
  `node_modules/`: incremental local builds and restored CI caches survive `scripts/build.sh`,
  and only changed files get a fresh mtime so Cargo never reuses a stale patched artifact.
  Requires `rsync`.
- Non-PR workflow runs get a per-run concurrency group, so a pending run on `main` is never
  replaced (and its changes never left unvalidated) by a later push.
- `cargo-audit` runs the pinned `cargo-audit` 0.23.0 CLI (installed by `taiki-e/install-action`)
  with `--deny warnings` instead of `rustsec/audit-check`, whose latest release targets the
  deprecated Node.js 20 runtime and required `checks`/`issues` write permissions.
- The docs `links` job also runs on any deletion or rename, since local link targets are not
  only Markdown.
- Rust caches use a `shared-key` so `release.yml` restores the caches `ci.yml` saves.
- `docs.yml` gains a weekly link-rot run.
- `main` protection moved from a classic branch protection rule to the `main` repository ruleset:
  same required checks (now pinned to the GitHub Actions app), linear history, no force pushes or
  deletions, PRs with resolved threads and squash merges; repository admins keep bypass.

### Fixed

- `orx doctor` printed `serial=?` for the RC400L in normal mode, which exposes no `serial` sysfs
  attribute and carries the serial in its product string (`MDM9207-MTP _SN:<serial>`). It now
  falls back to the `_SN:` token. The token is accepted only directly after a word-start `_SN:`
  marker, as 1-64 ASCII alphanumerics; a real `serial` attribute still takes precedence. The
  product string is split once when the device is read (`split_product()`): `product` keeps only
  the model text and the serial lives only in `serial`, so neither the doctor output nor a `Debug`
  print carries the serial twice (T-OR-106). Measured on the unit during the T-OR-012
  host-side recon.
- `scripts/apply-patches.sh` silently skipped every patch when staging inside this repository
  (the default `build/` layout): `git apply` discovered the superproject's `.git` and ignored
  patched paths outside its subdirectory while exiting 0. It now runs with
  `GIT_CEILING_DIRECTORIES` so it acts as plain `patch(1)`. Found by the Antigravity review; the
  series was still empty, so no build was affected. New regression test `tests/apply-patches.sh`
  (patch applied in and outside the repo, removal restores upstream with a fresh mtime,
  `target/` survives, missing and non-applying patches fail) runs in CI before every build.
- Change-detection jobs fetch full history, so on a push `dorny/paths-filter` can diff against
  `github.event.before` instead of treating every file as added in a shallow clone.
- `markdownlint` and `links` now cover every Markdown file, including `.github/` and the
  subdirectory READMEs; `lychee.toml` changes trigger only `links`; CodeQL also runs on
  `Cargo.lock` and `rust-toolchain.toml` changes.
- The armv7 build installs the upstream toolchain and target before `rust-cache` probes `rustc`;
  `release.yml` restores the rustup cache and no longer restores the dev-profile host cache.
- The reviewer's first-install bootstrap steps are removed (CodeQL `actions/untrusted-checkout`).
- The `review` job skips with a visible warning, instead of failing red, while the reviewer
  scripts are not yet on the default branch (the PR that installs it); any failure inside the
  reviewer still fails the job.
- `scripts/apply-patches.sh` canonicalizes the destination and patch directory: git ignores
  relative `GIT_CEILING_DIRECTORIES` entries, so a relative `$1` would again have skipped every
  patch. Tests now cover relative paths and assert that unchanged files keep their mtime.
- The patched-upstream Rust cache is no longer keyed on `patches/**` (rust-cache stores only
  third-party dependencies, so a patch edit discarded it for nothing); the lychee cache uses a
  per-run key and the scheduled run re-checks every link; scheduled and manual runs skip the
  unused full-history checkout; the exec-bit gate covers `tests/*.sh`.

- Scripts are committed with mode `100755`; the first CI run failed with `Permission denied`
  because the repository uses `core.fileMode=false`. CI now fails early if a script loses its
  executable bit.
