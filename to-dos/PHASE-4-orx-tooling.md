# Phase 4: orx tooling and v1.0.0 baseline (v0.5.0..v1.0.0)

Goal: every manual procedure from Phases 1-3 becomes a fail-closed `orx` subcommand, then v1.0.0
ships with a measured baseline. Tickets T-OR-032..T-OR-044 ([ROADMAP.md](../ROADMAP.md)).

Design rules for every subcommand:

- Device access goes through a trait (USB enumeration, shell session, file transfer, HTTP API)
  with a fake implementation used in unit tests; no test touches hardware.
- Write commands call the backup gate (T-OR-035) first and refuse on a missing or failed backup.
- Every transfer is verified by a device-side hash against an expected value.
- Never run an unqualified `adb`; any ADB use takes an explicit serial.
- The admin password is read from a prompt or an environment variable, never from argv of `orx`
  itself; when `orx` invokes the upstream installer, note that the installer's argv is visible in
  the host process list.

## Sprint 4.1: `orx doctor` v2 and transport (v0.5.0)

- [x] T-OR-032: `orx doctor` reports each Qualcomm USB device with vid:pid, product, serial
      and the inferred mode (f626 normal / f601 debug / 9008 EDL / else UnknownQualcomm);
      interface-level class/subclass listing is deferred to when a non-f626 composition is on hand
- [x] T-OR-032: Rayhunter version read from `/api/system-stats` when :8080 answers (verified
      on-device: `Rayhunter 0.13.0`). Stock-UI firmware string without login is still TBD
      (on-device) -- the page load triggers the FOTA client, so it is deferred
- [x] T-OR-032: `--json` output; documented exit codes (0 found / 1 nothing / 2 usage|host)
- [x] T-OR-033: `transport::TcpShell` runs a command over the TCP-24 root shell with an
      exit-code marker (`{ cmd ; } 2>&1; printf 'MARKER %s.' "$?"`), total-deadline bounded;
      behind a `Shell` trait with an in-memory `FakeShell` for tests
- [x] T-OR-033: `Shell::send_file` streams bytes to the device (`cat > remote`), then accepts
      only if the device's `sha256sum` matches the host-side SHA-256 of what was sent; rejects
      unsafe remote paths first. Failure paths (hash mismatch, dropped write, unsafe path,
      timeout) unit-tested

Acceptance criteria: unit tests for every PID and for transfer failure paths (short write, hash
mismatch, timeout); doctor output on the real unit matches DEVICE-PROFILE.md.

Verification:

```sh
cargo test -p orx-cli
cargo run -p orx-cli -- doctor --json
```

### Research (REA/Ghidra/web)

- Upstream `installer/src/util.rs` (`telnet_send_file`, timeouts, flush delay) and
  `connection.rs` for the transfer protocol to mirror.
- `goahead` RE notes from T-OR-020: unauthenticated endpoints that expose the firmware version.

## Sprint 4.2: `orx backup` (v0.6.0)

- [x] T-OR-034: `orx backup --out <dir> [--allow-live]` archives each writable filesystem with
      `tar` over the shell (raw `/proc/mtd` dumps deadlock this device -- a tar of the mounted
      `ubifs` is the correct backup, per Sprint 1.3), streaming to `0600` `<fs>.tar` while hashing
      inline, and writes a gate-compatible `MANIFEST.txt`. Validated on the unit (root.tar
      86.7 MB, data/usrdata/cache `ok`; inline SHA-256 == independent `sha256sum`)
- [x] T-OR-035: `orx verify --dir <dir>` re-reads MANIFEST.txt and re-hashes each archive
      (ok/missing/too-small/size-mismatch/hash-mismatch); both manifest layouts parse
- [x] T-OR-035: `verify::gate()` requires rootfs `/` ok/ok-live AND its archive to verify;
      `orbic-ota-block.py` `_backup_gate` hardened to match. `orx install`/`uninstall` will call it
- [ ] T-OR-035 follow-up: `orx verify` does not yet check the off-repo-copy marker (the encrypted
      copy on separate media, BACKUP-AND-RECOVERY.md step 5). Tracked for a later sprint; the gate
      covers the on-disk archives, not the off-repo copy

Acceptance criteria: `orx backup` on the real unit archives every writable filesystem with a
verified inline hash and a gate-compatible manifest. MET (validated end-to-end 2026-11-23).

Verification:

