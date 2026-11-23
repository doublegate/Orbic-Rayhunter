# Device Profile: Orbic RC400L, ORB400L_V1.2.0_BVZRT

Specification of the target unit. Two kinds of rows exist: **measured** (observed on our unit,
with the method) and **reported** (from the research corpus, with the source file). Anything not
yet observed on our unit is marked "TBD (on-device)" and is filled by T-OR-012 and T-OR-014
([ROADMAP.md](../ROADMAP.md)).

Recording rules for this file:

- Paste the exact command and the date with each new measurement.
- Redact IMEI, IMSI, ICCID, MAC addresses, Wi-Fi keys, admin passwords and serial numbers
  before committing ([RE-METHODOLOGY.md](RE-METHODOLOGY.md), [AGENTS.md](../AGENTS.md)).
- Full raw outputs stay in `backups/` (gitignored), not here.

## Measured facts

| Item | Value | Method | Date |
|---|---|---|---|
| Firmware string | `ORB400L_V1.2.0_BVZRT` | `/etc/version` on the device (T-OR-014), matching `GetDeviceInfo.softwareVersion`; `/etc/ver` does not exist | 2026-11-23 |
| USB ID (normal mode) | `05c6:f626`, product string "MDM9207-MTP _SN:<serial>", manufacturer "Android", `bcdUSB` 2.00, `bcdDevice` 3.18, 1 configuration, 500 mA | `lsusb -v -d 05c6:f626` | 2026-11-23 |
| USB serial | 8 hex characters; value redacted (device identifier). In normal mode there is **no** `serial` sysfs attribute: the serial is embedded in the product string after `_SN:` (so a product string is redacted too), and `orx doctor` reads it from there (T-OR-106) | `lsusb -v`, sysfs, `orx doctor` | 2026-11-23 |
| USB functions (normal mode) | two interfaces: 0 = class `e0`/`01`/`03` (Wireless, RF, RNDIS), "RNDIS Communications Control"; 1 = class `0a`/`00`/`00` (CDC Data), "RNDIS Ethernet Data" | `lsusb -v`, sysfs `bInterface*` | 2026-11-23 |
| Host address on the RNDIS link | DHCP from the device: `192.168.1.112` (2026-11-22), `192.168.1.134` (2026-11-23); default route via `192.168.1.1` | `ip -br addr`, `ip route` | 2026-11-23 |
| Stock web UI | `http://192.168.1.1/` reachable (TCP :80 open) | `orx doctor` | 2026-11-23 |
| Rayhunter | not installed (TCP :8080 closed) | `orx doctor` | 2026-11-23 |
| Web server | GoAhead (session cookie `-goahead-session-`; no `Server` header). `/` answers `302` to `/common/home.html` (`Last-Modified: Fri Nov 20 03:47:42 2020`); every response carries `X-Frame-Options: SAMEORIGIN` | `curl -sI` / `curl -D` | 2026-11-23 |
| Unauthenticated login probe | `GET /goform/GetLoginInfo` returns `retcode`, `loginStatus` (0 = logged out), `loginUser`, `loginRole` and an 11-character `priKey`, and sets a session cookie, all without authentication. The `priKey` value is not recorded | `curl` | 2026-11-23 |
| TLS on :443 | self-signed certificate with the OpenSSL default subject (`O=Internet Widgits Pty Ltd`), **expired** (valid 2017-08-30 to 2020-08-29), 1024-bit RSA key; TLSv1.2 `ECDHE-RSA-AES256-GCM-SHA384` | `openssl s_client` | 2026-11-23 |
| Listening ports (host side, TCP 1-1024, 5555, 8080, 8081) | open: 53 (DNS), 80, 443. Closed: 24 and 8081 (as expected before T-OR-013), 8080, all others to 1024. **5555 filtered** (ADB over TCP dropped, not refused) | `nmap -sT -Pn` | 2026-11-23 |
| OTA client trigger in the stock UI | the UI header calls `/action/FotaGetVersionInfo` on page load (`getNewVersionInfo()` in `js/common.js`), so **opening the stock UI in a browser asks the device to check for an update**. Not called by the recon, which used `curl` (no JavaScript). See T-OR-018 | UI JavaScript fetched with `curl` | 2026-11-23 |

