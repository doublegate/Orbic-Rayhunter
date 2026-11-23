# Phase 1: Device recon + verified backup (v0.2.0)

Goal: know exactly what runs on this unit, hold a verified, encrypted, off-repo backup of every
flash partition before anything is written, block OTA updates, and settle whether 0x11EB records
user traffic. Tickets T-OR-012..T-OR-020 ([ROADMAP.md](../ROADMAP.md)). Procedures:
[docs/DEVICE-PROFILE.md](../docs/DEVICE-PROFILE.md),
[docs/BACKUP-AND-RECOVERY.md](../docs/BACKUP-AND-RECOVERY.md).

Ordering constraint: 1.1 (host-only) -> 1.2 (shell, read-only inventory) -> 1.3 (backup and
verification) -> 1.4 (OTA block, then SIM, then 0x11EB). No write to the device before 1.3 passes.

## Sprint 1.1: Host-side recon

- [x] T-OR-012: `lsusb -v -d 05c6:f626` saved to `backups/<date>-V1.2.0/layout/lsusb-f626.txt`
- [x] T-OR-012: interface class/subclass/protocol per USB function recorded in DEVICE-PROFILE.md
- [x] T-OR-012: `orx doctor` transcript; serial redacted before committing
- [x] T-OR-012: stock UI response headers (2026-11-23)
- [ ] T-OR-012: the firmware string as the UI reports it. It is shown only after the admin login,
      so it needs the admin password; read it once the OTA client is blocked (see below)
- [x] T-OR-012: host-side port scan of the device (expect 24 and 8081 closed before any shell)

Acceptance criteria: DEVICE-PROFILE.md host-side table filled; no write to the device.

Verification:

```sh
lsusb -v -d 05c6:f626 | head -50
cargo run -p orx-cli -- doctor
curl -sI http://192.168.1.1/
nmap -Pn -p 1-1024,5555,8080,8081 192.168.1.1
```

### Research (REA/Ghidra/web)

- Stock web UI JavaScript (fetched with `curl`, saved locally, not committed). **Do not open the
  stock UI in a browser before T-OR-018:** its header calls `/action/FotaGetVersionInfo` on page
  load, which asks the device to check for an update (if a browser is unavoidable, remove the SIM
  first):
  locate the login flow (`/goform/GetLoginInfo`, `/goform/login`) and the remote-access settings
  page that drives `/action/SetRemoteAccessCfg`.
- Web: Verizon RC400L update pages for the current published build list (V1.3.2 is the newest
  known, [ref-docs/orbic-firmware-v1.2.0.md](../ref-docs/orbic-firmware-v1.2.0.md) section 1).

## Sprint 1.2: Shell and read-only inventory

- [ ] T-OR-013: open the network shell with `installer util orbic-shell` (upstream v0.13.0
      release binary, zip SHA-256 verified); record exactly what was sent
- [ ] T-OR-013: note the stock remote-access settings before and after; after the next reboot
      check whether port 24 still listens
- [ ] T-OR-014: run every command in the DEVICE-PROFILE.md checklist (identity, kernel, memory,
      storage, processes, sockets, init layout, USB mode, DIAG node, peripherals)
- [ ] T-OR-014: copy outputs to `backups/<date>-V1.2.0/layout/`; summarize redacted values in
      DEVICE-PROFILE.md
- [ ] T-OR-014: inventory restore tools on the device (`nanddump`, `nandwrite`, `flash_erase`,
      `ubiupdatevol`, `sha256sum`, `tar`, `nc` options)

Acceptance criteria: every DEVICE-PROFILE.md on-device row filled or deferred with a reason; no
file written on the device other than whatever `SetRemoteAccessCfg` itself persists (to be
determined).

Verification:

```sh
# on the device shell, spot-check the record
cat /proc/mtd
df -h
ps w | grep -E "atfwd|goahead|QCMAP"
# on the host
nmap -Pn -p 24,8081 192.168.1.1      # after closing the shell and rebooting
```

### Research (REA/Ghidra/web)

- Upstream `installer/src/orbic_network.rs` and `util.rs`: the exact requests `util orbic-shell`
  sends and how the listener is started.