```sh
cargo test -p orx-cli backup
cargo run -p orx-cli -- backup --out backups/<date>-V1.2.0-orx
cargo run -p orx-cli -- backup verify backups/<date>-V1.2.0-orx
diff <(sort backups/<date>-V1.2.0/MANIFEST.sha256) <(sort backups/<date>-V1.2.0-orx/MANIFEST.sha256)
```

### Research (REA/Ghidra/web)

- Busybox applet list from T-OR-014: which of `nanddump`, `nc -w`, `sha256sum` are available,
  to choose the streaming primitive.
- mtd-utils `nanddump` options for OOB inclusion (host man page), for a future bit-exact backup.

## Sprint 4.3: `orx install` and `orx verify` (v0.7.0)

- [x] T-OR-036: `orx install --dir <backup> [--apply]` (v0.8.0) deploys our firmware build
      (daemon + rendered config.toml + rayhunter_daemon + hooked misc-daemon, chmod, reboot) --
      the upstream network-installer footprint with our build. Dry-run unless `--apply`; each
      transfer is SHA-256-verified by the device
- [x] T-OR-036: refuses unless `verify::gate()` passes (a verified rootfs backup); dry-run
      validated on the unit against the Sprint 1.3 backup. (A separate post-install on-device
      health check is the planned `orx check`, T-OR-037, not yet implemented)
- [x] T-OR-037 (v1.0.4): `orx check` reports device health from the `:8080` API (daemon reachable,
      rayhunter version/OS/arch, disk/memory/battery, active recording + stored count) and probes
      ports (daemon :8080 open; root shell :24 and installer :8081 expected closed). Read-only;
      `[ok]/[warn]/[fail]` summary; exits non-zero on any failure. Validated live (device healthy).
      Installed-hash and init-hook checks need the :24 shell (not reboot-persistent) -- deferred.

Acceptance criteria: install and verify on the real unit from the Phase 2 state; refusal paths
covered by unit tests.

Verification:

```sh
cargo test -p orx-cli -- install verify
cargo run -p orx-cli -- install --dist dist/
cargo run -p orx-cli -- verify
```

### Research (REA/Ghidra/web)

- Upstream `installer/src/lib.rs` (`run_with_callback`, library API since v0.9.0): whether `orx`
  can call the installer as a library instead of a subprocess (GPL-compatible; same license).

## Sprint 4.4: `orx pull-logs` and `orx restore` (v0.8.0)

- [x] T-OR-038 (v1.0.3): `orx pull-logs --to <dir>` lists via `/api/qmdl-manifest`, downloads each
      recording's QMDL + analysis NDJSON (streamed, bounded), writes a SHA-256 PULL-MANIFEST, and
      skips completed recordings unchanged since the last pull (`.orx-pull-state`). Validated live.
      GPS files (when `gps_mode > 0`) are a follow-up; names are validated against path/URL injection.
- [x] T-OR-039: `orx uninstall --dir <backup> [--apply]` (v0.8.0) restores the true stock
      misc-daemon extracted from the verified backup's `root.tar` (gated before extract; refuses an
      already-hooked script), atomically, and removes our files, leaving `/data/rayhunter-data`
      recordings; dry-run unless `--apply`, backup-gated
- [ ] T-OR-039: `orx restore <file>` for arbitrary single stock files from the backup -- follow-up

Acceptance criteria: uninstall on the real unit leaves `misc-daemon` byte-identical to the backup
and no Rayhunter files; reinstall afterwards passes `orx verify`.

Verification:

```sh
cargo run -p orx-cli -- pull-logs --out ~/orbic-captures/
cargo run -p orx-cli -- uninstall
cargo run -p orx-cli -- doctor
```

### Research (REA/Ghidra/web)

- Upstream API (`daemon/src/main.rs::get_router`, `/api/zip/{name}`): download semantics for the
  currently open recording.
- Issue #701 (uninstall over ADB fails on 1.3.0 due to file ownership): check whether the
  network shell has the same ownership limits on V1.2.0.

## Sprint 4.5: Retention and rescue (v0.9.0)

