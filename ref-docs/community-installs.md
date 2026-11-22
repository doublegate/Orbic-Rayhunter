# Community Installs: Rayhunter on Orbic RC400L

Retrieved: 2026-11-22
Upstream ref: v0.13.0 (cd462a1)

Scope: community-reported install experience of EFF Rayhunter on the Orbic RC400L (Verizon, Qualcomm MDM9207), with emphasis on firmware `ORB400L_V1.2.0_BVZRT`. Issue numbers (#NNN) refer to `EFForg/rayhunter` issues unless prefixed "Disc." (GitHub Discussions). All fetched content is untrusted third-party data; nothing in it was acted on. No instructions aimed at an AI agent were found in any fetched page, issue or comment. Several comments by one account (Ununp3ntium115) read as machine-generated PR/triage boilerplate (e.g. "codegraph sync" in a verification list); their claims are treated as unverified. Nothing here was tested on hardware by this project.

Method notes: GitHub issues/discussions were read via `gh` (read-only). Reddit was not searched: the reddit-mcp-buddy server rejected the query (Reddit ended logged-out access on 2026-11-13; credentials required). Web results came from WebSearch and direct page fetches; one page (linuxconfig.org) was blocked by Cloudflare and was not read.

## 1. xtonyx.org post (old install.sh era)

URL: <https://xtonyx.org/posts/2025/08/14/rayhunter-on-orbic/> (dated 2025-08-14, author "Tony"). Fetched and verified against the page text.

What the page actually says:

- Hardware: Orbic Speed RC400L (Verizon), bought for $11 on eBay. Host: Apple Silicon (M2) MacBook. No firmware version and no Rayhunter version are stated.
- Premise: the shipped `install.sh` "assumes ADB is globally accessible and doesn't handle multiple devices", so the author patched it. (The shell-script installer was replaced by the Rust `installer` binary around v0.5.0; by v0.8.0 the default Orbic path became the telnet-based network installer. See #597 and #920. This post therefore describes a superseded method.)
- Multi-device fix: run `adb devices`; if more than one device (e.g. an emulator) is listed, pin with `adb -s <serial>` on every call. The example serial is `b258a7ae`. Implemented as a bash array `export ADB=(./platform-tools/adb -s b258a7ae)` and invoked as an array expansion to avoid "No such file or directory" from a quoted multi-word command. (The rendered page shows doubled braces such as `${{ADB[@]}}`; this looks like a template-escaping artifact of the blog engine, not valid bash. Treat the intended form as `"${ADB[@]}"`.)
- Timeout: `wait_for_atfwd_daemon()` polls `pgrep atfwd_daemon` through the device shell for up to 60 iterations with `sleep 1`, then prints "timeout waiting for atfwd_daemon" and `exit 1`, instead of looping forever.
- Install flow described: push the rootshell binary to `/data/local/tmp/`, wait for `atfwd_daemon`, install and start the Rayhunter server, forward the port.
- Web UI access: `adb forward tcp:8080 tcp:8080`, then browse `http://localhost:8080`. A `test_rayhunter()` helper re-issues the forward and curls the URL for up to 30 s.
- "Pro tips": kill a stray emulator with `adb -s emulator-XXXX emu kill`; reset ADB with `adb kill-server && adb start-server`; "Make sure `pgrep` works on the device; if not, use `ps | grep` fallback."

Verification of the expected lessons:

| Expected lesson | On the page? | Note |
|---|---|---|
| Pin `adb -s <serial>` with multiple devices | Yes | Explicit, with array form |
| Timeout on atfwd_daemon wait loop | Yes | 60 x 1 s, then exit 1 |
| `pgrep` fallback | Partly | Only a prose tip ("use `ps \| grep`"); the posted code calls `pgrep` with no fallback implemented |
| `adb forward tcp:8080 tcp:8080` | Yes | Same technique reappears for recovery in #1033 and Disc. #842 |

Relevance today: the patching advice applies only to the legacy `orbic-usb` path (ADB). Current upstream docs state the UI is at `http://192.168.1.1:8080` over the hotspot WiFi (upstream `doc/orbic.md`, v0.13.0). `adb forward tcp:8080 tcp:8080` remains the practical way to reach the UI when the hotspot AP is unreachable (#1033, Disc. #842) and requires a device that exposes ADB (orbic-usb install or later enabled). Note #523/#294: enabling tethering mode can make ADB disappear.

## 2. Firmware versions seen in the wild

All Verizon/BVZRT unless noted. "Outcome" is as reported by the user; most are single reports.

| Version | Carrier / SKU | Reported outcome | Source |
|---|---|---|---|
| ORB400L_V1.1.5_BVZRT (R200919) | Verizon | Original reference firmware; USB (ADB) install worked; later network install reports (#946, #966) on 1.1.5 failed for network/Windows reasons, not firmware per se. Verizon auto-update to 1.2.1/1.3.0 broke Rayhunter for some users (see below). | #156, #163, #294, #946, #966, #720 |
| ORB400L_V1.2.0_BVZRT | Verizon | ADB not visible on stock firmware on Mac (#228) and one user had only a single serial port visible (#130); fix was running Verizon's Windows RC400L Upgrade Tool just far enough to set developer/ADB mode. "Qmi Send Message Fail!" at "Installing rootshell" on v0.3.3 Windows (#366), later same-error duplicates (#497); same user reported macOS ARM worked first try. Rayhunter v0.11.1 WiFi Client Mode on this firmware made the AP/web UI unreachable (#1033). Reported by a tester as "does not allow tethering" (Disc. #842, unverified). | #228, #130, #366, #497, #1033, Disc. #842 |
| ORB400L_V1.2.1_BVZRT | Verizon | After `echo 9 > /usrdata/mode.cfg` and reboot, ADB no longer enumerated (v0.5.0/0.5.1) (#523). Earlier upgrade from 1.1.5 to 1.2.1 left the service not starting after install (#156; cause there traced to #151 corrupted capture data). | #523, #156 |
| ORB400L_V1.2.6_BVZRT, HW V1.1 | Verizon | Installed; WiFi client mode connects to WPA2 but not WPA3-SAE (v0.13.0) (#1177). | #1177 |
| ORB400L_V1.2.8_BVZRT, HW V1.1 | Verizon | Network installer fails (`connection closed before message completed`) and USB installer fails at "Installing rootshell" (`AT+SYSCMD=mv /tmp/rootshell /bin/rootshell`) on v0.10.1; maintainer says it fails only rarely elsewhere. Still open. | #901 |
| ORB400L_V1.3.0_BVZRT | Verizon | Mixed. Maintainer ran 1.3.0 "just fine" in March 2025 (#156); a user reported the 1.3.0 forced update locked the device and removed root (#163, USB-era, pre-network installer). Network installer worked on 1.3.0 for a green-line-with-SIM-ambiguity report (#875), uninstall over ADB fails on 1.3.0 due to file ownership (#701), and `connection refused` on telnet persisted on two 1.3.0 units after resets (Disc. #768). #522 (1.3.0, v0.5.x): install failed "No Orbic device found" until USB tethering was enabled; then it succeeded. #901 maintainers cite #522 as the "even newer firmware works" datapoint. | #156, #163, #522, #701, #875, Disc. #768 |
| 1.3.1 / 1.4.x | n/a | No issue or discussion found mentioning these versions (searches: "1.3.1", "V1.3", "1.4"). Unknown. | search results |
| Kajeet RC400L / RC400LX (ORB400LXUSBC) | Kajeet / AT&T-T-Mobile SKU | Same hardware; installs fine. Default admin password for managed Kajeet RC400LX is documented upstream. A "no data" report was a bad SIM, not Rayhunter. | #112, #178, #388, #801, #762 |
| ORB500L5 (Orbic Speed 5G R500L5) | Verizon | Different device (systemd, SDX55); installs but daemon does not start; unsupported. Included to avoid confusion with RC400L. | Disc. #739 |

Newer-firmware lockout evidence: the only claim that a Verizon OTA update blocks root/Rayhunter is #163 (2025-03, forced update to 1.3.0 "locked the device, removed root access, and broke Rayhunter"; the poster's mitigations were to stop `dmclient` updates). It was closed as a duplicate of #156, whose maintainer reply says 1.3.0 works. Later evidence (network exploit in v0.7+/0.8.0) works on 1.2.x and 1.3.0 firmware with a WiFi admin password, so a hard exploit lockout on 1.3.0 is not demonstrated; the 1.3.0 problems seen are installer/transport failures. No report of a post-1.3.0 firmware. Unverified: whether Verizon pushes a still-newer image to an activated SIM.

Observation relevant to updates: the OTA/forced update is reported to need Verizon network contact (an inactivated SIM kept 1.1.5) (#163). A tester also reported the Verizon upgrade tool only works from exactly 1.1.5, otherwise it requires an active data connection for the OTA (Disc. #842).

## 3. Install problems and fixes

Install paths for reference (upstream docs at v0.13.0): `installer orbic --admin-password '<pw>'` (network/telnet over the hotspot WiFi) is the recommended path; `installer orbic-usb` is the legacy ADB path "not recommended" and has repeatedly bricked or half-installed devices on Windows (Disc. #768, #866).

| Symptom | Likely cause | Fix / workaround | Source |
|---|---|---|---|
| `Login failed with retcode: 201` | Wrong password: the WiFi 2.4G, 5G and admin passwords are separate fields and an eBay device may have been changed; or device never factory-reset | Verify the password logs in at `http://192.168.1.1`; reset the device from the web UI login page (not only the hardware button); error message clarified in PR #869 | #767, #797, #693 |
| `Failed to get login info ... tcp connect error / Operation timed out` | Host is not on the Orbic WiFi (on the home network or USB instead), or Windows firewall | Connect directly to the Orbic hotspot SSID; confirm `http://192.168.1.1` loads. Installing while WiFi to the Orbic is dropping gives the same error | #720, #807, #955 |
| `No session cookie received` | macOS/Windows with USB-only connection (no hotspot WiFi); maintainers saw it Windows-specific | Join the device WiFi, or use `installer orbic-usb` (worked on macOS for one user) | #966, #955 |
| `Failed to send login request / connection closed before message completed` (telnet start) | Flaky first telnet start; some hosts' WiFi adapters (an Intel AX210 failed, an ASUS USB adapter worked); some units persistently refuse | Retry the installer repeatedly; try another WiFi adapter or OS; reboot the Orbic and wait (one user succeeded after a few days idle and a reboot); PR #902 attempts to fix flakiness | #643, #901, #946, Disc. #768, Disc. #791 |
| `"exit code 0" not found in: command done, exit code 1` | Partial prior install, or the installer's `mount -o remount,rw /dev/ubi0_0 /` step returning 1 | Clear `/data/rayhunter` via a telnet root shell (`installer util orbic-start-telnet`, then `nc 192.168.1.1 24`) and rerun. A claimed PR (#1165) removes the remount; v0.13.0 source still has the remount at `installer/src/orbic_network.rs:225` (verified locally); PR status unverified | #693 |
| `background nc writer failed ... command timed out after 10 seconds` while sending `rayhunter-daemon` | Transfer interrupted: WiFi dropout, or low free space | Keep the host on the Orbic WiFi (display shows 1 connected device), factory reset, free storage | #946, #955 |
| Bad hash / `File transfer unsuccessful` / `Received unexpected response: AT+SYSCMD=mv ...` | `/data` full from prior captures (legacy installer) or a first-boot race; seen on Kajeet units on first install | Delete `/data/rayhunter/qmdl` (after `adb pull`), rerun; looping `until ./installer orbic; do sleep 1; done` got past it sometimes; rebooting the device then rerunning worked for Kajeet | #510, #152 |
| `rootshell is not giving us root` (legacy) | rootshell created but not root until power cycle | Power cycle the Orbic and rerun; one user finished install manually via adb; superseded by network installer in 0.8.0 | #597 |
| `Qmi Send Message Fail!` at "Installing rootshell" | Legacy `orbic-usb` on Windows with extra WinUSB drivers; bad cable | Keep WinUSB driver only on interface 1; try another data cable; macOS worked first try for the same reporter. Superseded by the network installer | #366, #497 |
| `No Orbic device found` (legacy) / ADB not visible | Orbic not in ADB composition; bad USB-C cable; USB tethering disabled; for fw 1.2.0 stock ADB off | Use a different data cable; enable USB tethering in device settings; run Verizon's RC400L Upgrade Tool until it sets dev mode; native Linux rather than a VM | #522, #228, #130, #955 |
| `Timeout waiting for Orbic to reconnect` after a `mode.cfg` edit | Setting USB mode 9 changed the USB product ID (f622) from the one the installer scans (f601); also the serial interface index shifts | Do not switch USB mode if you rely on `orbic-usb`; PR claimed to re-discover Orbic IDs (unverified) | #523, #920 |
| Installer "does nothing" when double-clicked | `installer` is a CLI binary | Launch from a terminal in the extracted folder; macOS needs `xattr -d com.apple.quarantine installer` | #1112, #1040, #248 |
| Password with special chars fails in Windows shells | Quote-handling differences across cmd/PowerShell | Use PowerShell; single quotes per upstream docs | #767, #769 |
| Install "succeeds" but no green line / daemon not running | Service did not start after reboot; stale or corrupt qmdl data (#151) | Reboot via power button; run `/data/rayhunter/rayhunter-daemon config.toml` manually in a root shell and read the error; clear `/data/rayhunter/qmdl/` | #156, #192 |
| USB tethering gone after install (orbic-usb) | ADB composition replaces RNDIS | `adb shell '/bin/rootshell -c "echo 9 > /usrdata/mode.cfg"'` then reboot restores RNDIS+ADB per a contributor (did not work for a later user, see #523); on newer installers tethering works without this | #294, #523, Disc. #842 |

Port/ADB tips from the xtonyx page that still hold: `adb kill-server` clears "device in use by another program" (Disc. #669), and `adb forward tcp:8080 tcp:8080` reaches the UI over USB.

## 4. Hardware and runtime issues

Storage (`/data`)

- Rayhunter does not rotate recordings by default; the device stops recording when under 1 MB is free per maintainer (#1192). In #1192 (v0.13.0, Nov 2026) `/data` reached 100 percent with a qmdl directory over 200 MB, the daemon was not running, the hotspot AP vanished, and the web UI was gone; recovery needed `/bin/rootshell` over ADB, because the adb user cannot delete the files. The maintainer was unsure why the stop-at-1-MB guard did not fire; log growth is a hypothesis (unverified). A fix to wipe recordings in the installer was endorsed.
- Earlier: #152 (closed) and #510 (open) describe the same failure mode; capture deletion exists in the web UI now. Users report constant writes to flash (#955, #946) and the web UI misbehaving when the capture directory fills (#152). A corrupted NDJSON analysis line also broke the UI (#152, refs #184).
- Storage management feature requests: #607 (open), #819 (closed).

Display / LEDs

- Screen blanks after a few seconds; framebuffer writes do not count as activity. #539 and #916 (open) request keep-screen-on; maintainers say waking per status change must be implemented per device.
- With the EFF logo or full-screen status enabled, the screen flickers against the battery icon while charging off (#732, maintainer: "very unlikely to fix"). #1160 (open): the full-screen indicator shows green while the device is powered off and charging because Rayhunter starts in charge mode; cooperq suggests removing it from that bootmode in init.d, low priority.
- Green line does not prove a working capture path: it appears with no SIM or an unactivated SIM (#875, Disc. #1099).
- Double-press of power restarts the recording (documented), it does not stop it (#952).

Battery and heat

- No Orbic-specific heat or battery-runtime complaints were found in issue text. Searches for "heat", "hot", "thermal" returned nothing Orbic-specific. Unverified: battery life under continuous diag logging, thermal behavior. A user notes keeping the device on constantly writes storage (#946).
- Used units from eBay often have dead batteries (#522).

WiFi

- WiFi client mode (Orbic QCA6174 can run AP and station together, upstream docs v0.13.0). Failures: #1033 (v0.11.1, firmware V1.2.0: enabling client mode left no AP and no web UI; recovered via ADB port forward, or by re-pushing the package; the UI showed "scan failed with -EIO; radio may be busy (module reload needed)"), #1177 (v0.13.0, V1.2.6: WPA3-SAE unsupported, WPA2 on 5 GHz channels 36 and 112 works; maintainers will not fix).
- Fully disabling the hotspot AP needs a root edit of `/usrdata/data/usr/wlan/wlan_conf_6174.xml` (#880, #753); risk of lockout if Rayhunter fails to start, since the AP is how you reach the installer (Disc. #842).
- Wrong `/etc/ver` revisions and per-device oddities exist on eBay lots: one user's 10-device lot at "1.1.5" refused WiFi/USB installs until the Verizon upgrade tool put them on 1.2.x (Disc. #866; single source, unverified; maintainer suspects a bad factory flash).

Modem / carrier

- SIM required for the modem to attach, active plan not required (Disc. #507). An unactivated or cancelled SIM yields only broadcast-based heuristics (SIB1/paging) and may never connect (Disc. #507 comment by AlienMajik on a 0.13.0-based custom build, unverified).
- Carrier lock: unclear how locked; one user's Telus/Bell SIMs failed or worked intermittently (#207, open); a RC400LX failure was a bad SIM (#178).
- Spurious "Identity requested in suspicious manner v2 / Disconnected after Identity Request without Auth Accept" warnings reported on Orbic 400L and others, worse in low signal (#543, open).

## 5. Community enhancements and forks

| Item | What it is | Source |
|---|---|---|
| Keep-screen-on watchdog | Proposed config option; open feature request | #916, #539 |
| WiFi client mode | Merged upstream (PR #888 per #589 and upstream docs); known issues above | #589, #1033, #1177 |
| ntfy notifications | Shipped; https/URL issues; no-internet case is unsolved | #942, #770, #778, Disc. #1173, #539 |
| Notification workaround without a data plan | A Raspberry Pi connected to the Orbic over USB acts as a fake ntfy receiver and polls the HTTP API (health, disk, battery) and forwards to MQTT/Home Assistant | Disc. #777 |
| Bundled/pre-flashed units | Commercial sellers ship RC400L units pre-flashed (e.g. Tindie "STS Collective", $99) | <https://www.tindie.com/products/stscollective/new-rayhunter-imsi-catcher-stingray-detector> |
| Web flasher front ends | Third-party browser guides wrapping the upstream installer (orbic network and USB) | <https://raddad87.github.io/RayHunter-Web-Flasher/> |
| DagShell | Third-party custom firmware for the "RCL400" that requires pre-existing root via the same network exploit; adds an HTTPS dashboard on 8443, Pi companion for GPS/BT/WiFi scanning. Not reviewed or audited; unverified safety; use at own risk | <https://dagnazty.github.io/DagShell/installation.html> |
| Larger storage | SD-card style storage is mentioned only for TP-Link; Orbic RC400L has no SD slot per community reports; a contributor suggested pointing captures at SD if present (generic) (#152). No Orbic storage-expansion mod found | #152, #607 |
| GPS | Browser-location GPS sensor proposal (open) | #1047 |
| Root sshd / Alpine chroot | One user installed sshd and chrooted into Alpine on the device with the legacy root | #294 |
| Fixed-sensor / other ports | Inseego M2100 port with touchscreen UI, Home Assistant add-on, Meshtastic alerts (separate device) | Disc. #1195 |
| Batch flashing | Maintainers insist the USB installer stays for mass installs (hundreds of units) | #762, #920 |
| LTE bands | Docs claim 5G/Wi-Fi 6; Verizon manual lists B2/B4/B5/B13/B66; EU band change request | #695, #1129, #259 |

## 6. Open questions relevant to firmware V1.2.0_BVZRT

1. Does `installer orbic` (network/telnet, v0.13.0) install cleanly on V1.2.0? Evidence: no direct report on 1.2.0 with the modern installer; 1.2.0 reports are legacy ADB (#228, #366, #130) and v0.11.1 daemon-side (#1033). The closest successes are on 1.2.6, 1.2.8 (failing, #901) and 1.3.0 (#522, #875). Unverified for 1.2.0.
2. On V1.2.0 stock ADB may be disabled and the Verizon tool may be required to set developer mode (#228, #130); the network installer avoids ADB, so ADB may stay absent. Confirm that the `adb forward tcp:8080` recovery path from #1033 and the xtonyx post is usable on this unit.
3. Is the installer's `mount -o remount,rw /dev/ubi0_0 /` step harmful on this firmware (#693 suggests exit code 1)? The v0.13.0 source still performs it (`installer/src/orbic_network.rs:225`).
4. The #1033 daemon log was captured on `Orbic 400L_V1.2.0_BVZRT`: WiFi client mode must be treated as risky on this firmware until tested; verify the wifi_station module-reload path (`-EIO`).
5. Updates: does an activated Verizon SIM push 1.2.1/1.3.0 and break the exploit or root (#163)? Keep the SIM inactive or block the DM client during development; the OTA reportedly needs Verizon network contact.
6. Is the "stop recording below 1 MB free" guard reliable (#1192)? Confirm on this unit, and plan for `/data` full-state recovery and an installer-side purge of old recordings.
7. Can the project add Orbic-side power/charge-mode detection (#1160), keep-screen-on (#916), and power-key behavior notes (#952) without breaking the upstream update path?
8. Version gaps: no data on any firmware newer than 1.3.0; no data on 1.2.2 to 1.2.5, 1.2.7.

## Sources

- <https://xtonyx.org/posts/2025/08/14/rayhunter-on-orbic/>
- <https://github.com/EFForg/rayhunter/issues/6>
- <https://github.com/EFForg/rayhunter/issues/130>
- <https://github.com/EFForg/rayhunter/issues/151>
- <https://github.com/EFForg/rayhunter/issues/152>
- <https://github.com/EFForg/rayhunter/issues/156>
- <https://github.com/EFForg/rayhunter/issues/163>
- <https://github.com/EFForg/rayhunter/issues/178>
- <https://github.com/EFForg/rayhunter/issues/192>
- <https://github.com/EFForg/rayhunter/issues/207>
- <https://github.com/EFForg/rayhunter/issues/228>
- <https://github.com/EFForg/rayhunter/issues/259>
- <https://github.com/EFForg/rayhunter/issues/294>
- <https://github.com/EFForg/rayhunter/issues/366>
- <https://github.com/EFForg/rayhunter/issues/388>
- <https://github.com/EFForg/rayhunter/issues/401>
- <https://github.com/EFForg/rayhunter/issues/497>
- <https://github.com/EFForg/rayhunter/issues/510>
- <https://github.com/EFForg/rayhunter/issues/522>
- <https://github.com/EFForg/rayhunter/issues/523>
- <https://github.com/EFForg/rayhunter/issues/539>
- <https://github.com/EFForg/rayhunter/issues/543>
- <https://github.com/EFForg/rayhunter/issues/586>
- <https://github.com/EFForg/rayhunter/issues/589>
- <https://github.com/EFForg/rayhunter/issues/597>
- <https://github.com/EFForg/rayhunter/issues/607>
- <https://github.com/EFForg/rayhunter/issues/643>
- <https://github.com/EFForg/rayhunter/issues/677>
- <https://github.com/EFForg/rayhunter/issues/693>
- <https://github.com/EFForg/rayhunter/issues/695>
- <https://github.com/EFForg/rayhunter/issues/701>
- <https://github.com/EFForg/rayhunter/issues/720>
- <https://github.com/EFForg/rayhunter/issues/732>
- <https://github.com/EFForg/rayhunter/issues/737>
- <https://github.com/EFForg/rayhunter/issues/753>
- <https://github.com/EFForg/rayhunter/issues/762>
- <https://github.com/EFForg/rayhunter/issues/767>
- <https://github.com/EFForg/rayhunter/issues/769>
- <https://github.com/EFForg/rayhunter/issues/793>
- <https://github.com/EFForg/rayhunter/issues/797>
- <https://github.com/EFForg/rayhunter/issues/801>
- <https://github.com/EFForg/rayhunter/issues/807>
- <https://github.com/EFForg/rayhunter/issues/875>
- <https://github.com/EFForg/rayhunter/issues/880>
- <https://github.com/EFForg/rayhunter/issues/901>
- <https://github.com/EFForg/rayhunter/issues/916>
- <https://github.com/EFForg/rayhunter/issues/920>
- <https://github.com/EFForg/rayhunter/issues/946>
- <https://github.com/EFForg/rayhunter/issues/952>
- <https://github.com/EFForg/rayhunter/issues/955>
- <https://github.com/EFForg/rayhunter/issues/966>
- <https://github.com/EFForg/rayhunter/issues/1023>
- <https://github.com/EFForg/rayhunter/issues/1033>
- <https://github.com/EFForg/rayhunter/issues/1040>
- <https://github.com/EFForg/rayhunter/issues/1047>
- <https://github.com/EFForg/rayhunter/issues/1112>
- <https://github.com/EFForg/rayhunter/issues/1129>
- <https://github.com/EFForg/rayhunter/issues/1160>
- <https://github.com/EFForg/rayhunter/issues/1177>
- <https://github.com/EFForg/rayhunter/issues/1192>
- <https://github.com/EFForg/rayhunter/discussions/507>
- <https://github.com/EFForg/rayhunter/discussions/739>
- <https://github.com/EFForg/rayhunter/discussions/777>
- <https://github.com/EFForg/rayhunter/discussions/1099>
- <https://github.com/EFForg/rayhunter/discussions/1158>
- <https://github.com/EFForg/rayhunter/discussions/1172>
- <https://github.com/EFForg/rayhunter/discussions/1173>
- <https://github.com/EFForg/rayhunter/discussions/1195>
- <https://untitaker.github.io/rayhunter-discussions-mirror/discussions/866.html>
- <https://untitaker.github.io/rayhunter-discussions-mirror/discussions/842.html>
- <https://untitaker.github.io/rayhunter-discussions-mirror/discussions/768.html>
- <https://untitaker.github.io/rayhunter-discussions-mirror/discussions/669.html>
- <https://untitaker.github.io/rayhunter-discussions-mirror/discussions/528.html>
- <https://untitaker.github.io/rayhunter-discussions-mirror/discussions/791.html>
- <https://efforg.github.io/rayhunter/orbic.html> (surfaced in search; local copy `upstream/doc/orbic.md` at v0.13.0 was used for content)
- <https://dagnazty.github.io/DagShell/installation.html>
- <https://dagnazty.github.io/DagShell/orbic.html>
- <https://raddad87.github.io/RayHunter-Web-Flasher/>
- <https://www.tindie.com/products/stscollective/new-rayhunter-imsi-catcher-stingray-detector>
- <https://linuxconfig.org/rayhunter-tutorial-convert-a-verizon-orbic-speed-rc400l-into-a-stingray-detector> (blocked by Cloudflare; not read)
- <https://www.verizon.com/support/verizon-orbic-speed-mobile-hotspot-update-instructions/> (referenced in #228; not re-fetched)
- Local: `upstream/installer/src/orbic_network.rs` (line 225, v0.13.0), `upstream/doc/orbic.md`