- Discussion #528 (web RCE write-up) and DagShell's use of the same injection
  ([ref-docs/orbic-firmware-v1.2.0.md](../ref-docs/orbic-firmware-v1.2.0.md) section 6).

## Sprint 1.3: Backup and verification

- [ ] T-OR-015: stream every `/proc/mtd` partition to the host (`dd` or `nanddump` piped to
      `busybox nc`); SHA-256 on device and host; `MANIFEST.sha256` and `device-hashes.txt`
- [ ] T-OR-016: `rootfs.tar` of `etc bin sbin usr lib data usrdata`; byte copies with hashes of
      `/etc/init.d/misc-daemon`, all of `/etc/init.d`, `/usrdata/mode.cfg`,
      `/usrdata/mode_tmp.cfg`, `/data/usb/boot_hsusb_composition`, `wlan_conf_6174.xml`,
      `mobileap_cfg.xml`
- [ ] T-OR-017: second pass into `pass2/`; compare; explain every difference
- [ ] T-OR-017: encrypted off-repo copy on separate media; decrypt round-trip hash checked
- [ ] T-OR-017: all items of the BACKUP-AND-RECOVERY.md step 6 gate checked

Acceptance criteria: the step 6 gate passes in full. This is the gate for every later device
write in the project.

Verification:

```sh
cd backups/<date>-V1.2.0 && sha256sum -c MANIFEST.sha256
diff <(cut -d' ' -f1 MANIFEST.sha256) <(cut -d' ' -f1 device-hashes.txt)
diff MANIFEST.sha256 pass2/MANIFEST.sha256
tar -tf rootfs.tar > /dev/null && tar -tf rootfs.tar | grep -x 'etc/init.d/misc-daemon'
age -d /media/<offline-disk>/orbic-<date>.tar.age | sha256sum
```

### Research (REA/Ghidra/web)

- `ubi_reader` / `binwalk` on the host against the `system` and `usrdata` images to confirm they
  unpack and match `rootfs.tar` (cross-check of backup completeness).
- REA `extract_firmware` and `inspect_firmware_regions` on the `boot` and `aboot` images
  (read-only triage only; findings to `re/notes/`).

## Sprint 1.4: OTA block, SIM, 0x11EB

- [ ] T-OR-018: identify the FOTA client (process, init link, config path, update hosts)
- [ ] T-OR-018: block it reversibly (stop, rename the init link; record original name, path,
      hash); DNS block of update hosts if identified
- [ ] T-OR-018: verify across a reboot without the SIM, then with the active SIM inserted
- [ ] T-OR-019: run the 0x11EB procedure in [docs/TESTING.md](../docs/TESTING.md) with the
      upstream release daemon run manually (not installed persistently), then remove it
- [ ] T-OR-019: record the result in DEVICE-PROFILE.md; delete the capture
- [ ] T-OR-020: `re/notes/` entries for `goahead` endpoints, the FOTA client and the stock vs
      upstream `misc-daemon` diff

Acceptance criteria: FOTA client not running after reboot with the active SIM; firmware string
unchanged; 0x11EB answer recorded with evidence; RE notes follow the RE-METHODOLOGY template.

Verification:

```sh
# device shell
ps w | grep -i -E "fota|dm|ota"
ls -l /etc/rc*.d | grep -i -E "fota|dm|ota"
# host
diff backups/<date>-V1.2.0/files/misc-daemon upstream/dist/scripts/misc-daemon
grep -c -a 'orx-marker-<random>' <capture>.qmdl
```

### Research (REA/Ghidra/web)

- Ghidra (ARM v7 LE) on the backed-up `goahead`: `/goform/*` and `/action/*` handler table,
  `SetRemoteAccessCfg` handler (what it executes, what it writes to `/usrdata`), firmware-update
  and reboot endpoints.
- Ghidra/REA on the FOTA client binary (candidate name `dmclient`, #163): server hostnames,
  schedule, trigger conditions, package verification.
- REA `search_strings` across the `system` image for update URLs and `fota` paths.
- Upstream `lib/src/diag_device.rs` (0x11EB subscription) and `lib/src/gsmtap/parser.rs`: whether
  0x11EB packets are filtered before QMDL storage.
