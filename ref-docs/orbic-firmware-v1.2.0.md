# Orbic RC400L Firmware (V1.2.0 and Family)

- Retrieved: 2026-11-22
- Target: `ORB400L_V1.2.0_BVZRT` on an Orbic Speed RC400L (Verizon), MDM9207
- Convention: "unverified" marks claims we could not source. "TBD (on-device)" marks facts we will
  measure ourselves. No public source describes V1.2.0 specifically; most facts below come from
  V1.1.5/V1.2.x/V1.3.0 units and are assumed, not proven, to hold for V1.2.0.

---

## 1. Known firmware versions

| Version string | Date / channel | Source |
|---|---|---|
| `ORB400L_v1.1.5_BVZRT` | Early shipping firmware; Verizon says units on it may not update from the device menu and must use the Windows tool | Verizon Software Update Assistant page; discussions #682, #866 |
| `ORB400L_V1.2.0_BVZRT` | Our unit. Not listed on Verizon's update page; also reported by a Rayhunter user (#669) | Measured; discussion #669 |
| `ORB400L_V1.2.2_BVZRT` | Result of the Windows "RC400L Upgrade Tool" (`RC400L_Upgrade_Tool_Setup_20210624.exe`) | Verizon Software Update Assistant page |
| `ORB400L_V1.2.8.BVZRT` | Verizon "System Update 5", 2020-12-21 | Verizon update page |
| `ORB400L_V1.3.0_BVZRT` | Verizon "System Update 6", 2022-06-28 | Verizon update page; discussion #768; stathopulos/orbic |
| `ORB400L_V1.3.2_BVZRT` | Verizon "System Update 7", 2025-08-19 ("performance improvements") | Verizon update page |

Version ordering anomaly: V1.2.8 was published in 2020-12 while the 2021-06 Upgrade Tool lands on
V1.2.2; the tool is apparently a recovery path for V1.1.5 units rather than the newest build
(inference). One XDA user reports the tool only runs against a single expected source version
("v1.5.1" in the post, probably meaning 1.1.5) and he patched the check (unverified).

