# Phase 2: Stock upstream install via network installer (v0.3.0)

Goal: install the upstream v0.13.0 release with the default network installer, verify it, remove
it completely including the byte-identical stock `misc-daemon`, and reinstall. Tickets
T-OR-021..T-OR-025 ([ROADMAP.md](../ROADMAP.md)). Procedure: [docs/INSTALL.md](../docs/INSTALL.md)
steps 1-3, 5, 6.

Precondition: Phase 1 complete (verified backup, OTA block).

## Sprint 2.1: Preflight and install

- [ ] T-OR-021: preflight gate per INSTALL.md step 1 (doctor, backup manifest check, `misc-daemon`
      diff reviewed and recorded, `df -h /data`, firmware string)
- [ ] T-OR-021: decide from the `misc-daemon` diff whether any stock boot step would be lost; if
      so, stop and design the mitigation (input to T-OR-095) before installing
- [ ] T-OR-022: download `rayhunter-v0.13.0-linux-x64.zip` and `.sha256`; verify
- [ ] T-OR-022: `./installer orbic --admin-password "$ORX_ADMIN_PW"` over the Orbic network; keep
      the full output in the install log with date, host, link type (Wi-Fi AP or RNDIS) and
      firmware string
- [ ] T-OR-022: if it fails, follow the INSTALL.md error table; record each attempt; do not
      switch to `orbic-usb` without a decision recorded in the install log

Acceptance criteria: installer exits successfully; device reboots; web UI answers on :8080.

Verification:

```sh
sha256sum -c rayhunter-v0.13.0-linux-x64.zip.sha256
cargo run -p orx-cli -- doctor        # expect rayhunter: reachable
curl -s http://192.168.1.1:8080/api/system-stats
```

### Research (REA/Ghidra/web)

- Upstream `installer/src/orbic_network.rs::setup_rayhunter` (write order, remount at line 225 of
  v0.13.0) against what the install log shows.
- Upstream issues #901 (1.2.8 telnet-stage failure), #693 (remount exit code), #966/#955 (USB-only
  link failures): check for fixes merged after v0.13.0 before retrying failures.

## Sprint 2.2: Verify

- [ ] T-OR-023: run the [docs/TESTING.md](../docs/TESTING.md) smoke list (items 2-13; item 1
      compares against the release binary hash here)
- [ ] T-OR-023: record the DIAG ioctl convention that succeeded, `LOG_CONFIG_RETRIEVE_ID_RANGES`
      sizes and observed `ext_header_version` of 0xB0C0 from the daemon log and a recording
      (DEVICE-PROFILE.md DIAG rows)
- [ ] T-OR-023: confirm no port 24 or 8081 listener remains after the reboot

Acceptance criteria: all smoke items pass including reboot persistence and hotspot intact.

Verification:

```sh
curl -s http://192.168.1.1:8080/api/qmdl-manifest
curl -s http://192.168.1.1:8080/api/log | grep -i diag
nmap -Pn -p 24,8081 192.168.1.1
```

### Research (REA/Ghidra/web)

- Upstream `lib/src/diag_device.rs` (ioctl fallbacks) and `lib/src/gsmtap/parser.rs`
  (`ext_header_version` groups) to interpret the log lines.

## Sprint 2.3: Uninstall/restore rehearsal and reinstall

- [ ] T-OR-024: pull recordings worth keeping; then INSTALL.md step 6 manual uninstall
- [ ] T-OR-024: restore stock `misc-daemon` from the backup; device hash equals backup hash
- [ ] T-OR-024: confirm `/data/rayhunter`, `/data/rayhunter-data`, `/etc/init.d/rayhunter_daemon`
      absent; stock behavior (hotspot, tethering, menu) unchanged
- [ ] T-OR-024: reinstall with `installer orbic`; smoke list passes again
- [ ] T-OR-025: post the V1.2.0 result upstream (discussion or docs PR), with no identifiers

Acceptance criteria: restore verified by hashes; reinstall verified by the smoke list; upstream
report link recorded in PROJECT-STATUS.md.

Verification:

```sh
# device shell
sha256sum /etc/init.d/misc-daemon          # equals the value in MANIFEST / file hashes
ls /data/rayhunter /data/rayhunter-data /etc/init.d/rayhunter_daemon
# host
cargo run -p orx-cli -- doctor              # rayhunter: not running / not installed
```

### Research (REA/Ghidra/web)

- Upstream `doc/uninstalling.md` and `doc/faq.md` (tethering via `/usrdata/mode.cfg`) against
  what the rehearsal needed; propose a docs PR for the leftover `/data/rayhunter-data` directory
  and the `misc-daemon` restore ([ref-docs/rayhunter-install-orbic.md](../ref-docs/rayhunter-install-orbic.md)
  section 6).
