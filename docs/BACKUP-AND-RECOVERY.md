# Backup and Recovery

There is no software unbrick path for the RC400L. EDL (`05c6:9008`) is reachable only through a
hardware test point, and no working firehose programmer is known for this device
([ref-docs/orbic-rc400l-hardware.md](../ref-docs/orbic-rc400l-hardware.md) section 4). A
verified, pre-write backup is therefore the only recovery asset this project can count on.

The backup is a `tar` archive of every writable filesystem, not a raw NAND image. Raw
`nanddump`/`dd` of `/dev/mtd*` DEADLOCKS this device: raw MTD access contends with the live UBI
that backs the mounted filesystems and wedges the NAND into unkillable D-state reads that need a
power-cycle (measured 2026-11-23: the first partition reads, the second hangs). `tar` reads
through the kernel VFS, the path the device uses constantly, so it cannot wedge -- and it is the
right backup for this project's writes, which only ever touch the mounted `ubifs` volumes (a
Rayhunter install adds a daemon under `/data` and an init link on `/`). A bit-exact image of the
static bootloader/modem regions would need EDL, which has no proven loader, and is deferred to
the aggressive tier (T-OR-098). This file is the procedure behind tickets T-OR-015, T-OR-016,
T-OR-017 and T-OR-018 ([ROADMAP.md](../ROADMAP.md)). `orx backup --out <dir> [--allow-live]`
(T-OR-034) automates it from v0.6.0: a native Rust port of `scripts/orbic-backup.py` on the
`orx` device transport (streaming `tar` + inline SHA-256), producing the same gate-compatible
`MANIFEST.txt`. Either tool works; the Python one remains the reference.

## Rules

1. No command that writes to the device runs before a backup exists and has passed the
   verification in step 6. This includes installing Rayhunter, editing init scripts, changing
   USB mode and OTA blocking.
2. Images are streamed off the device. `/tmp` is a RAM-backed filesystem on a device with very
   little free RAM ([ref-docs/orbic-rc400l-hardware.md](../ref-docs/orbic-rc400l-hardware.md)
   section 1), so nothing large is staged there.
3. Every image is hashed with SHA-256 on the device and on the host; a mismatch fails the backup.
4. Never write a raw image back with `dd` onto a UBI partition: it destroys UBI erase counters and
   bad-block handling ([ref-docs/orbic-firmware-v1.2.0.md](../ref-docs/orbic-firmware-v1.2.0.md)
   section 4.1).
5. Backups are never committed and never uploaded unencrypted. `backups/` is gitignored except
   its README.
6. The host may have other ADB devices attached. Any `adb` command names the Orbic explicitly
   (`adb -s <serial>`).

## Data classification

| Class | Contents | Handling |
|---|---|---|
| Secret | `efs2`, `efs2_bak`, `dynamic_nv`, `sec` and `usrdata` images; file archives of `/usrdata` and `/data`; `wlan_conf_6174.xml`, `mobileap_cfg.xml` | Contains IMEI, RF calibration and NV items, keys, Wi-Fi credentials, the admin password, carrier provisioning. Encrypted at rest off-host; never printed into docs, issues or logs |
| Proprietary | all other partition images (`sbl`, `aboot`, `boot`, `modem`, `system`, ...), extracted binaries | Not ours to redistribute ([adr/0002-gpl-licensing.md](adr/0002-gpl-licensing.md)); local and off-repo copies only |
| Project metadata | `/proc/mtd`, mount table, process list, hash manifests | May be summarized in [DEVICE-PROFILE.md](DEVICE-PROFILE.md) after redacting identifiers |

## Procedure

Variables used below: device `192.168.1.1`, host `192.168.1.112` (the measured RNDIS address,
[DEVICE-PROFILE.md](DEVICE-PROFILE.md); substitute yours), backup directory
`backups/<YYYYMMDD>-V1.2.0/`.

### 1. Obtain a shell (the one pre-backup action)

A root shell is required, so the first device action cannot be preceded by a backup. Use the
least invasive path: upstream's network shell, which authenticates to the stock web admin and
starts a temporary `busybox nc -ll -p 24 -e /bin/sh` listener through `/action/SetRemoteAccessCfg`
([ref-docs/rayhunter-install-orbic.md](../ref-docs/rayhunter-install-orbic.md) section 3.4). The
scripts use the non-interactive variant `orbic-start-telnet`, which only starts the listener; the
interactive `orbic-shell` is the same exploit plus a terminal. Do not use `orbic-usb` here: it
writes `/bin/rootshell` and changes the USB composition.