Rayhunter's maintainer is "not aware of any major differences between firmware versions" for
installation purposes (#682). Whether V1.3.2 (2025) closes the web-UI injection used by the network
installer is not documented anywhere we found (unverified; see section 6).

## 2. OTA behaviour

- Verizon delivers updates over the air: its update page asks the user to connect the device to
  Wi-Fi or a strong Verizon connection with a charged battery, and to use the Windows tool only if
  updating from the device menu fails. The update is therefore user-initiated from the device menu
  at minimum; whether it is also pushed or auto-installed silently is unverified.
- Update transport, server and package format are undocumented publicly (unverified).
- Planning implication: an OTA to V1.3.x would replace the rootfs and may remove our init scripts or
  binaries outside `/data`, and could patch the exploit we depend on. Until measured, keep the unit off
  Verizon data (no Verizon SIM, or block FOTA hosts) and never accept an update from the menu.
  FOTA client process name and config path are TBD (on-device).

## 3. Platform and OS

| Item | Value | Source |
|---|---|---|
| Kernel | Linux 3.18.48, `#1 PREEMPT Thu Oct 8 21:40:29 CST 2020`, armv7l, hostname `mdm9607` | discussion #741 (firmware version of that unit unknown); mjg59 ("3.18.48") |
| Base distribution | Qualcomm MDM9607 "LE" Linux (OpenEmbedded/Yocto-derived, as for other MDM9x07 products) | inference from Qualcomm MDM LE conventions and `/etc/init.d` + `rcN.d` layout; unverified |
| Shell | `bash-4.3` and busybox (`busybox nc` used by the Rayhunter installer, `start-stop-daemon`) | #741; Rayhunter `installer/src/orbic_network.rs`, `dist/scripts/rayhunter_daemon` |
| Init | SysV-style: `/etc/init.d/<name>` with `/etc/rc5.d/S..` symlinks | wsmith blog; Rayhunter installs `/etc/init.d/rayhunter_daemon` and replaces `/etc/init.d/misc-daemon` |
| Root filesystem | UBI volume `/dev/ubi0_0` mounted at `/`, normally read-only on some devices (installer runs `mount -o remount,rw /dev/ubi0_0 /`, "really only necessary for the Moxee") | Rayhunter `orbic_network.rs` |
| Writable data | `/data` (Rayhunter at `/data/rayhunter`), `/usrdata` (QCMAP config, USB mode files) | Rayhunter; mjg59; wsmith |
| Bootloader | Little Kernel based `aboot` at `/dev/mtdblock7`; fastboot present; signed with Qualcomm "General Use Test Key"; passes `androidboot.poweronreason=USB/PWRKEY/Hard_Reset` | mjg59 (both posts) |
| Users | ADB shell runs unprivileged (Rayhunter docs/wsmith: UID 2000); several system dirs owned by UID 1000 (stathopulos, V1.3.0) | wsmith; stathopulos/orbic |

## 4. Flash layout

Raw NAND, UBI, no GPT. Partition names (XDA, Renate, from EDL/flash tooling; order as given):

```text
sbl, mibib, efs2, dynamic_nv, efs2_bak, tz, rpm, aboot, boot, scrub, modem, misc,
recovery, usrdata, recoveryfs, sec, system
```

mjg59 confirms `/proc/mtd` index 7 is `aboot`. Sizes, erase-block size and UBI volume names are not
public. `efs2` / `efs2_bak` / `dynamic_nv` hold modem NV items including the IMEI and RF calibration;
`modem` is the baseband firmware; `system` and `usrdata` are UBI. Mapping of mtd index to name beyond
`aboot` = 7 is TBD (on-device).

### 4.1 Backup method (proposed; read-only, to be validated)

From a root shell, with Rayhunter stopped and nothing writing to flash:

1. Record `cat /proc/mtd`, `cat /proc/mounts`, `ubinfo -a` (if present), `dmesg | grep -i -E "nand|ubi"`.
2. For every mtd partition, read it whole. Preferred: `nanddump -f /tmp/... /dev/mtdN` (includes OOB
   options; whether `nanddump` exists on V1.2.0 is TBD). Fallback: `dd if=/dev/mtdblockN bs=...`
   (mtdblock returns ECC-corrected page data without OOB; adequate for archival and diffing, not
   for a bit-exact NAND restore).
3. Stream images off the device rather than to `/tmp` (tmpfs, limited RAM): e.g. `dd ... | busybox nc
   <host> <port>` over the RNDIS/Wi-Fi link, or `adb pull` when in ADB mode. Record SHA-256 on both
   ends.
4. Additionally archive UBI volume contents at file level (`tar` of `/`, `/data`, `/usrdata` as
   mounted) for easy inspection.
5. Treat `efs2`, `efs2_bak`, `dynamic_nv`, `sec` and `usrdata` images as sensitive (IMEI, keys,
   Wi-Fi credentials); keep them out of the repository.

Never write a raw image back with `dd` onto a UBI partition (it destroys erase counters and bad-block
handling); restoring UBI volumes uses `ubiupdatevol`, and restoring raw partitions needs `flash_erase`
plus `nandwrite`. The XDA thread shows `dd if=/tmp/aboot of=/dev/mtdblock7` being used to patch aboot;
that is a known-dangerous operation (mjg59 warns of bricking) and is out of scope for this project.

## 5. Key daemons and services

| Component | Role | Source |
|---|---|---|
| `atfwd_daemon` | Root daemon handling AT commands forwarded from the modem/USB AT port; implements vendor `AT+SYSCMD=<cmd>` (runs `<cmd>` as root, no auth), `AT+SER`, `AT+CHIPID`; logs to `data/logs/atfwd.log` | mjg59; Rayhunter `orbic.rs` (waits for `pgrep atfwd_daemon`) |
| `/etc/init.d/start_atfwd_daemon` | Starts it from `misc-daemon` | Rayhunter `dist/scripts/misc-daemon` |
| GoAhead web server (`goahead`) | Admin UI on 80/443; endpoints `/goform/GetLoginInfo`, `/goform/login`, `/action/SetRemoteAccessCfg`, `/action/SetPortMode`, `/common/home.html`, `/common/shortmessage.html`; session cookie `-goahead-session-` | discussion #528; Rayhunter `orbic_network.rs`; DagShell |
| QCMAP (Qualcomm Mobile AP) | Hotspot/router management; config `/usrdata/data/qcmap/mobileap_cfg.xml` (real APN lives here; web UI APN field is cosmetic) | wsmith blog. Process name `QCMAP_ConnectionManager` is the Qualcomm default (unverified here) |
| DNS (port 53) | Local resolver/DHCP; hostname `my.jetpack` seen via reverse DNS | discussion #768 nmap |
| ADB over TCP | Port 5555 shows "filtered" | discussion #768 |
| `reboot-daemon`, `init_qcom_audio`, `start_stop_qti_ppp_le`, `start_loc_launcher` | Started by `misc-daemon` | Rayhunter `dist/scripts/misc-daemon` (upstream's copy of the stock script, modified) |
| Modem AT channels | `/dev/smd7`, `/dev/smd8` (e.g. `AT+CCLK?`, `AT+CGDCONT?`) | wsmith; discussion #532; DagShell |
| `/dev/diag` | Qualcomm DIAG char device used by Rayhunter | Rayhunter; see `qualcomm-diag-qmdl.md` |

Note: Rayhunter overwrites `/etc/init.d/misc-daemon` with its own copy (that adds
`/etc/init.d/rayhunter_daemon start`). Back up the stock file before any install so it can be
restored byte-for-byte.

## 6. How root/ADB is obtained (prior research)

1. USB vendor request (mjg59, 2022; Rayhunter `orbic-usb`): send control transfer
   `bmRequestType 0xC0, bRequest 0xA0, wValue 0, wIndex 0` to `05c6:f626`. The device reboots as
   `05c6:f601` with ADB + DIAG + AT, RNDIS off. Over the AT interface, `AT+SYSCMD=` gives root
   command execution; Rayhunter uses it to install a setuid `/bin/rootshell`
   (`chown root` + `chmod 4755`). `AT+SER=9,1` restores RNDIS alongside ADB/DIAG/AT.
2. Web UI command injection (Rayhunter `orbic` network installer since 0.6.0/0.8.0; discussion #528):
   after authenticating (`/goform/GetLoginInfo` -> `/goform/login` with Orbic's MD5/base64/char-swap
   password encoding), `POST /action/SetRemoteAccessCfg` with
   `{"password": "\"; <cmd> #"}` injects a shell command as root. Rayhunter starts
   `busybox nc -ll -p 24 -e /bin/sh` (port 24 because some Kajeet units run a password-protected
   telnetd on 23). Requires the admin password (Verizon default: the Wi-Fi password). No CVE has been
   assigned as far as NVD keyword search shows.
3. Factory "Upgrade Tool" internals (mjg59): the Windows tool bundles adb + libusb-win32 + Qualcomm
   drivers, sends the 0xA0 request, writes a per-device RSA/AES "chipid" blob via `AT+SYSCMD`, then
   `AT+CHIPID` and `AT+SER=3,1`. The chipid blob comes from `http://pro.w.ifelman.com/api/encrypt`.

Known failure modes: Windows `orbic-usb` with a Zadig-replaced driver has bricked many units
(USB networking lost; Wi-Fi install still works); full `/data` causing "Retrieving Data Usage Info"
and blank menus has been suspected (#733, #745, #977); some units need several installer retries
("connection closed before message completed", #768, #791).

## 7. Prior research and writeups

- Matthew Garrett, "Poking a mobile hotspot" (2022-11-25) and follow-up (2022-12-06): USB mode switch,
  `atfwd_daemon` and `AT+SYSCMD`, chipid crypto, aboot autoboot patch, test-key signing.
- XDA "Resetting Verizon Orbic Speed RC400L / firmware flash" thread (2021-): EDL/fastboot test points,
  Sahara IDs, partition list, loader attempts.
- EFF Rayhunter installers and discussions (#528 web RCE, #741 on-target info, #768/#791 install
  failures, #866 firmware 1.1.5 issue, #993 Moxee EDL attempts).
- stathopulos/orbic: Rust utilities (`enable_adb`, `at_commands`, a UID-1000 setuid shell) tested on
  V1.3.0.
- DagShell (dagnazty): third-party "custom firmware" (web app deployed to `/data/orbic_app`) using the
  same SetRemoteAccessCfg injection and `/dev/smd8` AT access.
- wsmith.io, "Orbic and Tailscale" (2026-02): sysvinit layout, QCMAP APN, RAM pressure.

## 8. TBD (on-device) for V1.2.0

Each section is to be filled with the exact command, its full output (redacting IMEI/ICCID/MACs/keys)
and the date.

### 8.1 uname

- `uname -a`; `cat /proc/version`; `cat /proc/cmdline` (expect `androidboot.poweronreason=`).
- Compare with the 3.18.48 / 2020-10-08 build reported in #741.

### 8.2 /proc/mtd

- `cat /proc/mtd`; `ls -l /dev/mtd* /dev/mtdblock* /dev/ubi*`; `ubinfo -a`; NAND ID from `dmesg`.
- Map each mtd index to the XDA partition list; record erase-block and page size.

### 8.3 Mounts

- `cat /proc/mounts`; `df -h` (free space on `/data`, `/usrdata`, `/tmp`, `/`); which are ro/rw.

### 8.4 Process list

- `ps w` (busybox) or `ps -ef`; note owner UID of each; confirm `atfwd_daemon`, `goahead`, QCMAP,
  FOTA client, any DIAG consumer; `netstat -tlnup` for listeners.

### 8.5 Init scripts

- `ls -l /etc/init.d /etc/rc*.d`; copy stock `misc-daemon`, `start_atfwd_daemon`, the USB composition
  scripts (`/sbin/usb/compositions/*`, `/data/usb/boot_hsusb_composition`), and the FOTA scripts.
- `cat /usrdata/mode.cfg /usrdata/mode_tmp.cfg` (current USB mode).

### 8.6 Web UI API

- Enumerate `/goform/*` and `/action/*` handlers by string-extracting the `goahead` binary (offline, on
  a copied binary) and by reading the UI JavaScript; document request/response shapes for login,
  `SetRemoteAccessCfg`, `SetPortMode`, firmware-update and reboot endpoints.
- Verify whether `SetRemoteAccessCfg` injection works on V1.2.0 (expected yes; Rayhunter docs make no
  version distinction).

### 8.7 Firmware identity

- Where the version string is stored (`/etc/version`, `/usrdata/...`, web API) and the baseband build
  (`/sys/devices/soc0/build_id`, DIAG `EXT_BUILD_ID`).

## Sources

- <https://www.verizon.com/support/verizon-orbic-speed-mobile-hotspot-update/>
- <https://www.verizon.com/support/verizon-orbic-speed-mobile-hotspot-update-instructions/>
- <https://www.orbic.us/support/orbicspeed/RC400L_Upgrade_Tool_Setup_20210624.exe> (not downloaded)
- <https://mjg59.dreamwidth.org/61725.html>
- <https://mjg59.dreamwidth.org/62419.html>
- <https://xdaforums.com/t/resetting-verizon-orbic-speed-rc400l-firmware-flash.4334899/>
- <https://untitaker.github.io/rayhunter-discussions-mirror/discussions/528.html>
- <https://untitaker.github.io/rayhunter-discussions-mirror/discussions/532.html>
- <https://untitaker.github.io/rayhunter-discussions-mirror/discussions/669.html>
- <https://untitaker.github.io/rayhunter-discussions-mirror/discussions/682.html>
- <https://untitaker.github.io/rayhunter-discussions-mirror/discussions/741.html>
- <https://untitaker.github.io/rayhunter-discussions-mirror/discussions/745.html>
- <https://untitaker.github.io/rayhunter-discussions-mirror/discussions/768.html>
- <https://untitaker.github.io/rayhunter-discussions-mirror/discussions/791.html>
- <https://untitaker.github.io/rayhunter-discussions-mirror/discussions/866.html>
- <https://untitaker.github.io/rayhunter-discussions-mirror/discussions/977.html>
- <https://github.com/EFForg/rayhunter/issues/733>
- <https://github.com/EFForg/rayhunter> (v0.13.0: `installer/src/orbic.rs`, `installer/src/orbic_network.rs`, `installer/src/orbic_auth.rs`, `dist/scripts/misc-daemon`, `dist/scripts/rayhunter_daemon`, `doc/orbic.md`)
- <https://efforg.github.io/rayhunter/orbic.html>
- <https://github.com/stathopulos/orbic>
- <https://dagnazty.github.io/DagShell/features.html>
- <https://github.com/dagnazty/DagShell>
- <https://blog.wsmith.io/posts/orbic-and-tailscale/>
- <https://services.nvd.nist.gov/rest/json/cves/2.0?keywordSearch=Orbic>
