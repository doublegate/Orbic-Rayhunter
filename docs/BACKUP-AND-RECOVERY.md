# Backup and Recovery

There is no software unbrick path for the RC400L. EDL (`05c6:9008`) is reachable only through a
hardware test point, and no working firehose programmer is known for this device
([ref-docs/orbic-rc400l-hardware.md](../ref-docs/orbic-rc400l-hardware.md) section 4). A
verified, pre-write backup of every flash partition is therefore the only recovery asset this
project can count on. This file is the procedure behind tickets T-OR-015, T-OR-016, T-OR-017
and T-OR-018 ([ROADMAP.md](../ROADMAP.md)); `orx backup` (T-OR-034) automates it from v0.6.0.

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

A root shell is required to read the flash, so the first device action cannot be preceded by a
backup. Use the least invasive path: upstream's network shell, which authenticates to the stock
web admin and starts a temporary `nc` listener on port 24 through `/action/SetRemoteAccessCfg`
([ref-docs/rayhunter-install-orbic.md](../ref-docs/rayhunter-install-orbic.md) section 3.4).
Do not use `orbic-usb` here: it writes `/bin/rootshell` and changes the USB composition.

```sh
read -rs ORX_ADMIN_PW            # bash; fish: read -s ORX_ADMIN_PW
                                 # value: the 2.4 GHz Wi-Fi password on Verizon units
./installer util orbic-shell --admin-password "$ORX_ADMIN_PW"
unset ORX_ADMIN_PW
```

The password still appears in the host process list while the installer runs. Before running,
note the stock UI's remote-access settings page; afterwards check it again. Whether the request
persists anything to `/usrdata` is TBD (on-device) and is answered offline by RE of the
backed-up `goahead` binary (T-OR-020). Whether the port 24 listener survives a reboot is TBD
(on-device); check with `nmap -Pn -p 24 192.168.1.1` after the next reboot.

### 2. Record the layout

On the device, capture each output to the host (copy from the terminal, or stream with `nc` as
in step 3):

```sh
cat /proc/mtd
cat /proc/mounts
df -h
ls -l /dev/mtd* /dev/mtdblock* /dev/ubi*
ubinfo -a
dmesg | grep -i -E "nand|ubi|mtd"
command -v nanddump nandwrite flash_erase ubiupdatevol sha256sum md5sum tar
```

Save these as `layout/*.txt` in the backup directory. The partition list expected from prior
research is `sbl, mibib, efs2, dynamic_nv, efs2_bak, tz, rpm, aboot, boot, scrub, modem, misc,
recovery, usrdata, recoveryfs, sec, system`; the authoritative list is this unit's `/proc/mtd`.

### 3. Raw partition images (T-OR-015)

Nothing should be writing flash: Rayhunter is not installed, no host is connected to the Wi-Fi
AP, and no web admin session is open other than the shell. For each index `N` in `/proc/mtd`:

```sh
# host: listen and write the image
nc -l 9000 > backups/<date>-V1.2.0/mtdN-<name>.img        # OpenBSD netcat syntax

# device: stream the partition
dd if=/dev/mtdblockN bs=4096 | busybox nc 192.168.1.112 9000
```

If the device side does not exit at end of stream, add busybox's `-w` timeout option (which
`nc` options this busybox supports is TBD (on-device)). Preferred over `dd` when available:
`nanddump` to stdout, which can include OOB data; whether `nanddump` exists on V1.2.0 is
TBD (on-device). `mtdblock` reads return ECC-corrected page data without OOB: adequate for
archival, diffing and file recovery, not for a bit-exact NAND restore.

Hash both ends:

```sh
# device
sha256sum /dev/mtdblockN        # fall back to md5sum if sha256sum is absent
# host
sha256sum backups/<date>-V1.2.0/mtdN-<name>.img
```

Append both values to `MANIFEST.sha256` (host format) and `device-hashes.txt`.

### 4. File-level archives and stock files (T-OR-016)

UBI volumes are easier to inspect and to restore piecewise at file level:

```sh
# host
nc -l 9001 > backups/<date>-V1.2.0/rootfs.tar
# device (directories, not "/", so /proc, /sys, /dev and /tmp are not traversed)
cd / && tar -cf - etc bin sbin usr lib data usrdata | busybox nc 192.168.1.112 9001
```

Which `tar` options this busybox supports is TBD (on-device). Separately, take byte copies with
hashes of every file Rayhunter or this project may replace or touch:

| File | Why |
|---|---|
| `/etc/init.d/misc-daemon` | overwritten by upstream's installer; must be restorable byte for byte ([ref-docs/orbic-firmware-v1.2.0.md](../ref-docs/orbic-firmware-v1.2.0.md) section 5) |
| `/etc/init.d/*` and `/etc/rc*.d/` listing | boot order and the FOTA client's init link |
| `/usrdata/mode.cfg`, `/usrdata/mode_tmp.cfg`, `/data/usb/boot_hsusb_composition` | USB composition state |
| `/usrdata/data/usr/wlan/wlan_conf_6174.xml` | hotspot AP configuration (secret) |
| `/usrdata/data/qcmap/mobileap_cfg.xml` | QCMAP configuration incl. real APN (secret) |

Record `sha256sum` of each on the device and on the host.

### 5. Second pass

Repeat step 3 for every partition into a separate directory (`pass2/`) and compare manifests.
Partitions that are not mounted writable must be byte-identical across passes. A writable,
mounted UBI partition (`usrdata`, possibly `system`) may legitimately differ between passes; if it
does, record which and rely on the file-level archive for those contents. Which partitions are
stable is TBD (on-device).

### 6. Verification gate (T-OR-017)

The backup is verified only when all of these hold:

- [ ] Every `/proc/mtd` entry has an image whose host SHA-256 equals the device SHA-256.
- [ ] Image sizes equal the sizes in `/proc/mtd`.
- [ ] Pass 1 and pass 2 hashes match for every partition not mounted writable; differences are
      listed with the reason.
- [ ] `rootfs.tar` lists cleanly (`tar -tf rootfs.tar > /dev/null`) and contains
      `etc/init.d/misc-daemon` with the recorded hash.
- [ ] An encrypted copy exists on separate media (step 7) and its decrypted hash was checked.
- [ ] The restore-tool inventory from step 2 is recorded (which of `nandwrite`, `flash_erase`,
      `ubiupdatevol` exist on the device).

```sh
cd backups/<date>-V1.2.0 && sha256sum -c MANIFEST.sha256
```

### 7. Off-repo copy

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
reversible with any known tooling. Until the update mechanism is measured:

1. Never start an update from the device menu or the Verizon Windows tool.
2. Keep the active SIM out of the unit until the block below is verified. The OTA is reported to
   need Verizon network contact (#163).
3. Identify the client: `ps w`, `ls -l /etc/init.d /etc/rc*.d`, and RE of the candidate binary
   (T-OR-020). Process name, config path and update hosts are TBD (on-device); #163 names a
   `dmclient`.
4. Block, after the verified backup: stop the client and disable its init link (rename, not
   delete), recording the original name, path and hash so the change can be reversed exactly. If
   the update hosts are identified, a DNS block is an additional layer. Permanent suppression with
   a full reversal procedure is T-OR-096.
5. Verify: reboot with the SIM inserted; the client is not running; the firmware string is
   unchanged; repeat the check after several days.

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
