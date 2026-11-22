# Related Tools: IMSI-Catcher Detection, DIAG Capture, and Lab Rigs

- Retrieved: 2026-11-22
- Scope: tools adjacent to Rayhunter, for positioning Orbic-Rayhunter and choosing references. GitHub
  metadata (licence, last push, archived flag) from the GitHub API on the retrieval date.
- Convention: "unverified" marks claims we could not source.

---

## 1. Detectors

| Tool | Platform | Licence / status | Approach |
|---|---|---|---|
| Rayhunter (EFF) | On-device on Qualcomm MDM9x07-class hotspots (Orbic RC400L origin; also TP-Link M7350/M7310, Moxee, T-Mobile TMOHS1, Wingtech CT2MHS01, UZ801, PinePhone) | GPL-3.0, active (v0.13.0) | Reads LTE RRC/NAS via `/dev/diag`, stores gzip QMDL, runs heuristics (2G downgrade/redirect, IMSI request, null cipher, incomplete SIB). Control plane only per EFF. |
| Crocodile Hunter (EFF) | Linux PC + SDR (LimeSDR, USRP B200, bladeRF x40), built on srsLTE | GPL-3.0, archived (unmaintained since Dec 2022) | Passive 4G eNodeB broadcast survey with GPS/Wigle, flags unusual base stations. Rayhunter's predecessor at EFF. |
| AIMSICD (Android IMSI-Catcher Detector) | Android app | GPL-3.0, repo still receives pushes (2026-10) but the app has long been described as alpha/unmaintained (unverified) | Uses Android telephony APIs (and historically optional root features) to watch cell ID/LAC changes, neighbour lists, silent SMS; 2G-centric. |
| SnoopSnitch (SRLabs) | Rooted Android phones with Qualcomm chipsets | GPL-3.0, last push 2022-05 | Reads baseband DIAG on-device (native helper, unverified detail), detects IMSI catchers, SS7 attacks and silent SMS; GSMmap upload. Also known for Android patch-level checks. |
| SeaGlass (Univ. of Washington, PETS 2017) | Vehicle-mounted sensors: Raspberry Pi + cellular scanning modem + GPS, aggregated in the cloud | Research code at github.com/seaglass-project/seaglass (licence unverified) | City-wide baseline of tower broadcast properties from 15 rideshare vehicles (Seattle, Milwaukee); flags anomalies statistically. GSM-era. |
| cellscan (jcrawfordor) | Vehicle sensor, SeaGlass-inspired | GitHub repo, WIP | Lower-cost SeaGlass-like data collection. |

## 2. DIAG capture and decoding tools

