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
| Firmware string | `ORB400L_V1.2.0_BVZRT` | read on the unit by the owner ([orbic-firmware-v1.2.0.md](../ref-docs/orbic-firmware-v1.2.0.md) section 1); storage location on the device TBD (on-device) | 2026-11-22 |
| USB ID (normal mode) | `05c6:f626`, product string "MDM9207-MTP _SN:<serial>", manufacturer "Android", `bcdUSB` 2.00, `bcdDevice` 3.18, 1 configuration, 500 mA | `lsusb -v -d 05c6:f626` | 2026-11-23 |
| USB serial | 8 hex characters; value redacted (device identifier). In normal mode there is **no** `serial` sysfs attribute: the serial is embedded in the product string after `_SN:` (so a product string is redacted too), and `orx doctor` reads it from there (T-OR-106) | `lsusb -v`, sysfs, `orx doctor` | 2026-11-23 |
| USB functions (normal mode) | two interfaces: 0 = class `e0`/`01`/`03` (Wireless, RF, RNDIS), "RNDIS Communications Control"; 1 = class `0a`/`00`/`00` (CDC Data), "RNDIS Ethernet Data" | `lsusb -v`, sysfs `bInterface*` | 2026-11-23 |
| Host address on the RNDIS link | DHCP from the device: `192.168.1.112` (2026-11-22), `192.168.1.134` (2026-11-23); default route via `192.168.1.1` | `ip -br addr`, `ip route` | 2026-11-23 |
| Stock web UI | `http://192.168.1.1/` reachable (TCP :80 open) | `orx doctor` | 2026-11-22 |
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
| Doctor transcript | `cargo run -p orx-cli -- doctor` | measured (see above) |
| Host link | `ip -br addr` and `ip route` | measured 2026-11-23 |
| Stock UI headers | `curl -sI http://192.168.1.1/` | measured 2026-11-23 |
| Listening ports from the host side | `nmap -Pn -p 1-1024,5555,8080,8081 192.168.1.1` (ports 24 and 8081 should be closed before T-OR-013) | measured 2026-11-23 (`-sT`, no root) |

### Identity and kernel (T-OR-014)

| Fact | Command | Status |
|---|---|---|
| Kernel | `uname -a; cat /proc/version` | TBD (on-device) |
| Boot arguments | `cat /proc/cmdline` (expect `androidboot.poweronreason=`) | TBD (on-device) |
| CPU | `cat /proc/cpuinfo` | TBD (on-device) |
| SoC | `cat /sys/devices/soc0/machine /sys/devices/soc0/soc_id /sys/devices/soc0/build_id` | TBD (on-device) |
| CPU frequencies | `cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_available_frequencies /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor` | TBD (on-device) |
| Firmware version location | `ls -l /etc/version /etc/ver 2>&1; cat /etc/version /etc/ver 2>&1` | TBD (on-device) |
| Tool availability | `busybox --list; command -v nanddump nandwrite flash_erase ubinfo ubiupdatevol sha256sum md5sum nc tar gzip` | TBD (on-device) |

### Memory and storage (T-OR-014)

| Fact | Command | Status |
|---|---|---|
| RAM | `cat /proc/meminfo` | TBD (on-device) |
| MTD table | `cat /proc/mtd` | TBD (on-device) |
| MTD and UBI nodes | `ls -l /dev/mtd* /dev/mtdblock* /dev/ubi*` | TBD (on-device) |
| UBI volumes | `ubinfo -a` (if present) | TBD (on-device) |
| NAND geometry | `dmesg \| grep -i -E "nand\|ubi\|mtd"` | TBD (on-device) |
| Mounts | `cat /proc/mounts` | TBD (on-device) |
| Free space | `df -h` (record `/`, `/data`, `/usrdata`, `/tmp`) | TBD (on-device) |
| Map mtd index to partition name | derived from `/proc/mtd` names | TBD (on-device) |

### Processes, services, network (T-OR-014)

| Fact | Command | Status |
|---|---|---|
| Processes and owners | `ps w` (busybox) or `ps -ef` | TBD (on-device) |
| Listening sockets | `netstat -tlnup` (or `netstat -tln` if `-p` is unsupported) | TBD (on-device) |
| Init layout | `ls -l /etc/init.d /etc/rc*.d` | TBD (on-device) |
| Stock `misc-daemon` hash | `sha256sum /etc/init.d/misc-daemon` | TBD (on-device) |
| FOTA/OTA client | `ps w \| grep -i -E "fota\|dm\|ota\|update"; ls /etc/init.d \| grep -i -E "fota\|dm\|ota"` | TBD (on-device) |
| QCMAP config present | `ls -l /usrdata/data/qcmap/` (do not print `mobileap_cfg.xml` into this file) | TBD (on-device) |
| USB mode | `cat /usrdata/mode.cfg /usrdata/mode_tmp.cfg /data/usb/boot_hsusb_composition 2>&1` | TBD (on-device) |
| AT forwarder | `ps w \| grep atfwd_daemon` | TBD (on-device) |

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
| Does the port 24 listener survive a reboot? | after reboot, `nmap -Pn -p 24 192.168.1.1` from the host | TBD (on-device) |
| Does the stock `misc-daemon` match upstream's replacement? | `diff` the backed-up stock file against `upstream/dist/scripts/misc-daemon` on the host | TBD (on-device) |
| Is `mount -o remount,rw /dev/ubi0_0 /` harmless here (#693)? | observe during T-OR-022 install log | TBD (on-device) |
| Does the network installer work on V1.2.0 over the RNDIS link (no Wi-Fi)? | T-OR-022 | TBD (on-device) |
| UE capability (RATs and bands actually supported) | `UECapabilityInformation` in a recording ([lte-imsi-catcher-detection.md](../ref-docs/lte-imsi-catcher-detection.md) section 1.3) | TBD (on-device) |
