# Install

Procedure for v1.0.0: Orbic-Rayhunter's build of Rayhunter on an Orbic RC400L running
`ORB400L_V1.2.0_BVZRT`. Until v1.0.0 is released, use the upstream release and the
[official Orbic guide](https://efforg.github.io/rayhunter/orbic.html); this document is the
specification the `orx` subcommands implement (T-OR-032..T-OR-039, [ROADMAP.md](../ROADMAP.md)).
Where an `orx` subcommand does not exist yet, the manual equivalent is given and marked with the
ticket that automates it.

Read [ref-docs/legal-and-ethics.md](../ref-docs/legal-and-ethics.md) first. Rayhunter is
receive-only; captures contain your IMSI and location history. Outside the US, consult a local
attorney before running it.

## Overview

| Step | What | Writes to device? |
|---|---|---|
| 1 | Preflight with `orx doctor` | no |
| 2 | Backup gate | no (shell only) |
| 3 | Install upstream with the network installer (primary) | yes |
| 3b | USB installer fallback (warnings apply) | yes |
| 4 | `orx install` of our `dist/` build | yes |
| 5 | Verify | no |
| 6 | Uninstall / restore | yes |

## Requirements

- Linux host. The upstream installer also runs on macOS and Windows, but the bricked-device
  reports concentrate on Windows with the USB installer
  ([ref-docs/community-installs.md](../ref-docs/community-installs.md) section 3).
- The upstream release asset `rayhunter-v0.13.0-linux-x64.zip` and its `.sha256`, not the
  "Source code" archive ([ref-docs/rayhunter-install-orbic.md](../ref-docs/rayhunter-install-orbic.md)
  section 2).
- This project's release assets: `orx`, `rayhunter-daemon`, `rootshell`, `SHA256SUMS`, or a local
  build per [BUILD.md](BUILD.md).
- The web admin password. On Verizon units it is the 2.4 GHz Wi-Fi password shown in the device
  menu ([ref-docs/rayhunter-install-orbic.md](../ref-docs/rayhunter-install-orbic.md) section 3.1).
- A verified backup ([BACKUP-AND-RECOVERY.md](BACKUP-AND-RECOVERY.md)).
- OTA updates blocked ([BACKUP-AND-RECOVERY.md](BACKUP-AND-RECOVERY.md), OTA blocking).

Handle the admin password without leaving it in shell history:

```sh
read -rs ORX_ADMIN_PW     # bash; fish: read -s ORX_ADMIN_PW
```

## 1. Preflight

```sh
orx doctor                # or: cargo run -p orx-cli -- doctor
```

Expected on a stock unit (measured 2026-11-22, [DEVICE-PROFILE.md](DEVICE-PROFILE.md)):

```text
usb:        <port> 05c6:f626 MDM9207-MTP serial=<redacted> mode=normal ...
stock ui:   192.168.1.1:80 reachable
rayhunter:  192.168.1.1:8080 not running / not installed
```

Stop if the mode is not `normal`, if the stock UI is unreachable, or if the firmware string is not
`ORB400L_V1.2.0_BVZRT`. Other firmware builds are out of scope; record the string for any attempt.

Preflight checks that `orx install` enforces (T-OR-021, T-OR-036):

- [ ] Verified backup present: `orx verify --dir <backup-dir>` (T-OR-035) re-hashes every
      archive against `MANIFEST.txt` and requires rootfs `/` to be `ok`/`ok-live` with a matching
      SHA-256 (manual: `sha256sum -c` against the manifest hashes in the backup directory).
- [ ] Stock `/etc/init.d/misc-daemon` backed up byte for byte, and diffed against
      `upstream/dist/scripts/misc-daemon`. Upstream's replacement is a fixed copy of a stock script;
      any step present only in our stock file would be lost
      ([ref-docs/rayhunter-upstream-overview.md](../ref-docs/rayhunter-upstream-overview.md)
      section 3.2). Review the diff before continuing.
- [ ] Free space on `/data` recorded (`df -h /data` in the shell). Full `/data` has wedged the
      daemon and the hotspot (#1192).
- [ ] OTA block verified.
- [ ] Wi-Fi client mode will stay off (`wifi_enabled = false`): on V1.2.0 it left the AP and web
      UI unreachable in one report (#1033).

## 2. Backup gate

```sh
orx backup --out <dir>    # T-OR-034 (v0.6.0): archive every writable filesystem
orx verify --dir <dir>    # T-OR-035 (v0.7.0): re-hash the archives; the backup gate
```

Every write command (`orx install`/`uninstall`, when added) refuses to run unless
`verify::gate()` passes for the backup directory: a single rootfs `/` entry, state `ok`/`ok-live`,
and a `root.tar` whose size and SHA-256 match the manifest. `orx verify --dir` reports that gate
plus a per-filesystem result (`ok` / `MISSING` / `not a regular file` / `too small` / `size
mismatch` / `SHA-256 mismatch` / `no size=/sha256= in manifest` / `read error`), and exits
non-zero if any entry fails or the gate is not satisfied. Exit codes: `0` verified, `1`
verification/gate failure, `2` usage or an unreadable/malformed MANIFEST.txt.

`orx backup`/`orx verify` automate this from v0.6.0/v0.7.0; the equivalent manual procedure is
[BACKUP-AND-RECOVERY.md](BACKUP-AND-RECOVERY.md) steps 1-7. Do not continue until the
verification gate (step 4 there, or `orx verify --dir`) passes.

## 3. Install upstream with the network installer (primary)

The default upstream installer `installer orbic` is network-based: it logs in to the stock web
admin (`/goform/GetLoginInfo`, `/goform/login`), uses the command injection in
`/action/SetRemoteAccessCfg` to start a BusyBox `nc` shell on port 24, transfers files with
`nc` on port 8081 verified by `md5sum`, writes the files below and reboots. It never uses USB or
ADB and does not install `/bin/rootshell`
([ref-docs/rayhunter-install-orbic.md](../ref-docs/rayhunter-install-orbic.md) section 3).

```sh
sha256sum -c rayhunter-v0.13.0-linux-x64.zip.sha256
unzip rayhunter-v0.13.0-linux-x64.zip && cd rayhunter-v0.13.0-linux-x64
./installer orbic --admin-password "$ORX_ADMIN_PW"
```

Connect the host to the Orbic by its Wi-Fi AP or by USB tethering; `http://192.168.1.1` must
load the admin page. Whether the installer completes over the RNDIS link alone on Linux is TBD
(on-device); "No session cookie received" was reported with USB-only links on macOS and Windows
(#966, #955).

What it changes on the device:

| Path | Change |
|---|---|
| `/` (`/dev/ubi0_0`) | remounted read-write |
| `/data/rayhunter-data/` | created (real data directory) |
| `/data/rayhunter` | symlink to `/data/rayhunter-data` |
| `/data/rayhunter/rayhunter-daemon` | daemon binary |
| `/data/rayhunter/config.toml` | written from the template only if absent (`--reset-config` overwrites) |
| `/data/rayhunter/bin/` | `wpa_supplicant`, `wpa_cli`, `iw` if not already on the device |
| `/etc/init.d/rayhunter_daemon` | new init script |
| `/etc/init.d/misc-daemon` | **stock file overwritten** with upstream's copy (boot hook) |

No public report covers the current installer on V1.2.0 specifically; it worked on 1.2.6 and
1.3.0 reports, and the 1.2.8 failure at the telnet stage is still open (#901)
([ref-docs/community-installs.md](../ref-docs/community-installs.md) sections 2, 6). Known errors
and fixes:

| Symptom | Action |
|---|---|
| `Login failed with retcode: 201` | wrong admin password |
| `Failed to get login info ... tcp connect error` | host is not on the Orbic network |
| `connection closed before message completed` | retry; reboot the Orbic; try another Wi-Fi adapter (#643, #901) |
| `"exit code 0" not found` | partial prior install or the remount step returning 1 (#693); inspect over `util orbic-shell` before retrying |
| `background nc writer failed ... timed out` | transfer interrupted or low space on `/data` (#946, #955) |

Record the full installer output and the firmware string in the install log (T-OR-022).

## 3b. USB installer fallback (warnings)

Use `installer orbic-usb` only if the network installer is shown not to work on this unit and the
backup is verified. Upstream warns it can brick devices, especially on Windows.

- It sends a USB vendor request that reboots the device into `05c6:f601` (ADB + DIAG + AT,
  RNDIS off), pushes `rootshell` to `/bin/rootshell` and makes it setuid root through
  `AT+SYSCMD` handled by `atfwd_daemon`, then installs the same file set
  ([ref-docs/rayhunter-install-orbic.md](../ref-docs/rayhunter-install-orbic.md) section 4).
- It can leave USB tethering disabled; tethering state lives in `/usrdata/mode.cfg` (`9` restores
  RNDIS plus ADB, `3` disables) ([ref-docs/rayhunter-install-orbic.md](../ref-docs/rayhunter-install-orbic.md)
  section 6).
- Windows with Zadig-replaced drivers has bricked units (USB networking lost)
  ([ref-docs/orbic-firmware-v1.2.0.md](../ref-docs/orbic-firmware-v1.2.0.md) section 6). Use a
  Linux host and a known-good data cable.
- On V1.2.0 stock ADB may be absent (#228, #130).
- A setuid root shell is a standing local privilege-escalation path; see
  [SECURITY-MODEL.md](SECURITY-MODEL.md) and T-OR-083.

```sh
./installer orbic-usb
```

If this path was used, the uninstall in step 6 must also remove `/bin/rootshell` and restore
`/usrdata/mode.cfg` from the backup.

## 4. Install our build with `orx install`

```sh
orx install --dir <backup-dir>           # T-OR-036 (v0.8.0): DRY RUN -- prints the plan
orx install --dir <backup-dir> --apply   # execute: writes to the device and reboots
# --dist defaults to build/rayhunter/dist; --daemon to dist/rayhunter-daemon
```

`orx install` is **dry-run by default** (it prints the exact step plan and changes nothing) and
**refuses unless `verify::gate()` passes** for `--dir` (a verified rootfs backup -- the rollback).
With `--apply` it, in order:

1. `mkdir -p /data/rayhunter/scripts /data/rayhunter/bin`.
2. Deploys, each **atomically** (send to `<path>.orx-new`, which the device re-hashes to confirm
   the transfer, then `chmod` and `mv` into place -- so a dropped connection never leaves a
   half-written or non-executable file, and `mv`-over-a-running-binary avoids `ETXTBSY`):
   - `dist/rayhunter-daemon` -> `/data/rayhunter/rayhunter-daemon` (0755);
   - the rendered `config.toml` (from `config.toml.in`, with `device = "orbic"`) ->
     `/data/rayhunter/config.toml` (0644). Values and reasons: [CONFIGURATION.md](CONFIGURATION.md);
   - `rayhunter_daemon` -> `/etc/init.d/rayhunter_daemon` (0755);
   - the hooked `misc-daemon` -> `/etc/init.d/misc-daemon` (0755), last before reboot.
3. Reboots.

The daemon binary and both init scripts are validated before any write (daemon non-empty; init
scripts start with a `#!` shebang and have no CRLF). The backup -- not a `.prev` copy -- is the
rollback; `orx uninstall` (section 6) uses it. On a stock unit, run upstream's `installer orbic`
(step 3) first if you prefer, then `orx install --apply` to replace its build with ours.

## 5. Post-install health check

This is the ON-DEVICE health check after an install -- distinct from `orx verify --dir`, which
checks a backup's integrity (section 2). It is planned as `orx check` (T-OR-037) and not yet
implemented; for now run the manual equivalents.

```sh
orx check                 # T-OR-037 (planned); manual equivalents below
```

Checks (manual equivalents in parentheses):

- [ ] Daemon hash equals `dist/SHA256SUMS` (`sha256sum /data/rayhunter/rayhunter-daemon` in the
      shell).
- [ ] Web UI and API answer (`curl -s http://192.168.1.1:8080/api/system-stats`).
- [ ] A recording is active (`curl -s http://192.168.1.1:8080/api/qmdl-manifest`) and the display
      shows the status line (green with `ui_level = 1`).
- [ ] The build identifier from T-OR-028 is reported.
- [ ] After a reboot the daemon starts without intervention and a new recording appears.
- [ ] The stock hotspot still works: a client joins the AP and `http://192.168.1.1/` loads.

A green line alone does not prove capture: it also appears without an active SIM (#875). The full
smoke list is in [TESTING.md](TESTING.md).

## 6. Uninstall and restore

```sh
orx uninstall --dir <backup-dir>           # T-OR-039 (v0.8.0): DRY RUN
orx uninstall --dir <backup-dir> --apply   # execute
```

`orx uninstall` is **dry-run by default** and **backup-gated** like install. With `--apply` it
restores the **true stock** `/etc/init.d/misc-daemon` -- extracted from the verified backup's
`root.tar` (piped to `tar -xO -f -`), NOT copied from the device, because the device may already carry the
hooked script -- atomically (`<path>.orx-new` -> `chmod 755` -> `mv`), then removes
`/etc/init.d/rayhunter_daemon` and `/data/rayhunter/{rayhunter-daemon,config.toml}`, leaving the
`/data/rayhunter-data` recordings, and reboots.

Upstream has no automated uninstaller, and its manual routine leaves the real data directory and
the overwritten `misc-daemon` behind
([ref-docs/rayhunter-install-orbic.md](../ref-docs/rayhunter-install-orbic.md) section 6). Full
manual procedure:

1. Pull any recordings you want to keep (`orx pull-logs`, T-OR-038, or `/api/zip/{name}` from the
   web UI).
2. Open a shell: `./installer util orbic-shell --admin-password "$ORX_ADMIN_PW"`.
3. Stop the daemon: `/etc/init.d/rayhunter_daemon stop`.
4. Remount the root filesystem read-write: `mount -o remount,rw /dev/ubi0_0 /`.
5. Restore the stock `misc-daemon` from the backup: transfer the backed-up file to
   `/etc/init.d/misc-daemon.tmp` with a checksum check, confirm
   `sha256sum /etc/init.d/misc-daemon.tmp` equals the value in the backup manifest, then
   `mv /etc/init.d/misc-daemon.tmp /etc/init.d/misc-daemon && chmod 755 /etc/init.d/misc-daemon`.
6. Remove Rayhunter: `rm -rf /data/rayhunter /data/rayhunter-data /etc/init.d/rayhunter_daemon`.
7. USB-installer path only: `rm -f /bin/rootshell` and restore `/usrdata/mode.cfg` from the
   backup (upstream's instruction is `echo 3 > /usrdata/mode.cfg`; the backed-up value is
   authoritative).
8. Any OTA block or other change from this project stays in place unless you reverse it
   deliberately with its recorded procedure.
9. `reboot`.

Verify the restore:

- [ ] `sha256sum /etc/init.d/misc-daemon` equals the backup value.
- [ ] `ls /data/rayhunter /data/rayhunter-data /etc/init.d/rayhunter_daemon` reports all absent.
- [ ] `orx doctor` shows the stock UI reachable and Rayhunter not running.
- [ ] Hotspot, USB tethering and the device menu behave as before the install.

This rehearsal is T-OR-024 and is repeated for every release that changes the install path
([RELEASE-PROCESS.md](RELEASE-PROCESS.md)).