```sh
read -rs ORX_ADMIN_PW            # bash; fish: read -s ORX_ADMIN_PW
./installer util orbic-start-telnet --admin-password "$ORX_ADMIN_PW"   # or: orbic-shell
unset ORX_ADMIN_PW
```

The password appears in the host process list while the installer runs. The Python tools in
this repo (`orbic-shell.py`, `orbic-ui-read.py`, `orbic-backup.py`) avoid that by reading it from
a mode-600 file (`~/.claude-secrets/orbic-admin.txt`) rather than the command line; only the
upstream `installer` above takes it as an argument. Measured on this unit (2026-11-23): the port-24
listener is a backgrounded process and **does not survive a reboot** (`nmap -Pn -p 24
192.168.1.1` shows it closed after boot); re-run the command to reopen it. The exploit persists
exactly one config change, `GetRemoteAccessCfg.telnetd` 1 -> 0, which **does** survive a reboot.
Whether the request writes anything else to `/usrdata` is answered offline by RE of the
backed-up `goahead` binary (T-OR-020).

### 2. Record the layout

Capture the layout to `layout/*.txt` in the backup directory with the read-only driver (it
refuses any writer, so nothing touches flash). Use `--out-dir`, not a `>` redirect: the tool
creates each transcript mode 0600 (the recon captures `/proc/cmdline` with the device serial and
`/etc/passwd`), whereas a shell redirect would create a world-readable file:

```sh
mkdir -p backups/<date>-V1.2.0/layout
scripts/orbic-shell.py --script scripts/recon/phase1-inventory.sh --out-dir backups/<date>-V1.2.0/layout
scripts/orbic-shell.py --command 'cat /proc/mounts' --out-dir backups/<date>-V1.2.0/layout
```

The second invocation is a one-command run, so it writes `001-cat-proc-mounts.txt` (distinct
from the `phase1-inventory.sh` run, whose first file is `001-cat-proc-mtd.txt`); pass that file
to the backup tool's `--proc-mounts` in step 3. Its `$ ...` / `# exit=` transcript lines are not
mount entries, so the mount parser ignores them. This unit's writable `ubifs` volumes are `/`
(rootfs), `/data` (usrfs, the install target), `/usrdata`, `/cache`; `/firmware` is read-only
modem data. The 17 raw MTD partitions are recorded in `/proc/mtd` for reference, but are not
imaged (see the note on raw images below).

### 3. Back up and verify the writable filesystems (T-OR-015..T-OR-017)

