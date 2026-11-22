# Phase 6: Performance, UI/UX, security hardening (v1.4.0..v1.6.0)

Goal: measure, then improve, resource use and battery life; fix the display and web UI gaps; and
close the attack surface in [docs/SECURITY-MODEL.md](../docs/SECURITY-MODEL.md). Tickets
T-OR-069..T-OR-088 ([ROADMAP.md](../ROADMAP.md)).

## Sprint 6.1: Re-baseline and profiling (v1.4.0, part 1)

- [ ] T-OR-069: re-run every runtime metric in [docs/PERFORMANCE.md](../docs/PERFORMANCE.md) on
      the v1.3 build (three runs per condition)
- [ ] T-OR-070: CPU and RSS over time, idle versus active LTE; identify the dominant cost
      (DIAG read, gzip, analysis, HTTP) before proposing any change
- [ ] T-OR-073: boot-to-recording time, cold boot, three runs

Acceptance criteria: PERFORMANCE.md table complete for v1.3 with commands and conditions; a
written hypothesis for each candidate optimization, tied to a measurement.

Verification:

```sh
# device shell
grep -E 'VmRSS|VmHWM' /proc/$(pidof rayhunter-daemon)/status
grep MemAvailable /proc/meminfo
cat /proc/uptime
```

### Research (REA/Ghidra/web)

- Upstream `daemon/src/diag.rs` and `lib/src/qmdl.rs` (10 MiB read buffer, gzip writer) as the
  likely hot paths; confirm with measurement, do not assume.
- `perf` availability on the device kernel (3.18) or a sampling alternative over `/proc/<pid>/stat`.

## Sprint 6.2: Optimization, battery, supervision (v1.4.0, part 2)

- [ ] T-OR-071: QMDL write path: bytes written per hour and write batching; before/after rows
- [ ] T-OR-072: battery drain per hour (bucketed) by `ui_level`, with a stock (no daemon) reference
- [ ] T-OR-074: `[LOCAL]` daemon supervision (respawn with backoff in the init script or a small
      supervisor); measured restart cost; no respawn loop on persistent failure

Acceptance criteria: every change carries before and after rows; golden vectors unchanged.

Verification:

```sh
scripts/apply-patches.sh && (cd build/rayhunter && cargo test)
# device: kill -9 $(pidof rayhunter-daemon); confirm respawn and a new log line
```

### Research (REA/Ghidra/web)

- Stock init scripts from the backup: whether the firmware already has a supervisor pattern
  (`start-stop-daemon` usage, `reboot-daemon`) to reuse.
- `/sys/kernel/chg_info/*` semantics (Ghidra on the charger-related kernel module or userspace
  reader if one exists) for a finer battery reading than five buckets.

## Sprint 6.3: UI/UX (v1.5.0)

- [ ] T-OR-075: keep-screen-on option (#916, #539)
- [ ] T-OR-076: do not show recording state in charge-mode boot (#1160)
- [ ] T-OR-077: web UI explanations and false-positive guidance for the v1.2 analyzers
- [ ] T-OR-078: web UI configuration for WebDAV and notification sinks (secrets write-only)
- [ ] T-OR-079: `orx status` live view from the API
- [ ] T-OR-080: probe framebuffer geometry instead of hardcoding 128x128

Acceptance criteria: upstream `daemon/web` gates pass (`npm run lint`, `check`, `test`); no
secret is returned by any `GET`.

Verification:

```sh
(cd build/rayhunter && npm run lint -w daemon/web && npm run check -w daemon/web && npm run test -w daemon/web)
cargo run -p orx-cli -- status
```

### Research (REA/Ghidra/web)

- Stock display and backlight control: Ghidra on the stock UI/menu process (name TBD (on-device)
  from T-OR-014) for the backlight timeout mechanism; `/sys/class/backlight` and `fb0` sysfs.
- Charge-mode boot detection: `/proc/cmdline` `androidboot.poweronreason` (mjg59) and the stock
  init scripts' handling of charge mode.
- Upstream `daemon/src/display/orbic.rs`, `generic_framebuffer`, `daemon/web`.

## Sprint 6.4: API authentication and exposure (v1.6.0, part 1)

- [ ] T-OR-081: authentication for the HTTP API and web UI (design agreed upstream first:
      token or password, session handling, how `rayhunter-check` and `orx` authenticate)
- [ ] T-OR-082: configurable bind address and interface restriction
- [ ] T-OR-083: `orx install` and `orx verify` assert no port 24/8081 listener and no setuid
      `/bin/rootshell` on network installs; removal path for USB installs
- [ ] T-OR-087: file permissions for `config.toml` and `wpa_sta.conf`; secrets omitted from
      `GET /api/config`

Acceptance criteria: unauthenticated requests to every API route are rejected (test enumerates the
router); `nmap` shows only intended listeners; existing installs keep working after the upgrade
path documented in CHANGELOG.

Verification:

```sh
curl -s -o /dev/null -w '%{http_code}\n' http://192.168.1.1:8080/api/qmdl-manifest   # expect 401
nmap -Pn -p- 192.168.1.1
# device shell
ls -l /bin/rootshell /data/rayhunter/config.toml /data/rayhunter/wpa_sta.conf
```

### Research (REA/Ghidra/web)

- Upstream `daemon/src/main.rs::get_router` and `run_server` (bind, routes).
- Ghidra on `goahead`: whether `SetRemoteAccessCfg` or another handler can be disabled without
  breaking the stock UI, and whether the injection is reachable without login (expected no).
- Ghidra on `atfwd_daemon`: `AT+SYSCMD` handler and whether it can be restricted while keeping
  stock behavior.

## Sprint 6.5: Supply chain, update path, audit (v1.6.0, part 2)

- [ ] T-OR-084: stock admin hardening guidance and `orx doctor` checks (admin password distinct
      from the Wi-Fi key if the firmware allows; WPA2 key strength advice)
- [ ] T-OR-085: sign release artifacts; `orx install` verifies the signature before transfer
- [ ] T-OR-086: update checker scope documented; `orx doctor` reports OTA block state
- [ ] T-OR-088: threat-model review of SECURITY-MODEL.md; pre-release security audit

Acceptance criteria: an unsigned or modified `dist/` is refused by `orx install` (test); audit
findings either fixed or recorded with rationale.

Verification:

```sh
cargo test -p orx-cli signature
gh release view v1.6.0
```

### Research (REA/Ghidra/web)

- Signing options compatible with GitHub releases (for example Sigstore/cosign or minisign):
  current documentation and key-management implications.
- Stock web admin password-change handler in `goahead` (whether admin and Wi-Fi credentials are
  separable on V1.2.0).