| Tool | Licence / status | How it gets DIAG | Output | Relevance |
|---|---|---|---|---|
| SCAT (fgsect) | GPL-2.0, active | USB DIAG interface via pyUSB, serial, or QMDL/SDM files; Qualcomm, Samsung, HiSilicon (experimental) | GSMTAP to UDP 4729 (control) / 47290 (user plane), pcap; GSMTAPv3 Lua plugin for NR and LTE MAC/PDCP | Reference parser for log-code layouts (`parsers/qualcomm/diagcmd.py`, `diagltelogparser.py`); can decode Rayhunter QMDL after gunzip (unverified with gzip). |
| QCSuper (P1sec) | GPL-3.0, active | `--adb` on rooted Android (`/dev/diag` bridged over ADB), `--usb-modem` serial DIAG, DLF/QMDL input | pcap/GSMTAP, live Wireshark, DLF | Origin of Rayhunter's log-code table and `/dev/diag` notes. |
| MobileInsight (UCLA/Purdue) | README states Apache 2.0 (GitHub shows NOASSERTION); core last push 2024-04 | On-device Android app over `/dev/diag` (rooted), desktop serial monitor, offline QMDL/mi2log replay | Decoded message objects; RRC/EMM/ESM analyzers | Largest open catalogue of Qualcomm log-packet versions. |
| QXDM / QCAT (Qualcomm) | Proprietary | USB DIAG | DLF/ISF/QMDL | Industry reference; not usable in an open project. |
| bkerler/edl | GPL-3.0, active | Sahara/Firehose over EDL `05c6:9008`; also DIAG utilities | Partition read/write | Recovery tooling candidate; needs a working MDM9207 NAND programmer (none confirmed). |
| qualcomm/qdl, qualcomm/qdlrs | BSD-3-Clause, active | EDL flashing | | Official open EDL clients; qdlrs tried unsuccessfully on Moxee (Rayhunter #993). |

## 3. Lab test rigs (controlled RF only)

These run a real LTE network on an SDR. They are listed only as lab rigs for generating known-good and
known-suspicious signalling against our own device inside a shielded enclosure (Faraday box / RF
shield tent) with attenuators and a test PLMN (e.g. 001/01) and a programmable test SIM. Transmitting
on licensed spectrum outside such a setup is illegal in the US without authorization; nothing in this
project transmits over the air.

| Stack | Licence / status | Notes |
|---|---|---|
| srsRAN 4G (srsLTE successor) | AGPL-3.0, active | eNodeB + EPC + UE; easy to script redirections, IMSI/identity requests, null-cipher configs to exercise Rayhunter heuristics. |
| srsRAN Project | archived on GitHub (2026-06) | 5G CU/DU; not relevant to an LTE-only MDM9207. |
| OpenAirInterface (OAI) | OAI Public License v1.1 (GitHub mirror shows NOASSERTION) | 4G/5G RAN + core; heavier than srsRAN. A community "oai-rayhunter" notes repo exists (see section 4). |
| Open5GS / srsEPC | (Open5GS AGPL-3.0; unverified for this note) | Core network for the above. |

## 4. Rayhunter-adjacent repositories on GitHub

From `gh search repos rayhunter --limit 40` and the EFForg/rayhunter fork list (top 40 by stars), read
only. Stars and dates as of retrieval; "stock fork" means the README is unchanged from upstream and no
added purpose is stated.

### 4.1 Forks and ports that add functionality

| Repository | What it adds |
|---|---|
| CyberChuck01/ratcatcher | Fork of v0.13.0 for the Orbic RC400L: on-screen status mascot, one-time pairing code, API-keyed WebSocket streaming cells and alerts, towers list, Wi-Fi scan, themes, PiPack client. |
| mallorybowes/rayhunter (branch `m2100-support`) | Port to Inseego MiFi M2100 5G UW: 24-byte msm-4.14 diag ioctl, multi-radio header stripping, touchscreen UI, panic lockdown, Home Assistant add-on, WASM sandboxed decoder. |
| salam-ai-learning3/rayhunter-enhanced | "Enhanced" fork claiming more cellular data extraction (cell IDs, RSRP/RSRQ/SINR, neighbours), GPS integration, CSV/JSON export. |
| drinkingc0ffee/rayhunter-enhanced-f1 | Enhanced Rayhunter with GPS correlation and advanced analysis (description only; README is upstream text). |
| shark-fi/rayhunter-mudi7 | Port to GL.iNet GL-E5800 (Mudi 7) as an OpenWrt `.ipk`. |
| tschakram/raypager | Not a fork: Rayhunter-inspired detector for GL-E750V2 (Quectel EM050-G, no `/dev/diag`) using `AT+QENG` + OpenCelliD + Blue Merle IMEI rotation. |
| ross-bradley/Rayhunter-MW70VK | Getting Rayhunter running on an Alcatel MW70VK MiFi. |
| m0veax/rayhunter-tplink-m7350 | Historical TP-Link M7350 port, now merged upstream (repo marked obsolete). |
| SpootyMcSpoot/rayhunter_picomesh | Fork name suggests a mesh/Pico integration; README is old upstream text (unverified purpose). |
| holdTheDoorHoid, Utter-Cha0s, rayhunterapp, and most other forks in the top 40 | Stock forks (no stated additions). |

### 4.2 Companion tools

| Repository | What it adds |
|---|---|
| RadDad87/RayHunter-Web-Flasher, RadDad87/RayHunter-Flasher | Interactive installer wizard for the Orbic RC400L with OS auto-detection. |
| zaneprall/rayhunter_bridge | Pushes Rayhunter alerts to Home Assistant. |
| JWhiteUX/orbic-rayhunter | Local push notifications without cellular service. |
| MHSEA/Rayhunter-Tailscale-Telegram | TP-Link M7350 v9 with Tailscale and Telegram alerts. |
| TheRob-N/RayBridge | Orbic companion for PCAP archiving and alerting. |
| untitaker/rayhunter-companion (moved to Codeberg) | Android app to install and manage Orbic/TP-Link devices (discussion #809). |
| UltraSunshine/python-rayhunter | Unofficial Python bindings for the Rayhunter HTTP API. |
| UltraSunshine/rayhunter-utils | Bulk data extraction and maintenance over USB. |
| mjstallard/raygatherer | CLI for Rayhunter. |
| h0bbl3s/rayhunter-gpsd-sync | C client pushing gpsd location to the Rayhunter API. |
| drinkingc0ffee/GPS2REST-ESP32-Orbic | ESP32 GPS push with JWT and `/api/time` clock following. |
| MatejKovacic/rayhuter-watchdog | Watchdog app. |
| salam-ai-learning3/RayhunterWalledGarden, salam-ai-learning3/rayhunter-merge | Walled-garden Wi-Fi module; merged monitoring system with GPS and JWT. |
| South-Lighthouse/fade-rayhunter-backend-server | Central server aggregating measurements from many Rayhunter units. |
| ZeroChaos-/rayhunter-check-image | Docker image for `rayhunter-check`. |
| untitaker/rayhunter-discussions-mirror | Static mirror of EFForg/rayhunter GitHub discussions (used heavily as a source here). |
| stathopulos/orbic | Orbic RC400L utilities: ADB enable, AT command sender, UID-1000 setuid shell (V1.3.0). |
| dagnazty/DagShell | Web "custom firmware" for the Orbic with AT console, TTL/MAC tweaks, a simple IMSI-catcher monitor. |

### 4.3 Data and analysis

| Repository | What it adds |
|---|---|
| otarZAP/rayhunter | LTE NAS analysis pipeline (tshark + Python) and a technical report from six captures. |
| JulianBurns85/rayhunter-threat-analyzer | Automated forensic analysis of Rayhunter captures. |
| ZeroChaos-/rayhunter-traces, brendonla/rayhunterdumps, akrontab/rayhunter-pcaps, drinkingc0ffee/rayhunter-logs | Published QMDL/pcap captures (useful as parser test inputs, subject to their licences). |
| cyberxml/oai-rayhunter | Notes on building OAI (bladeRF) as a Rayhunter lab rig. |
| KidPluto/Rayhunter | Setup notes. |

Name collisions (unrelated): castle-engine/rayhunter (ray tracer), naufalraihan/rayhunters,
russellconstruction9/sdfetre (mirror), dovel-pr17-validation/pr17r30-cargo-rayhunter (test mirror),
rayhunter/rayhunter.github.io. Our own repository (doublegate/Orbic-Rayhunter) also appears in the
search.

## 5. Positioning notes for this project

- No public fork in the survey targets backup/recovery, firmware-version hardening, or DIAG log-code
  extensions (0xB0C1/0xB0C2/security-protected NAS) for the RC400L specifically; ratcatcher is the
  closest Orbic-specific functional fork.
- Where we extend parsing, SCAT and MobileInsight are the reference decoders to diff against;
  QCSuper is the provenance of upstream's constants.
- Lab validation should use srsRAN 4G in a shielded enclosure with a test SIM, never over the air.

## Sources

- <https://github.com/EFForg/rayhunter>
- <https://www.eff.org/deeplinks/2025/03/meet-rayhunter-new-open-source-tool-eff-detect-cellular-spying>
- <https://github.com/EFForg/crocodilehunter>
- <https://github.com/CellularPrivacy/Android-IMSI-Catcher-Detector>
- <https://github.com/srlabs/snoopsnitch>
- <https://opensource.srlabs.de/projects/snoopsnitch>
- <https://seaglass.cs.washington.edu/>
- <https://www.petsymposium.org/popets/2017/popets-2017-0027.pdf>
- <https://www.washington.edu/news/2017/06/02/catching-the-imsi-catchers-seaglass-brings-transparency-to-cell-phone-surveillance/>
- <https://github.com/seaglass-project/seaglass>
- <https://github.com/jcrawfordor/cellscan>
- <https://github.com/fgsect/scat>
- <https://github.com/P1sec/QCSuper>
- <https://github.com/mobile-insight/mobileinsight-core>
- <https://github.com/bkerler/edl>
- <https://github.com/qualcomm/qdl>
- <https://github.com/qualcomm/qdlrs>
- <https://github.com/srsran/srsRAN_4G>
- <https://github.com/srsran/srsRAN_Project>
- <https://github.com/openairinterface/openairinterface5g>
- <https://github.com/CyberChuck01/ratcatcher>
- <https://github.com/mallorybowes/rayhunter>
- <https://github.com/salam-ai-learning3/rayhunter-enhanced>
- <https://github.com/drinkingc0ffee/rayhunter-enhanced-f1>
- <https://github.com/shark-fi/rayhunter-mudi7>
- <https://github.com/tschakram/raypager>
- <https://github.com/ross-bradley/Rayhunter-MW70VK>
- <https://github.com/m0veax/rayhunter-tplink-m7350>
- <https://github.com/SpootyMcSpoot/rayhunter_picomesh>
- <https://github.com/RadDad87/RayHunter-Web-Flasher>
- <https://github.com/zaneprall/rayhunter_bridge>
- <https://github.com/JWhiteUX/orbic-rayhunter>
- <https://github.com/MHSEA/Rayhunter-Tailscale-Telegram>
- <https://github.com/TheRob-N/RayBridge>
- <https://github.com/untitaker/rayhunter-companion>
- <https://github.com/UltraSunshine/python-rayhunter>
- <https://github.com/UltraSunshine/rayhunter-utils>
- <https://github.com/mjstallard/raygatherer>
- <https://github.com/h0bbl3s/rayhunter-gpsd-sync>
- <https://github.com/drinkingc0ffee/GPS2REST-ESP32-Orbic>
- <https://github.com/South-Lighthouse/fade-rayhunter-backend-server>
- <https://github.com/untitaker/rayhunter-discussions-mirror>
- <https://github.com/stathopulos/orbic>
- <https://github.com/dagnazty/DagShell>
- <https://github.com/otarZAP/rayhunter>
- <https://github.com/JulianBurns85/rayhunter-threat-analyzer>
- <https://github.com/ZeroChaos-/rayhunter-traces>
- <https://github.com/cyberxml/oai-rayhunter>
