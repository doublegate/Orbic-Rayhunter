# Security Model

Threat model for Rayhunter on the Orbic RC400L as installed by this project, and the mitigations
mapped to v1.6 tickets (T-OR-081..T-OR-088, [ROADMAP.md](../ROADMAP.md)). Vulnerability reporting:
[SECURITY.md](../SECURITY.md).

## Assets

| Asset | Where | Why it matters |
|---|---|---|
| Captures (QMDL, ndjson, PCAP) | `/data/rayhunter/qmdl/`, host archives | contain IMSI, IMEI, GUTI, cell IDs with timestamps (location history), other subscribers' S-TMSIs in paging, possibly user IP traffic via 0x11EB ([ref-docs/legal-and-ethics.md](../ref-docs/legal-and-ethics.md) section 5.1) |
| GPS records | `<entry>-gps.ndjson` when `gps_mode` is on | precise location history |
| Device backups | `backups/`, off-repo copy | IMEI, NV/RF calibration, keys, Wi-Fi credentials, admin password ([BACKUP-AND-RECOVERY.md](BACKUP-AND-RECOVERY.md)) |
| Stock admin password | device; equals the Wi-Fi password on Verizon units | grants root through the web-UI injection the installer uses |
| Wi-Fi AP key | device | network access to every service below |
| Notification secrets | `ntfy_url` topic, WebDAV password in `config.toml`; Wi-Fi client credentials in `wpa_sta.conf` | topic disclosure lets others read or spoof alerts; WebDAV credentials expose uploaded captures |
| Device integrity | rootfs, init scripts, daemon binary | a tampered daemon can hide detections or exfiltrate captures |
| Detection integrity | analyzers, display, notifications | the user's safety decisions depend on alerts being present and accurate |
| Host | the machine running `orx` and the upstream installer | holds backups and the admin password during install |

## Adversaries

| Adversary | Capability | Goal |
|---|---|---|
| Cell-site simulator operator | RF access; may attempt DoS, downgrade, identity catching | avoid detection, identify the user |
| Hotspot LAN client | any device that joins the AP (shared, guessed or default Wi-Fi key) | read captures, change config, disable analyzers or notifications, gain root |
| Wi-Fi client-mode LAN peer | devices on the upstream network when `wifi_enabled = true` | same as a hotspot client, since the API binds `0.0.0.0` |
| Physical-access attacker | holds the device briefly; USB access | root via USB AT/ADB paths, copy captures |
| Notification or WebDAV operator | receives payloads | learns alert timing; with WebDAV, the captures |
| Supply chain | upstream release, our release, build dependencies, GitHub account | ship a modified daemon |
| Carrier OTA | pushes firmware | remove the install, close the root path, change behavior |
| Operator error | the maintainer or user | data loss, bricked unit, leaked identifiers |

## Attack surface

| Surface | Exposure | Source |
|---|---|---|
| Stock GoAhead web admin (80/443) | login plus command injection in `/action/SetRemoteAccessCfg` gives a root shell to anyone with the admin password; no CVE assigned | [ref-docs/orbic-firmware-v1.2.0.md](../ref-docs/orbic-firmware-v1.2.0.md) section 6 |
| `nc` shell on port 24 | unauthenticated root shell while the listener runs (`busybox nc -ll -p 24 -e /bin/sh`); started by the installer and by `util orbic-shell`; lifetime across reboot TBD (on-device) | [ref-docs/rayhunter-install-orbic.md](../ref-docs/rayhunter-install-orbic.md) section 3.2 |
| `nc` file transfer on port 8081 | unauthenticated write of any file the installer targets while listening; integrity checked only by the installer's `md5sum` | [ref-docs/rayhunter-install-orbic.md](../ref-docs/rayhunter-install-orbic.md) section 3.3 |
| `/bin/rootshell` (USB installer only) | setuid root: any local process becomes root | [ref-docs/rayhunter-upstream-overview.md](../ref-docs/rayhunter-upstream-overview.md) section 1 |
| `AT+SYSCMD` via `atfwd_daemon` | root command execution without authentication over the USB AT interface (PID f601) | [ref-docs/orbic-firmware-v1.2.0.md](../ref-docs/orbic-firmware-v1.2.0.md) section 5 |
| ADB (f601 / f622) | shell over USB; persistent if the USB installer was used | [ref-docs/orbic-rc400l-hardware.md](../ref-docs/orbic-rc400l-hardware.md) section 3 |
| Rayhunter HTTP API on `0.0.0.0:8080` | **no authentication**: download all recordings, delete them, read and write config (including `ntfy_url`, WebDAV, analyzer toggles, `wifi_enabled`), trigger Wi-Fi scans, set clock offset, force display state | [ref-docs/rayhunter-upstream-overview.md](../ref-docs/rayhunter-upstream-overview.md) sections 3.3, 7 |
| Notification and WebDAV egress | plain HTTP POST to a configured URL; topic-in-URL secrecy; WebDAV basic auth | [ref-docs/rayhunter-upstream-overview.md](../ref-docs/rayhunter-upstream-overview.md) section 6 |
| Update checker | polls GitHub `releases/latest` every 6 h when enabled; display only, no install | [ref-docs/rayhunter-upstream-overview.md](../ref-docs/rayhunter-upstream-overview.md) section 6 |
| Install and update path | release zips verified by `.sha256` from the same GitHub release; no signatures; `orx install` will verify `dist/SHA256SUMS` from the same release | [ref-docs/rayhunter-install-orbic.md](../ref-docs/rayhunter-install-orbic.md) section 2 |
| Carrier OTA | update client and hosts unknown; newer builds up to V1.3.2 exist | [ref-docs/orbic-firmware-v1.2.0.md](../ref-docs/orbic-firmware-v1.2.0.md) sections 1-2 |
| Parsers | RRC/NAS/DIAG parsing of attacker-controlled over-the-air input in the daemon | [ref-docs/qualcomm-diag-qmdl.md](../ref-docs/qualcomm-diag-qmdl.md) section 6 |

