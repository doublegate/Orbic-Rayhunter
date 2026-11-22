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

- [ ] T-OR-032: classify `05c6:f622`; list interfaces with class/subclass/protocol per function
- [ ] T-OR-032: report the firmware string from the stock UI where obtainable without login
      (TBD (on-device) whether it is), Rayhunter `/api/system-stats` when :8080 answers
- [ ] T-OR-032: `--json` output; documented exit codes
- [ ] T-OR-033: shell session over the network shell (start via upstream `util orbic-start-telnet`
      or equivalent, connect to port 24, command framing with an exit-code marker)
- [ ] T-OR-033: checksummed file transfer (device listener, host stream, device `sha256sum` or
      `md5sum`, atomic `mv`)

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

- [ ] T-OR-034: enumerate `/proc/mtd`; stream each partition; hash both ends; write
      `MANIFEST.sha256`, `device-hashes.txt`, `layout/`
- [ ] T-OR-034: file-level archive and stock-file copies (BACKUP-AND-RECOVERY.md step 4)
- [ ] T-OR-035: `orx backup verify`: manifest, sizes vs `/proc/mtd`, second-pass comparison,
      presence of the off-repo copy marker
- [ ] T-OR-035: backup gate library function used by every write subcommand

Acceptance criteria: `orx backup` on the real unit reproduces the Phase 1 hashes for every
partition not mounted writable.

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

- [ ] T-OR-036: run upstream `installer orbic` on a stock unit; then replace daemon and config
      from `dist/` and `device/config/` with `.prev` copies (INSTALL.md step 4)
- [ ] T-OR-036: refuse when `dist/SHA256SUMS` fails, the backup gate fails, or the USB mode is
      not normal
- [ ] T-OR-037: `orx verify`: installed hashes, daemon process, API, active recording, init hooks
      present, no port 24/8081 listener

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

- [ ] T-OR-038: list via `/api/qmdl-manifest`; download QMDL, ndjson and GPS files; host
      manifest with SHA-256; skip already-pulled entries
- [ ] T-OR-039: `orx uninstall` implementing INSTALL.md step 6, restoring stock files from the
      verified backup and verifying their hashes
- [ ] T-OR-039: `orx restore <file>` for single stock files from the backup

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

- [ ] T-OR-040: `orx purge`: delete recordings on the device only after a verified pull, by age,
      total size, or count; never the current recording; keep flagged recordings unless forced
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
markdownlint-cli2 '**/*.md'
gh release view v1.0.0
```

### Research (REA/Ghidra/web)

- None new; review open upstream issues affecting the Orbic since v0.13.0 before tagging.
