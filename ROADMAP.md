# Roadmap

Planning entry point for Orbic-Rayhunter. Every work item has a stable ticket ID `T-OR-NNN`
minted here; IDs are never reused or renumbered. Cite the ID in branches
(`<type>/T-OR-NNN-slug`), commits, patch bodies and PRs. Current state lives only in
[PROJECT-STATUS.md](PROJECT-STATUS.md); sprint-level plans live in [to-dos/](to-dos/README.md).

Target: Orbic Speed RC400L, firmware `ORB400L_V1.2.0_BVZRT`, upstream Rayhunter pinned at
v0.13.0 (`upstream/`, commit cd462a1).

## Conventions

- Tag column: `[UPSTREAM]` = patch or contribution intended for EFForg/rayhunter, `[LOCAL]` =
  Orbic-only patch, `host` = `orx` or scripts in this repository, `device` = procedure executed
  on the unit, `doc` = documentation only.
- Before starting any `[UPSTREAM]` ticket, search upstream issues and PRs for an equivalent and
  link it in the ticket; do not duplicate upstream work
  ([ref-docs/rayhunter-upstream-overview.md](ref-docs/rayhunter-upstream-overview.md) section 12).
- Every `device` ticket that writes to the unit requires the verified backup from T-OR-017 first.
- Issue numbers `#NNN` refer to EFForg/rayhunter issues unless stated otherwise.
- Until v1.0.0, minor versions mark phase completions ([docs/RELEASE-PROCESS.md](docs/RELEASE-PROCESS.md)).

## Release overview

| Release | Theme | Phase | Tickets |
|---|---|---|---|
| v0.1.0 | Foundation | 0 | T-OR-001 .. T-OR-011 |
| v0.2.0 | Device recon + verified backup | 1 | T-OR-012 .. T-OR-020 |
| v0.3.0 | Stock upstream install via network installer | 2 | T-OR-021 .. T-OR-025 |
| v0.4.0 | Reproducible patched build deployed | 3 | T-OR-026 .. T-OR-031 |
| v0.5.0 | `orx doctor` v2 and device transport | 4 | T-OR-032 .. T-OR-033 |
| v0.6.0 | `orx backup` | 4 | T-OR-034 .. T-OR-035 |
| v0.7.0 | `orx install` and `orx verify` | 4 | T-OR-036 .. T-OR-037 |
| v0.8.0 | `orx pull-logs` and `orx restore` / uninstall | 4 | T-OR-038 .. T-OR-039 |
| v0.9.0 | Retention and purge for disk-full | 4 | T-OR-040 .. T-OR-041 |
| v1.0.0 | Baseline release | 4 | T-OR-042 .. T-OR-044 |
| v1.1.0 | Alerting (host enablement: orx alert status/test) | 5 | T-OR-044 |
| v1.1.1 | `orx alert set` (host-side ntfy_url write) | 5 | T-OR-107 |
| v1.2.0 | Alerting daemon sinks + analyzers | 5 | T-OR-045 .. T-OR-064 |
| v1.2.0 | Analyzers | 5 | T-OR-051 .. T-OR-064 |
| v1.3.0 | Storage and retention | 5 | T-OR-065 .. T-OR-068 |
| v1.4.0 | Performance and battery | 6 | T-OR-069 .. T-OR-074 |
| v1.5.0 | UI/UX | 6 | T-OR-075 .. T-OR-080 |
| v1.6.0 | Security hardening | 6 | T-OR-081 .. T-OR-088 |
| v1.7.0 | Host data pipeline | 7 | T-OR-089 .. T-OR-093 |
| v1.8.0 | Firmware-level work (aggressive tier) | 7 | T-OR-094 .. T-OR-099 |
| v1.9.0 | Upstream sync and 72 h soak | 7 | T-OR-100 .. T-OR-103 |
| v2.0.0 | Stable release | 7 | T-OR-104 .. T-OR-105 |

## v0.1.0 Foundation

Goal: a buildable, lintable, documented overlay repository with upstream pinned and CI defined.