Nothing should be writing the filesystems: Rayhunter is not installed, no host is on the Wi-Fi
AP, and no web admin session is open other than the shell. Run the backup tool, which archives
each mounted filesystem with `tar` on the device, streams it to the host over the **same port-24
shell channel** (the device firewall blocks a connect-back `nc` to a LAN host -- "No route to
host"), and verifies it:

```sh
scripts/orbic-backup.py \
  --proc-mounts backups/<date>-V1.2.0/layout/001-cat-proc-mounts.txt \
  --out-dir     backups/<date>-V1.2.0/fs \
  --allow-live  /data            # /data is live; its two passes may differ
```

`--allow-live /data` is required because `/data` is written while the device runs, so its two
passes are not byte-identical; every other filesystem (including read-only `/firmware` and the
quiescent rootfs) must match byte-for-byte or the tool FAILs it. For each filesystem the tool
streams the archive to a temp file while computing a rolling
SHA-256, takes a second independent pass, and promotes the archive to `<label>.tar` (mode 0600)
only when it is a complete, readable tar. For `/` it passes `tar --exclude` for every nested
mount so `/proc`, `/sys`, `/dev` and the other volumes are not traversed. A filesystem whose two
passes are byte-identical is marked `ok` (quiescent); a live filesystem such as `/data` whose
passes differ but which reads cleanly is marked `ok-live` (still a valid restore point). The
`MANIFEST.txt` records each filesystem, size, entry count, SHA-256 and verdict.

Separately confirm the stock files Rayhunter or this project may replace are inside the archives,
and record the baseline hash of the one the installer overwrites:

| File | Why |
|---|---|
| `/etc/init.d/misc-daemon` | overwritten by upstream's installer; must be restorable byte for byte. Baseline SHA-256 on this unit: `8f9bb4ce79eb878af49b366af2f784a44127736a62cce7992e4ab1c8fe3f1cf9` |
| `/etc/init.d/*` and `/etc/rc*.d/` | boot order and the FOTA/update client's init link (inside `root.tar`) |
| `/usrdata/mode.cfg`, `/usrdata/mode_tmp.cfg`, `/data/usb/boot_hsusb_composition` | USB composition state (inside `usrdata.tar` / `data.tar`) |
| `/usrdata/data/usr/wlan/wlan_conf_6174.xml` | hotspot AP configuration (secret; inside `usrdata.tar`) |
| `/usrdata/data/qcmap/mobileap_cfg.xml` | QCMAP configuration incl. real APN (secret; inside `usrdata.tar`) |

### 4. Verification gate (T-OR-017)

The backup is verified only when all of these hold:

- [ ] Every writable filesystem in `/proc/mounts` has a `<label>.tar` with verdict `ok` or
      `ok-live` in `MANIFEST.txt` (a truncated transfer reports `ABORT` and stops the run).
- [ ] Each archive reads cleanly on the host (`tar -tf <label>.tar > /dev/null`) with a non-zero
      entry count.
- [ ] `root.tar` contains `./etc/init.d/misc-daemon`, and its extracted copy matches the baseline
      hash above.
- [ ] Every `*.tar` is mode 0600 and under the gitignored `backups/` tree.
- [ ] An encrypted copy exists on separate media (step 5) and its decrypted hash was checked.

A bit-exact restore of a raw NAND partition is explicitly out of scope here (it would need the
unproven EDL path, T-OR-098); the file-level archive is what this project's writes can be rolled
back from.

**`orx verify --dir <dir>`** (T-OR-035, v0.7.0) automates this gate: it re-parses `MANIFEST.txt`,
re-hashes each `<label>.tar`, and reports `ok` / `MISSING` / `too small` (< 1 KiB) / `size
mismatch` / `SHA-256 mismatch` / `not a regular file` / `read error` / `no size=/sha256= in
manifest` per filesystem, failing if any
entry's archive does not verify or its recorded state is not `ok`/`ok-live`. The shared
`verify::gate()` -- the contract every write command must pass before touching the device --
requires **exactly one** rootfs `/` entry, state `ok`/`ok-live`, and a `root.tar` whose size and
64-hex SHA-256 match the manifest; a missing checksum, a wrong size, a duplicate `/` row, or a
bad state all fail closed. It is the Rust counterpart of `orbic-ota-block.py`'s `_backup_gate`.

### 5. Off-repo copy

Encrypt before the data leaves this machine, then verify the round trip:

```sh
tar -cf - -C backups <date>-V1.2.0 | age -p -o /media/<offline-disk>/orbic-<date>.tar.age
age -d /media/<offline-disk>/orbic-<date>.tar.age | sha256sum    # compare with:
tar -cf - -C backups <date>-V1.2.0 | sha256sum
```

Any authenticated encryption tool works (`gpg --symmetric` is an alternative); record which was
used. Keep the passphrase outside the repository and outside this machine's shell history. Do
not sync the backup to a cloud service in clear text.

## OTA blocking (T-OR-018)

Verizon publishes newer builds than ours, up to `ORB400L_V1.3.2_BVZRT` ("System Update 7",
2025-08-19); whether any of them closes the web-UI injection the network installer relies on is
unknown, and a forced 1.3.0 update was reported to remove root on one USB-era install
([ref-docs/orbic-firmware-v1.2.0.md](../ref-docs/orbic-firmware-v1.2.0.md) sections 1-2,
[ref-docs/community-installs.md](../ref-docs/community-installs.md) section 2). An OTA is not
reversible with any known tooling.

The mechanism is now measured from the backed-up firmware (T-OR-020, full detail in
[re/notes/ota-mechanism.md](../re/notes/ota-mechanism.md)): **auto-FOTA is already off** on this
unit (`/etc/xml/fota/fota_cfg.xml`: `autocheck`/`autodown`/`autoinstall` all `0`, server URLs
empty), there is **no FOTA daemon at boot**, and the updater (`/usr/bin/oma_dm`, `dmclient`,
`mdm-daemon`) runs only on demand. No static update hostname exists to DNS-block (the server is
provisioned dynamically via carrier OMA-DM). Two triggers remain: the stock UI's `goahead`
`/goform/Fota*` actions, and a carrier-pushed OMA-DM session (needs the SIM + Verizon
network, upstream #163). The block therefore is:

1. Never start an update from the device menu, the stock web UI (its page load calls
   `FotaGetVersionInfo`), or the Verizon Windows tool. `orbic-ui-read.py` refuses the `Fota*`
   endpoints.
2. Keep the active SIM out of the unit until the block is verified (no network -> no carrier push).
3. Preserve the `autocheck=0`/`autodown=0`/`autoinstall=0` policy; record `fota_cfg.xml`'s
   baseline SHA-256 and re-check it after any device work.
4. Block, after the verified backup: reversibly neutralize the on-demand updater -- rename
   `/usr/bin/oma_dm`, `/usr/bin/dmclient`, `/usr/bin/mdm-daemon` (or clear their exec bit),
   recording each original name, path and hash so the change reverses exactly (rename, not
   delete). Permanent suppression with a full reversal procedure is T-OR-096.
5. Verify: reboot with the SIM inserted; none of those updater processes run; `fota_cfg.xml` and
   the firmware string are unchanged; repeat the check after several days.

Rayhunter's own `auto_check_updates` only queries GitHub releases and is unrelated to Verizon OTA
([ref-docs/rayhunter-upstream-overview.md](../ref-docs/rayhunter-upstream-overview.md) section 6).

## What is and is not recoverable

| Failure | Recoverable? | Path |
|---|---|---|
| Rayhunter files in `/data/rayhunter*`, `/etc/init.d/rayhunter_daemon` | yes | network shell, `rm`, reboot ([INSTALL.md](INSTALL.md) uninstall) |
| Replaced `/etc/init.d/misc-daemon` | yes, while a shell works | push the backed-up stock file, compare SHA-256, reboot |
| `/data` full, daemon and AP down (#1192) | yes if any shell is reachable; the reported recovery used `/bin/rootshell` over ADB | `orx rescue` (T-OR-041); prevent with free-space thresholds and retention ([CONFIGURATION.md](CONFIGURATION.md)) |
| USB composition changed (`mode.cfg`) | yes, while a shell works | restore the backed-up `mode.cfg`, reboot |
| AP lost after enabling Wi-Fi client mode (#1033, reported on V1.2.0) | yes if ADB or the RNDIS link still reaches the device | `adb -s <serial> forward tcp:8080 tcp:8080`, disable client mode; keep `wifi_enabled = false` |
| Admin password forgotten | yes | hardware reset button under the back cover restores the default ([ref-docs/rayhunter-install-orbic.md](../ref-docs/rayhunter-install-orbic.md) section 3.1); its effect on installed Rayhunter files is TBD (on-device) |
| OTA to a newer firmware | no known downgrade | prevention only (OTA blocking above) |
| Corrupted UBI rootfs, `boot`, `aboot`, `modem` | not with proven tooling | only with a working shell and `ubiupdatevol` / `flash_erase` + `nandwrite`, or an EDL programmer that does not exist yet |
| Lost or corrupted `efs2` / `dynamic_nv` (IMEI, RF calibration) | no proven path | the backup preserves the data for analysis; no tested write-back procedure exists |
| Device does not boot at all | not today | EDL via test point (below) once a loader is proven (T-OR-098) |

Raw restores (`ubiupdatevol`, `flash_erase`, `nandwrite`) are not part of any procedure in this
project until they are rehearsed and documented under T-OR-098. The `dd` write onto
`/dev/mtdblock7` (`aboot`) seen in community posts is a known-dangerous operation and out of
scope ([ref-docs/orbic-firmware-v1.2.0.md](../ref-docs/orbic-firmware-v1.2.0.md) section 4.1).

## EDL test-point note

Reported, not verified by this project: lift the LCD; the "sense" test point under the top-left
corner of the LCD, grounded during reset, gives fastboot; tied to 1.8 V (about 1 kOhm suggested)
gives EDL, enumerating as `05c6:9008`. The Moxee's power+reset EDL entry is not confirmed on the
Orbic. Orbic Sahara identity: HWID `000480e100000000`; candidate loaders
(`prog_nand_firehose_9x07.mbn`, `NPRG9x07.mbn`, `ENPRG9x07.mbn`) hung on the Moxee and none is
confirmed for the Orbic ([ref-docs/orbic-rc400l-hardware.md](../ref-docs/orbic-rc400l-hardware.md)
section 4). Observing enumeration in EDL writes nothing; sending a loader is out of scope until
T-OR-098.
