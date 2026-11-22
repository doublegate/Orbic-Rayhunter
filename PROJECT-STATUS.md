# Project Status

Single source of truth for the current state of Orbic-Rayhunter. Other files point here instead
of restating status. Updated at every sprint and phase boundary.

- Last updated: 2026-11-22
- Version: 0.1.0-dev (`Cargo.toml` `workspace.package.version = "0.1.0"`, unreleased)
- Upstream pin: EFForg/rayhunter v0.13.0 (cd462a1)
- Target: Orbic RC400L, `ORB400L_V1.2.0_BVZRT`

## Executive summary

Phase 0 (Foundation, v0.1.0) is in progress. Sprint 0.1 (scaffold) is complete and Sprint 0.2
(specification docs) is complete. The overlay layout, upstream submodule, `orx doctor`, build
scripts, CI, research corpus, specification docs, roadmap and phase plans are on `main` (first
commit `12a7679`, CI fix `b982d00`). Every CI job has run its real gate and is green, including
the armv7 patched-upstream build, and `main` is governed by a repository ruleset. Rayhunter is not installed on
the unit and no device backup exists. Remaining for v0.1.0: tag the release (T-OR-011). Next
phase: Phase 1 (recon and verified backup).

## Phase matrix

| Phase | Release(s) | Theme | Status | Plan |
|---|---|---|---|---|
| 0 | v0.1.0 | Foundation | in progress (T-OR-001..010 done; T-OR-011 tag open) | [to-dos/PHASE-0-foundation.md](to-dos/PHASE-0-foundation.md) |
| 1 | v0.2.0 | Device recon + verified backup | not started | [to-dos/PHASE-1-recon-backup.md](to-dos/PHASE-1-recon-backup.md) |
| 2 | v0.3.0 | Stock upstream install | not started | [to-dos/PHASE-2-stock-install.md](to-dos/PHASE-2-stock-install.md) |
| 3 | v0.4.0 | Reproducible patched build | not started | [to-dos/PHASE-3-reproducible-build.md](to-dos/PHASE-3-reproducible-build.md) |
| 4 | v0.5.0..v1.0.0 | `orx` tooling, baseline | not started | [to-dos/PHASE-4-orx-tooling.md](to-dos/PHASE-4-orx-tooling.md) |
| 5 | v1.1.0..v1.3.0 | Alerting, analyzers, storage | not started | [to-dos/PHASE-5-v1.1-v1.3-features.md](to-dos/PHASE-5-v1.1-v1.3-features.md) |
| 6 | v1.4.0..v1.6.0 | Performance, UI/UX, security | not started | [to-dos/PHASE-6-v1.4-v1.6-performance-hardening.md](to-dos/PHASE-6-v1.4-v1.6-performance-hardening.md) |
| 7 | v1.7.0..v2.0.0 | Host pipeline, firmware, soak, release | not started | [to-dos/PHASE-7-v1.7-v2.0-pipeline-firmware-release.md](to-dos/PHASE-7-v1.7-v2.0-pipeline-firmware-release.md) |

## Capability matrix

| Capability | Status | Ticket | Notes |
|---|---|---|---|
| Upstream pinned as submodule | present | T-OR-001 | v0.13.0 |
| Patch series pipeline | present, series empty | T-OR-004 | `scripts/apply-patches.sh` |
| armv7 daemon build | present, built locally and in CI | T-OR-004 | `scripts/build.sh devel` produced `dist/` |
| `orx doctor` | present | T-OR-003 | USB f626/f601/9008, TCP :80 and :8080; f622 not classified |
| CI workflows | green on `main` | T-OR-002, T-OR-010 | ci, docs, security, upstream-watch; release untested until a tag |
| Specification docs | present | T-OR-007 | `docs/` |
| Device profile | measured facts only (USB, network) | T-OR-012, T-OR-014 | [docs/DEVICE-PROFILE.md](docs/DEVICE-PROFILE.md) |
| Verified device backup | absent | T-OR-015..T-OR-017 | blocks every device write |
| OTA block | absent | T-OR-018 | |
| Rayhunter on device | not installed | T-OR-022 | |
| `orx backup` / `install` / `verify` / `pull-logs` / `restore` / `purge` | absent | T-OR-034..T-OR-041 | |
| New analyzers | absent | T-OR-051..T-OR-064 | |

## Metrics snapshot

| Metric | Value | Source |
|---|---|---|
| Commits | 3 on `main` | `git log` |
| Tickets | 105 minted; 10 done (T-OR-001..T-OR-010) | [ROADMAP.md](ROADMAP.md) |
| Patches in series | 0 | `patches/series` |
| Host crates | 1 (`orx-cli`) | `Cargo.toml` workspace members |
| Host unit tests | 5 passed | `cargo test` (local and CI) |
| CI runs | `ci` run 45041852700 green (the 8 checks required at the time passed; the `main` ruleset now requires 11, see docs/CI.md) | GitHub Actions on `b982d00` |
| Latest release | none | `gh release list` |
| Local devel daemon build | 16291376 bytes, unstripped, SHA-256 `1bad88b8...a091c` | `ls -l dist/`, `dist/SHA256SUMS` (2026-11-22) |
| Device firmware | `ORB400L_V1.2.0_BVZRT` | [docs/DEVICE-PROFILE.md](docs/DEVICE-PROFILE.md) |
| Device backup | none | `backups/` |
| Performance baseline | none | [docs/PERFORMANCE.md](docs/PERFORMANCE.md) |

## Timeline

| Date | Event |
|---|---|
| 2026-09-16 | Upstream Rayhunter v0.13.0 released (pinned) |
| 2026-11-22 | Research corpus retrieved (`ref-docs/`), scaffold, `orx doctor`, CI definitions, first local devel build; GitHub repository created |
| 2026-11-22 | Specification docs, ROADMAP, PROJECT-STATUS and `to-dos/` written (Sprint 0.2) |
| 2026-11-22 | First commit and push (`12a7679`); CI fix for script modes and workflow input hardening (`b982d00`); all CI green; branch protection on `main` |

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

- Tag v0.1.0 (T-OR-011) following [docs/RELEASE-PROCESS.md](docs/RELEASE-PROCESS.md).
- Start Phase 1 with read-only recon (T-OR-012..T-OR-014), then the verified backup.

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