| ID | Ticket | Tag | Status |
|---|---|---|---|
| T-OR-001 | Repository scaffold: GPL-3.0-or-later, overlay layout, upstream submodule pinned to v0.13.0 | host | done |
| T-OR-002 | CI workflows: `ci.yml`, `docs.yml`, `security.yml`, `release.yml`, `upstream-watch.yml` | host | done (authored, not yet run) |
| T-OR-003 | `orx doctor`: USB sysfs detection (f626 normal, f601 debug, 9008 EDL) and TCP probes of :80 and :8080 | host | done |
| T-OR-004 | `scripts/apply-patches.sh`, `scripts/build.sh`, `scripts/sync-upstream.sh`, `scripts/gen-assets.sh` | host | done |
| T-OR-005 | Research corpus in `ref-docs/` | doc | done |
| T-OR-006 | GitHub repository `doublegate/Orbic-Rayhunter` created | host | done |
| T-OR-007 | Specification docs, ROADMAP, PROJECT-STATUS, `to-dos/` | doc | done (uncommitted) |
| T-OR-008 | First commit and push to `main` | host | done |
| T-OR-009 | Branch protection on `main` with required checks | host | done |
| T-OR-010 | First CI run on `main` with every job executing its real gate | host | done |
| T-OR-011 | Tag and release v0.1.0 | host | done |

Exit criteria: T-OR-001..T-OR-011 done; CI green on the tagged commit with no
`continue-on-error` on a blocking gate; `markdownlint-cli2 '**/*.md' '.github/**/*.md'` clean; README points to
PROJECT-STATUS.md.

## v0.2.0 Device recon + verified backup

Goal: know exactly what is on this unit and hold a verified, off-repo backup of every
partition before anything is written. Block OTA updates and settle the 0x11EB privacy question.

| ID | Ticket | Tag | Status |
|---|---|---|---|
| T-OR-012 | Host-side read-only recon: `lsusb -v` per PID, `orx doctor` transcript, stock UI firmware string | device | open |
| T-OR-013 | Root shell access via upstream `installer util orbic-shell`; determine whether `SetRemoteAccessCfg` persists any state | device | open |
| T-OR-014 | On-device read-only inventory per [docs/DEVICE-PROFILE.md](docs/DEVICE-PROFILE.md) checklist | device | open |
| T-OR-015 | Raw backup of every MTD partition, streamed off-device, SHA-256 on both ends | device | open |
| T-OR-016 | File-level archives of `/`, `/data`, `/usrdata`; byte copies of stock `/etc/init.d/*` incl. `misc-daemon` | device | open |
| T-OR-017 | Backup verification: second-pass hash comparison, encrypted off-repo copy, restore-tool inventory | device | open |
| T-OR-018 | Identify the FOTA/OTA client and block it before an active SIM is inserted | device | open |
| T-OR-019 | 0x11EB check: does user IP traffic land in QMDL on this unit | device | open |
| T-OR-020 | Firmware RE triage: `goahead` endpoints, FOTA client, stock vs upstream `misc-daemon` diff | doc | open |
| T-OR-106 | `orx doctor` reads the USB serial from the product string (`_SN:`) when sysfs has no `serial` attribute, as on the RC400L in normal mode (found in T-OR-012) | host | done |

Exit criteria: backup manifest with SHA-256 per image matching on host and device, two
consecutive passes compared, encrypted copy verified on separate media; OTA block in place and
verified across a reboot; 0x11EB result recorded with evidence in DEVICE-PROFILE.md; every
DEVICE-PROFILE.md "TBD (on-device)" row either filled or explicitly deferred with a reason.

## v0.3.0 Stock upstream install via network installer

Goal: upstream v0.13.0 release installed with the default network installer, verified, then
fully removed and reinstalled to prove the restore path.

| ID | Ticket | Tag | Status |
|---|---|---|---|
| T-OR-021 | Install preflight gate: doctor, verified backup, `misc-daemon` diff reviewed, `/data` free space, firmware string | device | open |
| T-OR-022 | Install upstream v0.13.0 release with `installer orbic` (release zip SHA-256 verified) | device | open |
| T-OR-023 | Post-install verification against the [docs/TESTING.md](docs/TESTING.md) smoke list | device | open |
| T-OR-024 | Uninstall/restore rehearsal incl. byte-identical stock `misc-daemon` restore, then reinstall | device | open |
| T-OR-025 | Field report upstream: network installer result on V1.2.0 (no public report exists) | [UPSTREAM] doc | open |