Important consequence: on the stock firmware, anyone who knows the admin password (by default the
Wi-Fi password) already has root through the vendor web UI, independent of Rayhunter. The
unauthenticated Rayhunter API lowers the bar further: any LAN client can disable detection or
redirect notifications without any password.

## Captured identifiers

- Captures are personal data of the owner and, through paging and possibly 0x11EB, of
  bystanders. Default local; no telemetry; sanitize before sharing (`orx scrub`, T-OR-091).
- Analyzers never write IMSI, IMEI, GUTI or S-TMSI into event messages ([ANALYZERS.md](ANALYZERS.md)).
- 0x11EB is verified on the device (T-OR-019) and dropped from our build if it carries user traffic
  (T-OR-029).
- Captures can be seized or subpoenaed; users at risk should treat the device as a location log
  ([ref-docs/legal-and-ethics.md](../ref-docs/legal-and-ethics.md) section 5.2).

## Mitigations

| Threat | Mitigation | Ticket | Until then |
|---|---|---|---|
| LAN client uses the API | authentication for the API and web UI | T-OR-081 | strong, unique Wi-Fi key; no untrusted clients on the AP |
| API reachable from the Wi-Fi client network | configurable bind address / interface restriction | T-OR-082 | `wifi_enabled = false` |
| Lingering port 24 / 8081 listeners; setuid rootshell | verify no listener remains after install, shell sessions closed by `orx`, no setuid `rootshell` on network installs, removal on USB installs | T-OR-083 | check `nmap -Pn -p 24,8081 192.168.1.1` after every install and reboot; prefer the network installer |
| Admin password equals Wi-Fi key; stock remote-access settings | hardening guidance and checks for the stock admin (distinct admin password if the firmware allows, strong WPA2 key) | T-OR-084 | strong Wi-Fi key |
| Tampered release artifacts | signed releases; `orx` verifies signatures before install | T-OR-085 | verify `SHA256SUMS` and build from source when in doubt |
| Unwanted updates (Rayhunter or carrier) | update checker scope documented, no unattended install, OTA state reported by `orx doctor` | T-OR-086 | `auto_check_updates = false`; OTA block (T-OR-018) |
| Secrets readable on device | restrictive permissions for `config.toml` and `wpa_sta.conf`; secrets not returned by `GET /api/config` | T-OR-087 | do not configure WebDAV; keep the ntfy topic private |
| Model drift | threat-model review and security audit before v2.0.0 | T-OR-088, T-OR-104 | this document |
| Parser crashes on hostile input | upstream fuzzing fixes (v0.9.0); golden and malformed vectors in the harness | T-OR-051 | none beyond upstream |
| Detection suppressed by DoS or downgrade | analyzers for rejects and legacy-RAT steering | T-OR-052, T-OR-057, T-OR-064 | upstream analyzers |
| Bricking or data loss by operator error | backup gate in every write command | T-OR-035 | manual backup gate |

Out of scope: defending against an attacker with prolonged physical access and the test-point EDL
path, and modem baseband compromise. Baseband vulnerabilities found during this work go to
Qualcomm PSIRT and the OEM, not to this public repository
([ref-docs/legal-and-ethics.md](../ref-docs/legal-and-ethics.md) section 6).
