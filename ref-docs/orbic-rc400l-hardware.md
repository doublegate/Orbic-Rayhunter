# Orbic Speed RC400L Hardware Reference

- Retrieved: 2026-11-22
- Device: Orbic Speed RC400L (Verizon), SKU `ORB400LBVZRT`; also sold as Kajeet RC400L / Kajeet v400
  (same hardware per Rayhunter maintainers). Manufacturer of record: Reliance Communications LLC
  (Orbic brand).
- Our unit: firmware `ORB400L_V1.2.0_BVZRT`; USB ID `05c6:f626` "Qualcomm MDM9207-MTP"; enumerates as
  RNDIS + CDC data; web UI at 192.168.1.1 (measured by this project).
- Convention: "unverified" marks claims we could not source. "TBD (on-device)" marks facts we will
  measure ourselves. Nothing in this document was obtained by running commands against our unit
  except the four facts in the line above.

---

## 1. Summary table

| Item | Value | Confidence / source |
|---|---|---|
| SoC | Qualcomm MDM9207 (reports as MDM9607 platform in device tree) | `/proc/cpuinfo` "Hardware: Qualcomm Technologies, Inc MDM9207", `/sys/devices/soc0/machine` = `MDM9207`, `soc_id` 297; device tree `qcom,mdm9607-mtp` (user report, Rayhunter discussion #741) |
| CPU | 1 x ARM Cortex-A7 (ARMv7 rev 5, part 0xc07), NEON, VFPv4, LPAE | same user report |
| CPU clock | Up to 1.3 GHz is commonly quoted for MDM9x07 | unverified; TBD (on-device) via `/sys/devices/system/cpu/cpu0/cpufreq/` |
| LTE category | Cat 4 | mjg59 blog ("LTE Cat 4 only"); Qualcomm 9207 product page (not text-verified) |
| Baseband build ID | `9x07A-LAATANAZA-40000000` (image_variant `LAATANAZA`) | user report #741 (firmware version of that unit unknown) |
| RAM | Unknown size; reported very tight (2-4 MB free with Rayhunter + Tailscale running) | wsmith blog; size TBD (on-device) `/proc/meminfo` |
| Flash | Raw NAND with UBI (no eMMC, no GPT) | XDA thread (Renate), mjg59 follow-up (`/proc/mtd`); size TBD (on-device) |
| Memory package | Nanya-branded package beside the MDM, likely a NAND + LPDDR MCP | FCC internal photo p.6 (marking illegible at that resolution); part number unverified |
| PMIC | Probably PMD9607 / PM9607-class (marking partly visible in FCC photo) | unverified |
| Wi-Fi | 2.4 GHz 802.11b/g/n and 5 GHz 802.11a/n/ac, 2T2R MIMO, up to 40 MHz (2.4) / 80 MHz (5) | FCC grant notes. Chip: Rayhunter docs name QCA6174; not independently verified |
| Display | Small colour LCD, 128 x 128 framebuffer at `/dev/fb0` | Rayhunter `daemon/src/display/orbic.rs`; panel size/controller unverified |
| Battery | Removable Li-ion, model BTE-3003, 3000 mAh, 11.1 Wh, 3.7 V nominal, 4.2 V charge, maker Huizhou DXDragon Inc. | FCC internal photos p.1 (battery label) |
| Battery telemetry | `/sys/kernel/chg_info/level`, `/sys/kernel/chg_info/chg_en` | Rayhunter `daemon/src/battery/orbic.rs` |
| Buttons | Power (front), reset (pinhole under back cover beside battery) | FCC photo p.3 ("RESET" label), Rayhunter docs |
| Input events | `/dev/input/event0` (power key) | Rayhunter `daemon/src/key_input.rs` |
| USB | USB Type-C receptacle | FCC internal photos |
| SIM | Nano/micro SIM slot under battery (form factor unverified) | FCC photos |
| PCB | Main board silk "DX-L7"-series antennas (`DX-L7-MAIN-V1`, `DX-L7-DIV-V1`) | FCC photos; suggests DXDragon-family ODM design (inference, unverified) |
| UART | No UART found on Orbic by Rayhunter maintainer; Moxee (same platform) has 1.8 V LK-only UART pads | Rayhunter discussion #993 |

## 2. RF bands (FCC filing 2ABGH-RC400L)

FCC equipment authorization (granted 2020-05-07, test lab Shenzhen Morlab, TCB Phoenix Testlab) lists:

| Rule part | Uplink range (MHz) | Band | Mode |
|---|---|---|---|
| 27 | 779.5-784.5 | LTE B13 | LTE (QPSK/16QAM) |
| 22H | 824.7-848.3 | LTE B5 | LTE |
| 22H | 826.4-846.6 | UMTS B5 | WCDMA (4M14F9W) |
| 27 | 1710.7-1754.3 | LTE B4 | LTE |
| 27 | 1710.7-1779.3 | LTE B66 | LTE |
| 24E | 1850.7-1909.3 | LTE B2 | LTE |
| 24E | 1852.4-1907.6 | UMTS B2 | WCDMA |
| 15C | 2412-2462 | Wi-Fi 2.4 GHz | 802.11b/g/n, 2T2R |
| 15E | 5180-5240, 5745-5805 | Wi-Fi 5 GHz (UNII-1, UNII-3) | 802.11a/n/ac, 2T2R |

LTE therefore covers B2/B4/B5/B13/B66 in the FCC filing. The Rayhunter book lists
"4G 2/4/5/12/13/48/66" and also 5G bands; the 5G entries are wrong for this hardware (MDM9207 is
LTE-only), and B12/B48 do not appear in the FCC grant (B48/CBRS would need a Part 96 grant). Treat
B12/B48 as unverified. Highest reported simultaneous-transmission SAR: 1.54 W/kg.

## 3. USB modes and composition

| VID:PID | Mode | How entered | Source |
|---|---|---|---|
| `05c6:f626` | Default: RNDIS + CDC (our unit, "Qualcomm MDM9207-MTP") | Normal boot | Measured; Rayhunter `installer/src/orbic.rs` uses it as the pre-switch PID |
| `05c6:f601` | Debug: ADB + DIAG + AT serial (interface 1, bulk EP 0x02/0x82), no RNDIS | Vendor control request `bmRequestType=0xC0` (vendor, device), `bRequest=0xA0`, `wValue=0`, `wIndex=0`, no data, sent to the RNDIS interface; device reboots | Rayhunter `enable_command_mode()`; mjg59 blog; XDA #8 |
| `05c6:f622` | RNDIS plus serial (Rayhunter's `open_orbic()` also accepts it) | e.g. `AT+SER=9,1` (adb + diag + at + rndis, persisted) | Rayhunter `open_orbic()`; mjg59/XDA #8 |
| `05c6:9008` | Qualcomm EDL (Sahara/Firehose, "QHSUSB__BULK") | Hardware test point (see section 4) | XDA (Renate); Rayhunter discussion #993 (Moxee) |

USB composition mechanism (mjg59): `AT+SER=<mode>,<1 permanent | 0 temporary>` is handled by
`atfwd_daemon`, which runs `/sbin/usb/compositions/switch_usb`; the mode is written to
`/usrdata/mode.cfg` (permanent) or `/usrdata/mode_tmp.cfg` (temporary) and applied at boot by
`/data/usb/boot_hsusb_composition`. Mode 3 requires an RSA "chipid" check; mode 9 (rndis + adb + diag +
at) does not. `AT+SER=3,1` returns to the normal (no-ADB) mode used by the Verizon update tool.

AT serial: the vendor control request `0x22` (class, interface, value 3, index 1) is sent before AT
writes (CDC SET_CONTROL_LINE_STATE: DTR+RTS). Commands are framed `\r\n<cmd>\r\n`; success is
`\r\nOK\r\n` (Rayhunter `send_serial_cmd`).

The exact interface list (class/subclass/protocol per interface) of each PID on our unit is
TBD (on-device, host-side `lsusb -v` only).

## 4. Recovery: EDL, fastboot, firehose

- Boot ROM EDL exists (it is in the MDM9207 PBL) and appears as `05c6:9008`. There is no software path
  into EDL: `adb reboot bootloader` does nothing and the LK fastboot has no `reboot-edl` /
  `oem edl` (XDA, mjg59).
- Hardware entry (XDA, Renate, unverified by us): lift the LCD; the "sense" test point under the
  top-left corner of the LCD, grounded during reset, gives fastboot; tied to 1.8 V (about 1 kOhm
  suggested) gives EDL. On the Moxee (same platform), holding reset then power+reset enters EDL
  without opening the case (discussion #993); this is not confirmed on the Orbic.
- Sahara identity: Orbic `HWID 000480e100000000, MSM 000480e1, OEM 0000, Model 0000,
  Hash cc3153a80293939b` (XDA, truncated hash); Moxee `MSM_ID 0x0004a0e1, OEM_ID 0x0158,
  MODEL_ID 0x779a, PK_HASH 952f545d...260df8` (discussion #993). They differ, so Moxee results do not
  transfer directly.
- Firehose programmer: none confirmed working for the Orbic or Moxee. Candidates tried or suggested:
  `prog_nand_firehose_9x07.mbn` (bkerler Loaders, patched/mdm9x07), `nprg9x07p.bin`, `NPRG9x07.mbn` /
  `ENPRG9x07.mbn` (Biktorgj/quectel_eg25_recovery). All hung after upload on the Moxee; one XDA user
  reports the Orbic needs a different loader. One XDA user claims a private full-reflash procedure
  (unverified, nothing published).
- Secure boot: aboot is signed with "General Use Test Key (for testing only)", suggesting secure boot
  is not enforced (mjg59). If the OEM PK hash is a Qualcomm test hash, an unsigned or generic 9x07 NAND
  programmer could be accepted; this is untested and unverified.
- Planning implication: there is no proven unbrick path. A raw NAND backup taken from a root shell
  before any write (see `orbic-firmware-v1.2.0.md`) is the only recovery asset we can count on, and
  restoring it requires a working Linux (or an EDL programmer we do not yet have).

## 5. FCC filing and teardown references

- FCC ID: 2ABGH-RC400L (grantee code 2ABGH, Reliance Communications LLC; equipment description
  "Orbic Speed (Wireless router with LTE, WCDMA, WIFI)").
- Internal photos (7 pages: battery label, main board both sides with and without shields, antenna
  layout): <https://fccid.io/2ABGH-RC400L/Internal-Photos/Internal-Photos-4714662> and
  <https://fcc.report/FCC-ID/2ABGH-RC400L/4714662>
- External photos: <https://fccid.io/2ABGH-RC400L/External-Photos/External-Photos-4714661>
- User manuals: PSWI <https://fccid.io/2ABGH-RC400L/User-Manual/User-Manual-PSWI-4714664> ,
  QSG <https://fccid.io/2ABGH-RC400L/User-Manual/User-Manual-QSG-4714665>
- Test reports (Morlab SZ20040157*): EMC 4714562, WCDMA 4714639, LTE parts 1-5 4714640/4714641/
  4714657/4714658/4714659, Wi-Fi DTS 4714492, Wi-Fi NII 4714593 (base URL
  `https://fccid.io/2ABGH-RC400L/Test-Report/...`), SAR 4714678.
- Block diagram, schematics, BOM, operational description and tune-up exhibits are under long-term
  confidentiality (metadata only).
- Antennas: main LTE (top, `DX-L7-MAIN-V1`), diversity LTE (bottom, `DX-L7-DIV-V1`), two Wi-Fi
  antennas on the sides (FCC internal photos p.7).
- Teardown/notes: Rayhunter discussion #741 (on-target `uname`, cpuinfo, soc0); mjg59 "Poking a
  mobile hotspot" and follow-up; Rayhunter discussion #993 (Moxee teardown, UART).

## 6. Related devices on the same platform

| Device | Relationship |
|---|---|
| Kajeet RC400L / Kajeet v400 / Smartspot | Same hardware; default admin password `$m@rt$p0tc0nf!g`; some units have password-protected telnetd on port 23 |
| Moxee Mobile Hotspot (K779HSDL) | Same platform and admin UI; ADB serial enable does not work; different Sahara OEM ID/PK hash |
| Orbic Speed 5G UW (R500L5), Orbic R100ML5 (SDX55) | Different hardware; not covered here |

## 7. TBD (on-device)

To be measured on our unit and recorded here with the exact command:

- `cat /proc/cpuinfo`, `uname -a`, `/sys/devices/soc0/*` (compare with discussion #741).
- CPU frequency range: `cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_available_frequencies`.
- `cat /proc/meminfo` (total RAM, free under idle and with Rayhunter).
- `cat /proc/mtd` (partition sizes, erase block size), `ubinfo -a`, NAND chip ID from `dmesg`.
- Display controller and panel size (`dmesg | grep -i fb`, `/sys/class/graphics/fb0/*`).
- Wi-Fi chip: `dmesg`, `lspci` (if PCIe) or module list; confirm QCA6174 or otherwise.
- USB descriptors for PIDs f626, f601, f622 from the host (`lsusb -v -d 05c6:<pid>`).
- Battery: `/sys/kernel/chg_info/*` values and charge behaviour on USB power.
- Whether power+reset enters EDL on the Orbic without opening the case (only if we accept the risk;
  observing enumeration does not write anything).

## Sources

- <https://fccid.io/2ABGH-RC400L>
- <https://fccid.io/2ABGH-RC400L/Internal-Photos/Internal-Photos-4714662>
- <https://fcc.report/FCC-ID/2ABGH-RC400L/4714662>
- <https://untitaker.github.io/rayhunter-discussions-mirror/discussions/741.html> (GitHub: <https://github.com/EFForg/rayhunter/discussions/741>)
- <https://untitaker.github.io/rayhunter-discussions-mirror/discussions/993.html> (<https://github.com/EFForg/rayhunter/discussions/993>)
- <https://untitaker.github.io/rayhunter-discussions-mirror/discussions/528.html>
- <https://untitaker.github.io/rayhunter-discussions-mirror/discussions/389.html>
- <https://untitaker.github.io/rayhunter-discussions-mirror/discussions/791.html>
- <https://mjg59.dreamwidth.org/61725.html>
- <https://mjg59.dreamwidth.org/62419.html>
- <https://xdaforums.com/t/resetting-verizon-orbic-speed-rc400l-firmware-flash.4334899/>
- <https://blog.wsmith.io/posts/orbic-and-tailscale/>
- <https://efforg.github.io/rayhunter/orbic.html>
- <https://github.com/EFForg/rayhunter> (v0.13.0: `installer/src/orbic.rs`, `daemon/src/display/orbic.rs`, `daemon/src/battery/orbic.rs`, `daemon/src/key_input.rs`, `doc/orbic.md`)
- <https://www.qualcomm.com/modems/products/qualcomm-9207-lte-modem>
- <https://github.com/bkerler/edl>
- <https://github.com/bkerler/Loaders>
- <https://github.com/Biktorgj/quectel_eg25_recovery>
- <https://github.com/qualcomm/qdlrs>
- <https://github.com/SoMainline/linux/blob/1a674af2625ba214b641b488c7868ef5ced1adb0/arch/arm/boot/dts/qcom-mdm9607.dtsi>