Exit criteria: install log archived; smoke list passes including a reboot; after uninstall,
`sha256sum /etc/init.d/misc-daemon` equals the backup value and no Rayhunter file remains;
reinstall passes the smoke list again.

## v0.4.0 Reproducible patched build deployed

Goal: our own build from `patches/` runs on the device and is distinguishable from upstream's.

| ID | Ticket | Tag | Status |
|---|---|---|---|
| T-OR-026 | Compare unpatched `firmware` build with the upstream v0.13.0 release binary (size, sections, bytes) | host | open |
| T-OR-027 | First `[UPSTREAM]` patch: `doc/heuristics.md` fixes (SIB6 is UTRA, IMSI Requested heading version) | [UPSTREAM] | open |
| T-OR-028 | Build identification: version suffix visible in `/api/system-stats` | [LOCAL] | open |
| T-OR-029 | Conditional on T-OR-019: drop 0x11EB from the DIAG subscription in our build | [LOCAL] | open |
| T-OR-030 | Deploy `dist/` build to the device manually, verify, roll back to the release binary | device | open |
| T-OR-031 | Release pipeline dry run; tag v0.4.0 | host | open |

Exit criteria: CI builds the patched series; the device reports our build identifier; rollback
to the release binary verified; reproducibility result recorded in
[docs/PERFORMANCE.md](docs/PERFORMANCE.md).

## v0.5.0 .. v0.9.0 orx tooling

Goal: every manual procedure from Phases 1-3 is a fail-closed `orx` subcommand.

