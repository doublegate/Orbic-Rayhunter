# Project Status

Single source of truth for the current state of Orbic-Rayhunter. Other files point here instead
of restating status. Updated at every sprint and phase boundary.

- Last updated: 2026-11-28 (v1.2.0 WIP -- sinks + hardening + harness + NAS-Reject + auth-failure + null-integrity + identity-disclosure + inter-RAT-downgrade detectors, patches 0001-0014; T-OR-110 mobile-identity parsing landed (patch 0012, pycrate-rs fork pin), unblocking T-OR-054; T-OR-061 device session started -- patched daemon installed (patches 0001-0009), detectors live; host tooling adds rayhunter-check --config (T-OR-113, patch 0010) + RRC golden-vector harness (T-OR-112, patch 0011); reproducible 0-FP baseline over ~614k messages, but the built-in Verizon SIM is not activated, so this is broadcast-only traffic (every attach refused with EMM cause #7; no Auth/SMC/Identity NAS) and the NAS detectors need an active SIM to calibrate meaningfully)
- Version: 1.1.1 (`Cargo.toml` `workspace.package.version = "1.1.1"`)
- Upstream pin: EFForg/rayhunter v0.13.0 (cd462a1)
- Target: Orbic RC400L, `ORB400L_V1.2.0_BVZRT`

## Executive summary

Phase 0 (Foundation, v0.1.0) is complete except its release tag (T-OR-011). The overlay layout,
upstream submodule, `orx doctor`, build scripts, CI, research corpus, specification docs, roadmap
and phase plans are on `main`, which is governed by a repository ruleset and now runs the jailed
Antigravity reviewer (#2). Phase 1 has started: the Sprint 1.1 host-side recon (T-OR-012) ran on
the unit on 2026-11-23, read-only, and its results are in
[docs/DEVICE-PROFILE.md](docs/DEVICE-PROFILE.md). Only the UI's firmware string is left, and it
needs the admin login. Rayhunter is not installed and no device backup exists. Next: Sprint 1.2
(network shell, read-only inventory), which changes device state and so starts on the owner's
go-ahead. The stock UI is not to be opened in a browser until the OTA client is blocked
(T-OR-018), because its page load triggers an update check.

## Phase matrix

| Phase | Release(s) | Theme | Status | Plan |
|---|---|---|---|---|
| 0 | v0.1.0 | Foundation | **released 2026-11-23** (T-OR-001..T-OR-011 done) | [to-dos/PHASE-0-foundation.md](to-dos/PHASE-0-foundation.md) |
| 1 | v0.2.0 | Device recon + verified backup + OTA block | **released 2026-11-23** (recon, verified backup, off-repo copy, reversible OTA block; empirical 0x11EB deferred -- no active plan) | [to-dos/PHASE-1-recon-backup.md](to-dos/PHASE-1-recon-backup.md) |
| 2 | v0.3.0 | Stock upstream install | installed & validated on-device 2026-11-23 (web UI, recording, reboot-persistent, rayhunter-check); released 2026-11-23 | [to-dos/PHASE-2-stock-install.md](to-dos/PHASE-2-stock-install.md) |
| 3 | v0.4.0 | Reproducible patched build | **released 2026-11-23** (firmware build reproduces upstream to within 384 B; releases now ship it) | [to-dos/PHASE-3-reproducible-build.md](to-dos/PHASE-3-reproducible-build.md) |
| 4 | v0.5.0..v1.0.0 | orx tooling + v1.0.0 baseline | **v1.0.0 released 2026-11-23** (install milestone: our build installed + validated on the unit); core orx ops done (doctor, transport, backup, verify, install, uninstall); `pull-logs`/`check`/`purge` remain as Phase 4 follow-ups | [to-dos/PHASE-4-orx-tooling.md](to-dos/PHASE-4-orx-tooling.md) |
| 5 | v1.1.0..v1.3.0 | Alerting, analyzers, storage | in progress (v1.1.0/v1.1.1 `orx alert` released; v1.2.0 daemon notification sinks + review hardening done at patch level -- `patches/0001-0005`, host-validated; analyzers + on-device validation remain) | [to-dos/PHASE-5-v1.1-v1.3-features.md](to-dos/PHASE-5-v1.1-v1.3-features.md) |
| 6 | v1.4.0..v1.6.0 | Performance, UI/UX, security | not started | [to-dos/PHASE-6-v1.4-v1.6-performance-hardening.md](to-dos/PHASE-6-v1.4-v1.6-performance-hardening.md) |
| 7 | v1.7.0..v2.0.0 | Host pipeline, firmware, soak, release | not started | [to-dos/PHASE-7-v1.7-v2.0-pipeline-firmware-release.md](to-dos/PHASE-7-v1.7-v2.0-pipeline-firmware-release.md) |

## Capability matrix

| Capability | Status | Ticket | Notes |
|---|---|---|---|
| Upstream pinned as submodule | present | T-OR-001 | v0.13.0 |
| Patch series pipeline | present, series empty | T-OR-004 | `scripts/apply-patches.sh` |
| armv7 daemon build | present, built locally and in CI | T-OR-004 | `scripts/build.sh devel` produced `dist/` |
| `orx doctor` | present | T-OR-003, T-OR-106 | USB f626/f601/9008 with the serial read from the `_SN:` product suffix, TCP :80 and :8080; f622 not classified |
| CI workflows | green on `main` | T-OR-002, T-OR-010 | ci, docs, security, upstream-watch; release untested until a tag |
| Specification docs | present | T-OR-007 | `docs/` |
| Device profile | host-side and on-device facts measured (partitions, UBI, mounts, tools, processes, sockets) | T-OR-012, T-OR-013, T-OR-014 | [docs/DEVICE-PROFILE.md](docs/DEVICE-PROFILE.md) |
| Verified device backup | on-device `tar` archives of every writable fs captured and verified (`backups/2026-11-23-V1.2.0/fs/`, two-pass + tar-exit-status + readable); encrypted off-repo copy on the DS-1621+ (GPG to the owner's key) | T-OR-015..T-OR-017 done | gates every device write |
| OTA block | applied & reboot-persistent (oma_dm/dmclient/mdm-daemon disabled) | T-OR-018 | reversible; undo manifest in backups/ |
| Rayhunter on device | **our reproducible build installed & validated** (reboot-persistent, capturing; device-side SHA-256 verified at install) | T-OR-022, T-OR-036 | replaced upstream v0.13.0 on 2026-11-23 |
| `orx` ops: doctor/backup/verify/install/uninstall/pull-logs/check/purge/rescue/alert | present (zero-dep, 136 tests, backup-gated) | T-OR-032..T-OR-036, T-OR-037 (check), T-OR-038 (pull-logs), T-OR-039, T-OR-040 (purge), T-OR-041 (rescue), T-OR-044 (alert status/test), T-OR-107 (alert set) | rescue is read-only (destructive `--apply` = T-OR-041b); retention policy (T-OR-040b) remains |
| New analyzers | harness (0006, T-OR-051) + 3 NAS detectors landed: NAS Reject identity-forcing (0007, T-OR-052), auth failure/reject (0008, T-OR-053), null integrity EIA0 (0009, T-OR-055). **Off-device NAS-decodable tranche complete.** Landed: T-OR-054 (identity disclosure, 0013), T-OR-057 (inter-RAT downgrade, 0014). Remaining: T-OR-056 (IMSI paging), T-OR-058 (ML1 plumbing), T-OR-059 (cell-id state) | T-OR-051..T-OR-064 | detectors ship OFF by default; enabling needs T-OR-061 on-device FP calibration |

## Metrics snapshot

| Metric | Value | Source |
|---|---|---|
| Commits | see `git rev-list --count main` (a count here drifts with every merge) | `git log` |
| Tickets | 115 minted; done: T-OR-001..T-OR-022, T-OR-030, T-OR-032..T-OR-041, T-OR-044, T-OR-045, T-OR-046, T-OR-047, T-OR-049, T-OR-051, T-OR-052, T-OR-053, T-OR-055, T-OR-106, T-OR-107, T-OR-110, T-OR-112, T-OR-113, T-OR-054, T-OR-057 (v1.2.0 sinks 045/046/047/049 + analyzer harness 051 + detectors 052/053/055 + check --config 113 + RRC harness 112 + mobile-identity parsing 110 done at patch level, on-device validation/calibration pending; 0x11EB-empirical part of T-OR-019, MQTT T-OR-048, queue T-OR-050, T-OR-054 + T-OR-057 landed (0013/0014); T-OR-056 has the T-OR-112 RRC harness but needs detector impl + device IMSI (056), analyzers T-OR-058..064, T-OR-052 precision follow-ups T-OR-108, T-OR-053 stateful follow-up T-OR-109, T-OR-055 RRC/emergency follow-up T-OR-111, rescue delete T-OR-041b, retention T-OR-040b open) | [ROADMAP.md](ROADMAP.md) |
| Patches in series | 14 (`[UPSTREAM]` notification sinks 0001-0004 + review hardening 0005 + analyzer test harness 0006 + NAS Reject detector 0007 + auth-failure detector 0008 + null-integrity detector 0009 + check --config 0010 + RRC harness 0011 + mobile-identity parsing via pycrate-rs fork pin 0012 + identity-disclosure detector 0013 + inter-RAT downgrade detector 0014, T-OR-045/046/047/049/051/052/053/055/113/112/110/054/057) | `patches/series` |
| Host crates | 1 (`orx-cli`) | `Cargo.toml` workspace members |
| Host unit tests | 136 orx-cli tests; 4 Python tool self-tests | `cargo test` (local and CI) |
| CI runs | `ci` run 45041852700 green (the 8 checks required at the time passed; the `main` ruleset now requires 11, see docs/CI.md) | GitHub Actions on `b982d00` |
| Latest release | v1.1.1 (2026-11-25, via PR) | `gh release list` |
| Local devel daemon build | 16291376 bytes, unstripped, SHA-256 `1bad88b8...a091c` | `ls -l dist/`, `dist/SHA256SUMS` (2026-11-22) |
| Device firmware | `ORB400L_V1.2.0_BVZRT` | [docs/DEVICE-PROFILE.md](docs/DEVICE-PROFILE.md) |
| Device backup | verified (`backups/2026-11-23-V1.2.0/fs/`) + encrypted off-repo copy | `backups/` |
| Performance baseline | none | [docs/PERFORMANCE.md](docs/PERFORMANCE.md) |

## Timeline

| Date | Event |
|---|---|
| 2026-09-16 | Upstream Rayhunter v0.13.0 released (pinned) |
| 2026-11-22 | Research corpus retrieved (`ref-docs/`), scaffold, `orx doctor`, CI definitions, first local devel build; GitHub repository created |
| 2026-11-22 | Specification docs, ROADMAP, PROJECT-STATUS and `to-dos/` written (Sprint 0.2) |
| 2026-11-22 | First commit and push (`12a7679`); CI fix for script modes and workflow input hardening (`b982d00`); all CI green; branch protection on `main` |
| 2026-11-23 | Releases v0.1.0 through v0.8.0; our reproducible build installed on the unit via `orx install` and v1.0.0 cut (documented, reproducible, reversible install validated on ORB400L_V1.2.0_BVZRT) |

## Known issues and risks

| Risk | Impact | Mitigation |
|---|---|---|
| No software unbrick path (EDL via test point only, no loader) | a bad write can permanently brick the unit | verified backup gate before every write ([docs/BACKUP-AND-RECOVERY.md](docs/BACKUP-AND-RECOVERY.md)) |
| No public report of the current network installer on V1.2.0 | install may fail; fallback `orbic-usb` has bricked units | Phase 2 rehearsal with restore; USB only with warnings |
| A root shell is needed to take the backup | the first device action precedes the backup | use the least invasive network shell; check what it persists (T-OR-013) |
| Verizon OTA (newer builds up to V1.3.2) with an active SIM | may remove the install or close the root path; not reversible | OTA block before the SIM goes in (T-OR-018) |
| Unauthenticated Rayhunter API on `0.0.0.0:8080` | any LAN client can read captures and change config | strong Wi-Fi key, client mode off; T-OR-081, T-OR-082 |
| 0x11EB may record hotspot users' IP traffic | privacy and wiretap exposure | verify on device (T-OR-019), drop in our build if so (T-OR-029) |
| `/data` full wedges daemon and hotspot (#1192) | loss of access | raised free-space thresholds; retention (T-OR-040, T-OR-065) |
| Upstream replaces the stock `misc-daemon` wholesale | stock boot steps on V1.2.0 could be lost | diff before install (T-OR-021); minimal hook later (T-OR-095) |
| Rayhunter misses NAS-Reject IMSI catching (Devilray) | false sense of safety | T-OR-052 |
| `release.yml` has never run | the first tag may fail to publish | dry-run with `workflow_dispatch` on a test tag before T-OR-011 |

## Prioritized remaining work

1. T-OR-011 tag v0.1.0 (rename CHANGELOG `[Unreleased]` to `[0.1.0]`).
2. T-OR-012..T-OR-014 read-only recon.
3. T-OR-015..T-OR-017 verified backup with an encrypted off-repo copy.
4. T-OR-018 OTA block, then T-OR-019 0x11EB check with the active SIM.
5. Phase 2 install and restore rehearsal (T-OR-021..T-OR-024).

## Next steps

v1.2.0 analyzer phase: the off-device, NAS-decodable detector tranche is complete (harness + 3
detectors landed on `main`, patches 0006-0009, all OFF by default; check `--config` patch 0010). The **T-OR-061 device session
is in progress** (user present):

- DONE: patched daemon (patches 0001-0009) built and installed on the device over a verified
  checksummed backup; all 3 detectors enabled on-device and live in the capture pipeline.
- DONE: calibration tooling (`scripts/calib-run.sh`, `scripts/calib-snapshot.py`,
  `scripts/calib-analyzers.toml`) + `rayhunter-check --config` (T-OR-113, patch 0010) -> a
  **reproducible** baseline from committed code: 0 detector firings over ~614k messages (every
  message re-scored offline with the detectors enabled; real per-message denominator). Caveat: the
  built-in Verizon SIM is not activated ([docs/DEVICE-PROFILE.md](docs/DEVICE-PROFILE.md)), so the
  traffic is broadcast-only -- real SIBs and paging, but the only NAS is one Attach Request ->
  Attach Reject (#7) per boot. That is a valid negative baseline for `nas_reject_identity` (#7 is
  correctly ignored) and for the RRC detectors, but says little about `auth_failure` or
  `null_integrity`, which never see an authentication or Security Mode Command here.
- DONE: RRC golden-vector harness (T-OR-112, patch 0011) seeded from the RRC-rich captures --
  unblocks the RRC detectors T-OR-056/057 and the RRC half of T-OR-111.
- DONE: T-OR-110 mobile-identity parsing (patch 0012): `EPSID`/`ID` decode in a pycrate-rs fork
  pinned by full commit (`doublegate/pycrate-rs@77b1cc8`, audited: no build script, unsafe, I/O or
  new runtime deps); EFF's public GUTI/M-TMSI/IMEISV test vectors plus synthetic IMSI/IMEI vectors cross-checked against Python pycrate and
  Wireshark. Unblocks T-OR-054. Upstreaming to EFForg/pycrate-rs is the maintainer's call.
- Remaining: build T-OR-054 on T-OR-110; build T-OR-056/057 on the T-OR-112 harness; decide each detector's default
  on/off. NAS-detector calibration, T-OR-055 EIA0-path verification, and real-capture validation of
  T-OR-110 need an **active SIM** (activate the built-in one, use another carrier's SIM if the unit
  is unlocked, or a shielded/cabled lab eNB) -- no radio cycle or reboot can produce an attach
  exchange on an inactive SIM.
- Then the v1.2.0 release ceremony (tag + GitHub release + on-device install validated).

## Readiness checklist (v0.1.0)

- [x] Scaffold, license, overlay layout, submodule pinned (T-OR-001)
- [x] CI workflows authored (T-OR-002)
- [x] `orx doctor` implemented with tests (T-OR-003)
- [x] Build and sync scripts (T-OR-004)
- [x] Research corpus (T-OR-005)
- [x] GitHub repository created (T-OR-006)
- [x] Specification docs, ROADMAP, PROJECT-STATUS, to-dos (T-OR-007)
- [x] `markdownlint-cli2 '**/*.md' '.github/**/*.md'` clean on the full tree
- [x] First commit and push (T-OR-008)
- [x] Branch protection on `main` (T-OR-009; classic rule converted to the `main` ruleset, see [docs/CI.md](docs/CI.md))
- [x] First CI run green with every job executing its real gate (T-OR-010)
- [ ] CHANGELOG `[Unreleased]` renamed to `[0.1.0]`, tag v0.1.0 (T-OR-011)