`orx doctor` (T-OR-003) classifies `05c6:f626` as normal, `05c6:f601` as debug (ADB + DIAG + AT)
and `05c6:9008` as EDL. It does not yet classify `05c6:f622` (RNDIS plus serial, reachable via
`AT+SER=9,1`, [ref-docs/orbic-rc400l-hardware.md](../ref-docs/orbic-rc400l-hardware.md)
section 3); that is T-OR-032.

## Reported facts (not yet confirmed on our unit)

| Item | Reported value | Source |
|---|---|---|
| SoC / CPU | Qualcomm MDM9207 (device tree `qcom,mdm9607-mtp`), 1 x Cortex-A7 ARMv7 | [orbic-rc400l-hardware.md](../ref-docs/orbic-rc400l-hardware.md) section 1 (discussion #741) |
| Kernel | Linux 3.18.48 PREEMPT, built 2020-10-08, hostname `mdm9607` (firmware of that unit unknown) | [orbic-firmware-v1.2.0.md](../ref-docs/orbic-firmware-v1.2.0.md) section 3 |
| RAM | size unknown; 2-4 MB free reported with Rayhunter plus Tailscale | [orbic-rc400l-hardware.md](../ref-docs/orbic-rc400l-hardware.md) section 1 |
| Flash | raw NAND with UBI, no GPT; partitions `sbl, mibib, efs2, dynamic_nv, efs2_bak, tz, rpm, aboot, boot, scrub, modem, misc, recovery, usrdata, recoveryfs, sec, system`; `/proc/mtd` index 7 = `aboot` | [orbic-firmware-v1.2.0.md](../ref-docs/orbic-firmware-v1.2.0.md) section 4 |
| Root filesystem | UBI volume `/dev/ubi0_0` at `/`; upstream installer remounts it rw | [orbic-firmware-v1.2.0.md](../ref-docs/orbic-firmware-v1.2.0.md) section 3 |
| Init | SysV `/etc/init.d` with `/etc/rc5.d/S..` links; `misc-daemon` starts `atfwd_daemon` and others | [orbic-firmware-v1.2.0.md](../ref-docs/orbic-firmware-v1.2.0.md) sections 3, 5 |
| Web admin | GoAhead on 80/443; `/goform/GetLoginInfo`, `/goform/login`, `/action/SetRemoteAccessCfg` | [orbic-firmware-v1.2.0.md](../ref-docs/orbic-firmware-v1.2.0.md) section 5 |
| Admin password | Verizon convention: the 2.4 GHz Wi-Fi password | [rayhunter-install-orbic.md](../ref-docs/rayhunter-install-orbic.md) section 3.1 |
| Battery | removable BTE-3003, 3000 mAh, 11.1 Wh; telemetry in `/sys/kernel/chg_info/level` (5 coarse buckets) | [orbic-rc400l-hardware.md](../ref-docs/orbic-rc400l-hardware.md) section 1, [rayhunter-upstream-overview.md](../ref-docs/rayhunter-upstream-overview.md) section 3.1 |
| Display / input | `/dev/fb0` 128x128 RGB565; power key `/dev/input/event0` | [rayhunter-upstream-overview.md](../ref-docs/rayhunter-upstream-overview.md) section 8 |
| LTE bands (FCC grant) | B2, B4, B5, B13, B66 (B12/B48 claimed by upstream docs, not in the FCC grant) | [orbic-rc400l-hardware.md](../ref-docs/orbic-rc400l-hardware.md) section 2 |
| USB modes | f626 normal; f601 ADB + DIAG + AT (no RNDIS) via vendor request 0xA0; f622 RNDIS + serial; 9008 EDL (hardware test point only) | [orbic-rc400l-hardware.md](../ref-docs/orbic-rc400l-hardware.md) sections 3, 4 |
| USB mode files | `/usrdata/mode.cfg`, `/usrdata/mode_tmp.cfg`, `/data/usb/boot_hsusb_composition` | [orbic-rc400l-hardware.md](../ref-docs/orbic-rc400l-hardware.md) section 3 |
| Recovery | no software path into EDL; no confirmed firehose loader | [orbic-rc400l-hardware.md](../ref-docs/orbic-rc400l-hardware.md) section 4 |

## Measured on-device (T-OR-013 / T-OR-014, 2026-11-23)

Read from the root shell opened per [BACKUP-AND-RECOVERY.md](BACKUP-AND-RECOVERY.md) step 1.
Raw outputs are in the gitignored `backups/2026-11-23-V1.2.0/layout/`; identifiers (serial,
IMEI, ICCID, MDN, MEID, MAC) are redacted here.

- **Kernel:** `Linux mdm9607 3.18.48 #1 PREEMPT Fri Nov 20 11:38:41 CST 2020 armv7l GNU/Linux`.
  Command line: `root=ubi0:rootfs ubi.mtd=16 rootfstype=ubifs rootflags=bulk_read`,
  `androidboot.hardware=qcom`, `androidboot.baseband=msm`,
  `androidboot.poweronreason=Reset_PON_CHG`, `androidboot.authorized_kernel=true`,
  `console=ttyHSL0,115200`, `androidboot.serialno=<redacted>`.
- **CPU:** single ARMv7 core; `scaling_available_frequencies` 400000..1305600 kHz (7 steps).
- **RAM:** `/proc/meminfo` (see raw); userspace tmpfs budgets ~78.6 MiB each.
- **Partitions (`/proc/mtd`, erase size 0x40000 throughout):** 17 MTD partitions `mtd0..mtd16`:
  `sbl, mibib, efs2, dynamic_nv, efs2_bak, tz, rpm, aboot, boot, scrub, modem, misc, recovery,
  usrdata, recoveryfs, sec, system`. `system` (`mtd16`, 0x16d80000 ~365 MiB) is the largest;
  `modem` is `mtd10` (~56 MiB).
- **UBI:** `ubi0` attaches `mtd16` (`ubi.mtd=16`) with volumes `rootfs` (74.1 MiB), `usrfs`
  (232.3 MiB), `cachefs` (37.1 MiB); `ubi1` has `modem` (43.4 MiB); `ubi3` has `usrdata`
  (13.3 MiB). LEB size 253952 bytes, min I/O 4096.
- **Mounts / free space:** `/` is `ubi0:rootfs` ubifs rw (66.0M, 74% used); `/data` =
  `ubi0:usrfs` (214.7M, 6%); `/firmware` = `/dev/ubi1_0` (37.1M, 81%); `/usrdata` =
  `/dev/ubi3_0` (8.9M, 3%); `/cache` = `ubi0:cachefs`. `/tmp`, `/run`, `/var/volatile`,
  `/media/ram` are tmpfs.
- **Backup tools present** (standalone in `/bin`/`/sbin`/`/usr/bin` and via busybox):
  `nanddump`, `nandwrite`, `flash_erase`, `ubinfo`, `ubiupdatevol`, `md5sum`, `sha256sum`,
  `dd`, `tar`, `gzip`, `nc`. **Do not use `nanddump`/`dd` on `/dev/mtd*`** -- they deadlock the
  NAND (see the raw-NAND note below); Sprint 1.3 backs up with `tar` of the mounted filesystems.
- **Listening sockets (`netstat -tlnp`):** `dnsmasq` on 53 (loopback + `192.168.1.1`);
  `goahead` on 80 and 443; **`busybox` on 24** -- the root shell this recon opened (pid changes
  per run). No other listeners.
- **Processes of interest:** `goahead` (web admin), `atfwd_daemon`, `QCMAP_ConnectionManager`
  (`/usrdata/data/qcmap/mobileap_cfg.xml`), `dnsmasq`, `diagrebootapp`. **No persistent FOTA/OTA
  daemon** was found: the update check is driven by the web UI's `FotaGetVersionInfo` call, not a
  background service (bears on T-OR-018).
- **Stock `misc-daemon` SHA-256** (baseline for the T-OR-020 stock-vs-modified diff):
  `8f9bb4ce79eb878af49b366af2f784a44127736a62cce7992e4ab1c8fe3f1cf9`.
- **Remote-access config change from the exploit:** `GetRemoteAccessCfg` read `telnetd: 1`
  before and `telnetd: 0` after (`sshd: 1`, `telnetStatus: 0` unchanged). This is the one
  persisted device change from opening the shell, and it **survives a reboot** (the config is
  persistent; measured 2026-11-23). The port-24 listener itself is a backgrounded `busybox nc`,
  not a service, and **does not survive a reboot**: `nmap -Pn -p 24 192.168.1.1` shows it closed
  after boot, so the exploit must be re-run to reopen it (measured 2026-11-23, T-OR-013).
- **Raw NAND reads deadlock the device.** `nanddump`/`dd` of `/dev/mtd*` reads the first partition
  fine, then the second hangs: raw MTD access contends with the live UBI backing the mounted
  filesystems and wedges the NAND into unkillable D-state reads that a `kill -9` cannot clear,
  needing a power-cycle (reproduced three times, 2026-11-23). The pre-write backup therefore uses
  `tar` of the mounted `ubifs` volumes through the VFS, which cannot wedge (see
  [BACKUP-AND-RECOVERY.md](BACKUP-AND-RECOVERY.md)). A raw partition image is deferred to the
  aggressive tier (T-OR-098, EDL).

## TBD (on-device) checklist

Every command below is read-only. Host-side commands need no device access beyond USB
enumeration. On-device commands run in the network shell from `installer util orbic-shell`
(T-OR-013, see the caveat in [BACKUP-AND-RECOVERY.md](BACKUP-AND-RECOVERY.md) step 1). That shell
may not print a prompt; run one command at a time and wait for output. Never run an unqualified
`adb` command: the host can have other ADB devices attached, so always use `adb -s <serial>`.

### Host side (T-OR-012)

| Fact | Command | Status |
|---|---|---|
| Full USB descriptors, normal mode | `lsusb -v -d 05c6:f626` | measured 2026-11-23 |
| Interface classes per function | `for i in /sys/bus/usb/devices/*:1.*; do echo "$i $(cat $i/bInterfaceClass $i/bInterfaceSubClass $i/bInterfaceProtocol 2>/dev/null)"; done` | measured 2026-11-23 |
| Doctor transcript | `cargo run -p orx-cli -- doctor` | measured 2026-11-23 (transcript below, serial redacted) |
| Host link | `ip -br addr` and `ip route` | measured 2026-11-23 |
| Stock UI headers | `curl -sI http://192.168.1.1/` | measured 2026-11-23 |
| Listening ports from the host side | `nmap -Pn -p 1-1024,5555,8080,8081 192.168.1.1` (ports 24 and 8081 should be closed before T-OR-013) | measured 2026-11-23 (`-sT`, no root) |

`orx doctor` on the unit, 2026-11-23 (serial redacted):

```text
usb:        1-1 05c6:f626 MDM9207-MTP serial=<redacted> mode=normal (RNDIS + CDC data, stock composition)
stock ui:   192.168.1.1:80 reachable
rayhunter:  192.168.1.1:8080 not running / not installed
```

### Identity and kernel (T-OR-014)

| Fact | Command | Status |
|---|---|---|
| Kernel | `uname -a; cat /proc/version` | measured 2026-11-23 |
| Boot arguments | `cat /proc/cmdline` (expect `androidboot.poweronreason=`) | measured 2026-11-23 |
| CPU | `cat /proc/cpuinfo` | measured 2026-11-23 |
| SoC | `cat /sys/devices/soc0/machine /sys/devices/soc0/soc_id /sys/devices/soc0/build_id` | measured 2026-11-23 |
| CPU frequencies | `cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_available_frequencies /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor` | measured 2026-11-23 |
| Firmware version location | `ls -l /etc/version /etc/ver 2>&1; cat /etc/version /etc/ver 2>&1` | measured 2026-11-23 |
| Tool availability | `busybox --list; command -v nanddump nandwrite flash_erase ubinfo ubiupdatevol sha256sum md5sum nc tar gzip` | measured 2026-11-23 |

### Memory and storage (T-OR-014)

| Fact | Command | Status |
|---|---|---|
| RAM | `cat /proc/meminfo` | measured 2026-11-23 |
| MTD table | `cat /proc/mtd` | measured 2026-11-23 |
| MTD and UBI nodes | `ls -l /dev/mtd* /dev/mtdblock* /dev/ubi*` | measured 2026-11-23 |
| UBI volumes | `ubinfo -a` (if present) | measured 2026-11-23 |
| NAND geometry | `dmesg \| grep -i -E "nand\|ubi\|mtd"` | measured 2026-11-23: 17 partitions, erase size 0x40000 (256 KiB) per `/proc/mtd`; rootfs `ubi0` on mtd16 (`ubi.mtd=16` in cmdline) |
| Mounts | `cat /proc/mounts` | measured 2026-11-23 |
| Free space | `df -h` (record `/`, `/data`, `/usrdata`, `/tmp`) | measured 2026-11-23 |
| Map mtd index to partition name | derived from `/proc/mtd` names | measured 2026-11-23 |

### Processes, services, network (T-OR-014)

| Fact | Command | Status |
|---|---|---|
| Processes and owners | `ps w` (busybox) or `ps -ef` | measured 2026-11-23 |
| Listening sockets | `netstat -tlnup` (or `netstat -tln` if `-p` is unsupported) | measured 2026-11-23 |
| Init layout | `ls -l /etc/init.d /etc/rc*.d` | measured 2026-11-23 |
| Stock `misc-daemon` hash | `sha256sum /etc/init.d/misc-daemon` | measured 2026-11-23 |
| FOTA/OTA client | `ps w \| grep -i -E "fota\|dm\|ota\|update"; ls /etc/init.d` + RE (T-OR-020) | measured 2026-11-23: no persistent FOTA/OTA daemon and no matching `/etc/init.d` link. RE of the backed-up firmware ([re/notes/ota-mechanism.md](../re/notes/ota-mechanism.md)): auto-FOTA is **off** (`/etc/xml/fota/fota_cfg.xml` autocheck/autodown/autoinstall = 0, server URLs empty); the on-demand updater is `/usr/bin/oma_dm` + `dmclient` + `mdm-daemon`; triggers are the stock UI's `goahead` `/goform/Fota*` actions and a carrier-pushed OMA-DM session (needs SIM). `orbic-ui-read.py` refuses the `Fota*` read endpoints |
| QCMAP config present | `ls -l /usrdata/data/qcmap/` (do not print `mobileap_cfg.xml` into this file) | measured 2026-11-23 |
| USB mode | `cat /usrdata/mode.cfg /usrdata/mode_tmp.cfg /data/usb/boot_hsusb_composition 2>&1` | measured 2026-11-23 |
| AT forwarder | `ps w \| grep atfwd_daemon` | measured 2026-11-23: `atfwd_daemon` running (started by `misc-daemon`) |

### DIAG and peripherals (T-OR-014, T-OR-019)

| Fact | Command | Status |
|---|---|---|
| DIAG node | `ls -l /dev/diag` | TBD (on-device) |
| Other DIAG consumers | `ls -l /proc/*/fd 2>/dev/null \| grep diag` | TBD (on-device) |
| Framebuffer | `cat /sys/class/graphics/fb0/virtual_size /sys/class/graphics/fb0/bits_per_pixel` | TBD (on-device) |
| Battery | `cat /sys/kernel/chg_info/level /sys/kernel/chg_info/chg_en` | TBD (on-device) |
| Wi-Fi chip | `lsmod; dmesg \| grep -i -E "wlan\|qca\|ath"` | TBD (on-device) |
| Input device | `cat /proc/bus/input/devices` | TBD (on-device) |
| 0x11EB user traffic in QMDL | procedure in [TESTING.md](TESTING.md) (on-device privacy check) | TBD (on-device) |
| DIAG ioctl convention, log ID ranges, `ext_header_version` of 0xB0C0 | from the Rayhunter log and QMDL once running ([qualcomm-diag-qmdl.md](../ref-docs/qualcomm-diag-qmdl.md) section 9) | TBD (on-device) |

### Firmware behavior questions

| Question | How to answer | Status |
|---|---|---|
| Does `SetRemoteAccessCfg` persist anything to flash? | diff `/usrdata` file listing and hashes before and after T-OR-013 (`find /usrdata -type f -exec md5sum {} +`) | TBD (on-device) |
| Does the port 24 listener survive a reboot? | after reboot, `nmap -Pn -p 24 192.168.1.1` from the host | measured 2026-11-23: no -- closed after boot; re-run the exploit to reopen. (`telnetd: 0` in the config does persist.) |
| Does the stock `misc-daemon` match upstream's replacement? | `diff` the backed-up stock file against `upstream/dist/scripts/misc-daemon` on the host | TBD (on-device) |
| Is `mount -o remount,rw /dev/ubi0_0 /` harmless here (#693)? | observe during T-OR-022 install log | TBD (on-device) |
| Does the network installer work on V1.2.0 over the RNDIS link (no Wi-Fi)? | T-OR-022 | TBD (on-device) |
| UE capability (RATs and bands actually supported) | `UECapabilityInformation` in a recording ([lte-imsi-catcher-detection.md](../ref-docs/lte-imsi-catcher-detection.md) section 1.3) | TBD (on-device) |