| ID | Release | Ticket | Tag | Status |
|---|---|---|---|---|
| T-OR-032 | v0.5.0 | `orx doctor` v2: f622 and all known PIDs, interface list, firmware string, Rayhunter `/api/system-stats`, JSON output | host | open |
| T-OR-033 | v0.5.0 | Device transport layer: shell session and checksummed file transfer (network shell; ADB only with explicit serial) | host | open |
| T-OR-034 | v0.6.0 | `orx backup`: stream every MTD partition and file-level archives, SHA-256 both ends, manifest | host | open |
| T-OR-035 | v0.6.0 | `orx backup verify` and the backup gate consumed by every write command | host | open |
| T-OR-036 | v0.7.0 | `orx install`: run upstream `installer orbic`, then install `dist/` daemon and our config, refuse without verified backup | host | open |
| T-OR-037 | v1.0.4 | `orx check`: device health from :8080 (version, disk/mem/battery, recording) + port posture | host | done |
| T-OR-038 | v1.0.3 | `orx pull-logs`: download recordings (QMDL, ndjson; GPS follow-up) with manifest and hashes | host | done |
| T-OR-039 | v0.8.0 | `orx restore` / `orx uninstall`: remove Rayhunter, restore stock `misc-daemon` and other replaced files, verify against backup | host | open |
| T-OR-040 | v1.0.5 | `orx purge`: delete completed recordings (dry-run default, --apply); keeps active | host | done |
| T-OR-041 | v1.0.6 | `orx rescue`: free `/data` over the network shell when the daemon and web UI are down (#1192 failure mode) | host | done |

Exit criteria per release: subcommand has unit tests on fake sysfs/fake shell, fails closed on
missing backup or hash mismatch, and is documented in [docs/INSTALL.md](docs/INSTALL.md) or
[docs/BACKUP-AND-RECOVERY.md](docs/BACKUP-AND-RECOVERY.md) in the same change.

## v1.0.0 Baseline release

Goal: a documented, repeatable install of our build on V1.2.0 with a measured baseline.

| ID | Ticket | Tag | Status |
|---|---|---|---|
| T-OR-042 | End-to-end rehearsal from restored stock state using only `orx` and the docs | device | open |
| T-OR-043 | First performance baseline (all [docs/PERFORMANCE.md](docs/PERFORMANCE.md) metrics) | device | open |
| T-OR-044 | v1.0.0 docs sweep, security note, release | doc | open |

Exit criteria: [docs/INSTALL.md](docs/INSTALL.md) followed verbatim succeeds; baseline table
filled; release checklist passes.

## v1.1.0 Alerting

Goal: extend upstream's ntfy path (plain `POST` body, no headers) without duplicating it.
Upstream v0.13.0 ALREADY sends a `Warning` notification on analyzer detections (and `LowBattery`)
to `ntfy_url` with per-type debounce and backoff retry, so v1.1.0 is host-side ENABLEMENT of that
existing path; the sink-type and queue extensions below (all `[UPSTREAM]`) move to v1.2.0 per the
2026-11-25 decision (enable now, patch the daemon after).

| ID | Ticket | Tag | Status |
|---|---|---|---|
| T-OR-044 | `orx alert status`/`test`: surface the daemon's ntfy config and send a test via `:8080` (host-side enablement; `set` is T-OR-107/v1.1.1) | host | done (v1.1.0) |
| T-OR-107 | `orx alert set --ntfy-url <url>\|--clear [--apply]`: write the daemon's `ntfy_url` via a faithful read-modify-write of `/api/config` (host-side; dry-run unless `--apply`) | host | done (v1.1.1) |
| T-OR-045 | ntfy title, priority and tags headers mapped from event severity | [UPSTREAM] (v1.2.0) | done (patch 0001) |
| T-OR-046 | Notification endpoint authentication (bearer token or basic auth) | [UPSTREAM] | done (patch 0002) |
| T-OR-047 | Generic webhook sink with a JSON body (type, message, timestamp; no identifiers) | [UPSTREAM] | done (patch 0003) |
| T-OR-048 | MQTT sink (discuss with upstream first; dependency weight on armv7) | [UPSTREAM] | deferred (post-v1.2.0) |
| T-OR-049 | Gotify sink | [UPSTREAM] | done (patch 0004) |
| T-OR-050 | Persistent retry queue for notifications that survives loss of backhaul and daemon restart | [UPSTREAM] | deferred (post-v1.2.0; overlaps v1.3.0 storage) |

Exit criteria: each sink tested against a local test server; no IMSI/IMEI/GUTI in any payload;
existing `ntfy_url` behavior unchanged when new keys are absent. Status: the four scoped sinks
(T-OR-045/046/047/049) are `patches/0001-0004`, host-validated on upstream's own gates; MQTT and the
persistent queue are deferred by the 2026-11-25 scope decision. On-device validation is batched with
the analyzers phase (firmware rebuild + reinstall).

## v1.2.0 Analyzers

Goal: close the published blind spots, ranked in
[ref-docs/lte-imsi-catcher-detection.md](ref-docs/lte-imsi-catcher-detection.md) section 5.
Details: [docs/ANALYZERS.md](docs/ANALYZERS.md).

| ID | Ticket | Tag | Status |
|---|---|---|---|
| T-OR-051 | Analyzer test harness: synthetic RRC/NAS vectors, golden-output checks, capture scrubber | [UPSTREAM] | landed at patch level (patch 0006: `nas_plain`/`packet_time`/`collect_events`/`emm_message`); the capture scrubber needs on-device captures, so it is deferred to T-OR-061 |
| T-OR-052 | Unauthenticated NAS Reject with identity-forcing or service-denying cause (rank 1, Devilray blind spot) | [UPSTREAM] | landed (patch 0007, `nas_reject_identity`; keys on cause #9 Medium / #10 Low; unprotected + IMSI-re-attach confirmation deferred to parser support; off by default pending T-OR-061 calibration) |
| T-OR-053 | Authentication failure / authentication reject (rank 2) | [UPSTREAM] | landed (patch 0008, `auth_failure`; #20 High / #21 Low / #26 Medium / reject High; stateless; off by default pending T-OR-061; stateful suppression T-OR-109) |
| T-OR-054 | Own permanent-identity disclosure, incl. `epsid` parsing gap (rank 3) | [UPSTREAM] | **landed** (patch 0013, `identity_disclosure`): Identity Response with IMSI (Medium) / IMEI/IMEISV (High), Attach Request with IMSI-type EPS mobile identity (Low); stateless, keys on message + identity type, never digits. Off by default pending calibration. Request side stays with upstream `imsi_requested`; real-capture firing needs an active SIM |
| T-OR-055 | Null integrity (EIA0) and security never activated (rank 4) | [UPSTREAM] | landed (NAS half: patch 0009, `null_integrity`; EIA0 in NAS Security Mode Command -> High; stateless; off by default pending T-OR-061). RRC-SMC-EIA0 and DRB-before-SMC parts need RRC vectors -- deferred to T-OR-111 |
| T-OR-056 | IMSI paging, IMSI held as a salted in-memory hash (rank 5) | [UPSTREAM] | open -- keys on PCCH (RRC), so blocked on T-OR-112 RRC vectors; the "is it our IMSI" compare also needs the device IMSI (AT) from the T-OR-061 session |
| T-OR-057 | Dedicated-priority and sunset-RAT downgrade, SIB8 (rank 6) | [UPSTREAM] | **landed** (patch 0014, `rat_downgrade`): `MobilityFromEUTRACommand` CellChangeOrder or Handover to GERAN (High), Handover to UTRA (Informational), Handover/e-CSFB to CDMA2000 (Low), NR/E-UTRA ignored; SIB8 broadcast (Low). Vectors pycrate-encoded, telcom_parser-decoded. Follow-up tracked as T-OR-115 (`idleModeMobilityControlInfo` dedicated priorities + per-MCC sunset policy table) |
| T-OR-058 | Measurement plumbing: ML1 RSRP/PCI and RACH timing advance into analyzer input (rank 8 prerequisite) | [UPSTREAM] | open |
| T-OR-059 | Cell identity consistency, recording-scoped (rank 7) | [UPSTREAM] | open |
| T-OR-060 | Recording-level identity-exposure summary (rank 10) | [UPSTREAM] | open |
| T-OR-061 | Negative baselines on US carriers and severity calibration | device | open |
| T-OR-062 | Stretch: SIB reselection-parameter anomaly features (rank 9) | [UPSTREAM] | open |
| T-OR-063 | Stretch: emergency-alert spoofing context (rank 11) | [UPSTREAM] | open |
| T-OR-064 | Stretch: RRC Connection Reject DoS context (rank 12) | [UPSTREAM] | open |
| T-OR-108 | T-OR-052 precision follow-ups: confirm the reject was integrity-unprotected (NAS security-header type -- needs DIAG/parser verification, overlaps T-OR-061) and correlate a following IMSI re-attach (needs T-OR-110 mobile-identity parsing); fold in the broader cause tiers (#3/#6/#7, #11-#15/#22/#42) post-calibration | [UPSTREAM] | open (identity-correlation part now buildable: T-OR-110 landed in patch 0012) |
| T-OR-111 | T-OR-055 follow-ups: (a) the "security never activated" half -- flag an RRC Security Mode Command selecting EIA0, and a DRB setup or measurement reconfiguration with no prior Security Mode Command (needs RRC/ASN.1 synthetic vectors in the harness); (b) stateful emergency suppression -- downgrade/suppress an EIA0 Security Mode Command that follows an emergency `AttachRequest` (`eps_attach_type` = EPSEmergencyAttach; decodable, not blocked on T-OR-110), since EIA0 is legitimate there. Until (b) lands, the stateless detector accepts emergency calls as a known false positive (off by default) | [UPSTREAM] | open |
| T-OR-112 | **Enabling: RRC golden-vector test harness.** Upstream Rayhunter does not unit-test its RRC analyzers (connection_redirect_downgrade, incomplete_sib, priority_2g_downgrade have no tests), so there was no pattern for building an RRC `LteInformationElement` in a test, and hand-encoding UPER PDUs blind is error-prone. Added `test_util` builders (`lte_rrc`/`bcch_dl_sch`/`dl_dcch`) that wrap raw RRC octets through the live `lte_rrc` decode path, seeded with identifier-free golden vectors captured from the device (BCCH-DL-SCH SystemInformation + DL-DCCH DLInformationTransfer; real PCCH paging carries an S-TMSI so T-OR-056 builds synthetic paging). Unblocks T-OR-056, T-OR-057, and the RRC half of T-OR-111 | [UPSTREAM] | landed (patch 0011) |
| T-OR-110 | **Enabling: `pycrate-rs` EPS mobile-identity IE parsing.** The generated `ID`/`EPSID` structs are empty, so identity type and digits (IMSI/IMEI/IMEISV/GUTI; type nibble + BCD, TS 24.301 9.9.3.12) cannot be read. Hand-add the IE to those structs (or fix the generator) in EFForg/pycrate-rs, bump the Cargo.lock pin, and validate against real DIAG captures. Unblocks T-OR-054 and the identity-correlation part of T-OR-108; also sharpens T-OR-056 (compare paged identity to the SIM IMSI) | [UPSTREAM] (EFForg/pycrate-rs) | **landed** (patch 0012, pinned to `doublegate/pycrate-rs@77b1cc8` until upstreamed): hand-written `mobile_identity.rs` + generator emits type aliases. Validated against real GUTI/M-TMSI/IMEISV vectors and synthetic IMSI/IMEI vectors cross-checked with Python pycrate and Wireshark; a real-capture IMSI/IMEI check awaits an active SIM. Upstream submission to EFForg is the maintainer's call (EFF requires a human-written description and AI disclosure) |
| T-OR-109 | T-OR-053 stateful follow-up: suppress a benign Authentication Failure #21 (SQN resync) that is followed within milliseconds by a successful re-authentication, and raise its severity when no re-auth follows; likewise scope Authentication Reject to before a successful authentication; deduplicate repeated auth-failure/reject on a retry storm | [UPSTREAM] | open |
| T-OR-114 | **Tooling: `scripts/orbic-open-shell.py`.** The documented way to re-open the non-persistent `:24` root shell after a reboot (`installer util orbic-start-telnet --admin-password <PASS>`) puts the admin password in the host process list. Added a host tool that performs the identical `SetRemoteAccessCfg` injection but reads the password from the mode-600 file via the self-tested `orbic-ui-read.py` login machinery, so it never reaches argv. `--selftest` asserts the payload still matches upstream's exploit string. Host-only (this repo's tooling, not a Rayhunter patch) | host | landed |
| T-OR-115 | T-OR-057 follow-up: dedicated-priority downgrade via `idleModeMobilityControlInfo` in RRC Connection Release (per-UE reselection priorities favoring a legacy RAT), and a per-MCC "sunset market" policy table to raise SIB8/CDMA2000 and legacy reselection to High where the RAT is decommissioned | [UPSTREAM] | open |
| T-OR-113 | **Tooling: `rayhunter-check --config <file>`.** The check tool hardcoded `AnalyzerConfig::default()`, so offline re-analysis silently ran only the default-on analyzers and could exercise no default-off detector (ours or a future one). Added a `--config` flag that loads a config's `[analyzers]` table, so archived QMDL can be re-scored with the v1.2.0 detectors. This is the reproducible calibration path (the daemon's live NDJSON has no per-message denominator); drives `scripts/calib-*`. Upstreamable | [UPSTREAM] | landed (patch 0010) |

Exit criteria: T-OR-051..T-OR-060 merged into the series with golden vectors; each analyzer
off-by-default until T-OR-061 shows an acceptable false-positive rate; upstream PR opened for
each `[UPSTREAM]` analyzer.

## v1.3.0 Storage and retention

| ID | Ticket | Tag | Status |
|---|---|---|---|
| T-OR-065 | On-device retention: size and age caps, oldest unflagged recordings deleted first | [UPSTREAM] | open |
| T-OR-066 | Disk-full guard root cause (#1192) and `rayhunter.log` rotation | [UPSTREAM] | open |
| T-OR-067 | Configurable DIAG log-code subscription (privacy and storage minimization) | [UPSTREAM] | open |
| T-OR-068 | Measured storage growth per hour and calibrated default caps in our config | device | open |

Exit criteria: a fill test to the configured cap leaves the hotspot and web UI working; measured
growth rates recorded in PERFORMANCE.md.

## v1.4.0 Performance and battery

Baseline first: no optimization lands without a before/after number.

| ID | Ticket | Tag | Status |
|---|---|---|---|
| T-OR-069 | Re-baseline all PERFORMANCE.md metrics on the v1.3 build | device | open |
| T-OR-070 | CPU and RSS profile of the daemon under idle and active LTE | device | open |
| T-OR-071 | QMDL write path and flash wear: bytes written per hour, write batching | [UPSTREAM] | open |
| T-OR-072 | Battery drain per hour and display power by `ui_level` | device | open |
| T-OR-073 | Boot-to-recording time | device | open |
| T-OR-074 | Daemon supervision and respawn (upstream has none), with measured restart cost | [LOCAL] | open |

Exit criteria: every change carries a before/after row; no regression in detection on golden
vectors.

## v1.5.0 UI/UX

| ID | Ticket | Tag | Status |
|---|---|---|---|
| T-OR-075 | Keep-screen-on option (#916, #539) | [UPSTREAM] | open |
| T-OR-076 | Do not show recording state while booted in charge mode (#1160) | [UPSTREAM] | open |
| T-OR-077 | Web UI: per-analyzer explanations and false-positive guidance for new analyzers | [UPSTREAM] | open |
| T-OR-078 | Web UI: WebDAV and notification sink configuration (today config-file only) | [UPSTREAM] | open |
| T-OR-079 | `orx status`: host-side live view of device state via the API | host | open |
| T-OR-080 | Framebuffer geometry probed instead of hardcoded 128x128 | [UPSTREAM] | open |

## v1.6.0 Security hardening

Threat model: [docs/SECURITY-MODEL.md](docs/SECURITY-MODEL.md).

| ID | Ticket | Tag | Status |
|---|---|---|---|
| T-OR-081 | HTTP API and web UI authentication | [UPSTREAM] | open |
| T-OR-082 | Configurable bind address and interface restriction (today `0.0.0.0:8080`) | [UPSTREAM] | open |
| T-OR-083 | Remove nc/rootshell exposure: no lingering port 24 or 8081 listener, no setuid `/bin/rootshell` | [LOCAL] host | open |
| T-OR-084 | Stock web admin hardening: admin password, Wi-Fi key, remote-access settings | doc device | open |
| T-OR-085 | Signed release artifacts and `orx` signature verification before install | host | open |
| T-OR-086 | Update path hardening: update checker scope, no unattended install, OTA state check in `orx doctor` | host | open |
| T-OR-087 | Secrets at rest on device: permissions of `config.toml` (ntfy topic, WebDAV password) and `wpa_sta.conf` | [UPSTREAM] | open |
| T-OR-088 | Threat-model review and pre-release security audit | doc | open |

## v1.7.0 Host data pipeline

| ID | Ticket | Tag | Status |
|---|---|---|---|
| T-OR-089 | Incremental `orx pull-logs` sync with a local archive index | host | open |
| T-OR-090 | Host batch re-analysis with `rayhunter-check` over the archive | host | open |
| T-OR-091 | `orx scrub`: replace IMSI/IMEI/GUTI/S-TMSI, coarsen cell IDs and GPS, keep message structure | host | open |
| T-OR-092 | Reports and sanitized share bundles (for EFF submissions) | host | open |
| T-OR-093 | Cross-decoding check against SCAT/MobileInsight for parser regressions | host | open |

## v1.8.0 Firmware-level work (aggressive tier)

Only after T-OR-017; every write has a documented restore. Receive-only rules unchanged.

| ID | Ticket | Tag | Status |
|---|---|---|---|
| T-OR-094 | Boot chain RE (read-only): `aboot`, `boot`, kernel cmdline, test-key signing | doc | open |
| T-OR-095 | Replace whole-file `misc-daemon` overwrite with a minimal boot hook | [LOCAL] | open |
| T-OR-096 | Permanent OTA suppression with a documented reversal | device | open |
| T-OR-097 | Disable unneeded stock services, each measured for RAM/CPU/battery effect | device | open |
| T-OR-098 | EDL/firehose recovery research (no NAND writes until a loader is proven read-only first) | doc | open |
| T-OR-099 | `diagchar` driver RE: ioctl conventions, mask sharing with other DIAG clients | doc | open |

## v1.9.0 Upstream sync and 72 h soak

| ID | Ticket | Tag | Status |
|---|---|---|---|
| T-OR-100 | Rebase the series onto the latest upstream tag | host | open |
| T-OR-101 | Drop `[UPSTREAM]` patches merged upstream; record in CHANGELOG | host | open |
| T-OR-102 | 72 h continuous recording soak with PERFORMANCE.md metrics sampled hourly | device | open |
| T-OR-103 | Release candidate v2.0.0-rc.1 with full uninstall/restore rehearsal | device | open |

Exit criteria: soak completes with no daemon exit, no storage exhaustion, hotspot reachable
throughout.

## v2.0.0 Stable release

| ID | Ticket | Tag | Status |
|---|---|---|---|
| T-OR-104 | Final security audit and documentation sweep | doc | open |
| T-OR-105 | Release v2.0.0 | host | open |

Exit criteria: [docs/RELEASE-PROCESS.md](docs/RELEASE-PROCESS.md) checklist passes; all
`[UPSTREAM]` tickets either merged upstream, open as upstream PRs, or explicitly declined with
the reason recorded.