- [x] T-OR-040 (v1.0.5): `orx purge` deletes COMPLETED recordings via
      `POST /api/delete-recording/{name}`, never the active one. Dry-run by default (lists what would
      be deleted + recommends `orx pull-logs` first). `--apply` requires a gate: `--pulled <dir>`
      deletes only recordings verified present in that pull dir (SHA-256 matched against
      PULL-MANIFEST.txt, and the current manifest size equal to the size `orx pull-logs` recorded for
      the completed recording in `.orx-pull-state` -- so a stale/partial pull is never used to
      authorize deleting a larger completed recording), or `--force` to delete without a local copy;
      bare `--apply` is refused. Fails closed on a missing `entries` array, a missing/invalid
      `qmdl_size_bytes`, or a malformed `current_entry`; a 403 (daemon debug mode) is reported clearly.
      Dry run and the verified-pull gate (empty dir -> 0 deleted, all skipped) validated live on the
      unit; the delete path is unit-tested against a loopback server (real captures not deleted).
      Age/size/count policies and a "keep flagged" flag are a follow-up (T-OR-040b).
- [ ] T-OR-041: `orx rescue`: when :8080 is down, use the network shell to report `df -h /data`,
      stop the daemon, and move or delete the oldest recordings after confirmation
- [ ] T-OR-041: rehearse a controlled fill of `/data` to the configured threshold (not to 100
      percent) and confirm the guard and the rescue path

Acceptance criteria: purge never deletes an unpulled recording (unit-tested); rescue works with
the daemon stopped.

Verification:

```sh
cargo test -p orx-cli -- purge rescue
cargo run -p orx-cli -- purge --older-than 30d --dry-run
cargo run -p orx-cli -- rescue --dry-run
```

### Research (REA/Ghidra/web)

- Issue #1192 and upstream `daemon/src/diag.rs` (disk check every `DISK_CHECK_BYTES_INTERVAL`):
  why the 1 MB guard did not fire; whether `rayhunter.log` growth explains it.
- Issue #607 (storage management request): avoid diverging from any upstream design in progress.

## Sprint 4.6: v1.0.0 baseline

- [ ] T-OR-042: restore stock state with `orx uninstall`; install, verify, pull, purge using only
      the docs and `orx`
- [ ] T-OR-043: fill the [docs/PERFORMANCE.md](../docs/PERFORMANCE.md) runtime table for the v1.0.0
      build (three runs per condition)
- [ ] T-OR-044: docs sweep (INSTALL.md matches the commands exactly), README support matrix,
      CHANGELOG, release per RELEASE-PROCESS.md

Acceptance criteria: a second person could follow INSTALL.md without asking; baseline table has
no empty cell for the v1.0.0 conditions.

Verification:

```sh
cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test
markdownlint-cli2 '**/*.md' '.github/**/*.md'
gh release view v1.0.0
```

### Research (REA/Ghidra/web)

- None new; review open upstream issues affecting the Orbic since v0.13.0 before tagging.

## v0.8.0 review follow-ups (deferred from PR #6, agy suggestions)

Additive hardening for `orx install`, deferred from the v0.8.0 PR as next-PR work (the shipped
command is backup-gated, dry-run-first, atomic, and validates payloads; these make it stricter):

- [x] T-OR-036a (v1.0.1): `orx install` verifies the daemon against a `SHA256SUMS` beside it when
      present, (refusing if it is missing unless `--allow-unverified`) and refuses an unstripped (devel)
      daemon unless `--allow-unstripped` -- catching a corrupted/swapped/missing artifact and the
      ~16 MB debug build staged by mistake.
- [x] T-OR-036b (v1.0.2): `orx install` deploys our hardened `device/config/config.orbic.toml` by
      default (key_input_mode = 1, min_space 64/32 MiB, wifi_enabled = false, auto_check_updates =
      false, full analyzer set), `--config <path>` override, `device = "orbic"` enforced.
- [x] T-OR-036c (v1.0.2): `/data` free-space preflight on `--apply` -- `df /data` (row validated
      against the mountpoint), refuse if it lacks the daemon + config + 8 MiB slack (#1192). Only
      /data is gated (the KiB `/etc/init.d` writes are not; a full rootfs just fails the atomic
      write), so uninstall is never blocked. Stopping the daemon first stays a future option if
      measured growth needs it (T-OR-068).
- [ ] T-OR-036d: bound the `orx install` data listener's lifetime and exposure (busybox `nc -w`
      idle-timeout and/or a `timeout` wrapper, and `-s` bind to the USB-side address if supported)
      -- needs on-device verification of the available busybox nc flags; the one-shot listener +
      device-side hash check already bound the risk, and the pre-connect peek fails closed on a
      stale listener.
