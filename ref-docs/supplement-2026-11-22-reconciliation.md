# Supplement 2026-11-22: Reconciliation of the Phase 0 research corpus

- Date: 2026-11-22
- Applies to: every `ref-docs/*.md` file retrieved on 2026-11-22
- Status: open items are resolved on-device or in source during Phase 1 (T-OR-012 to T-OR-020)

The `ref-docs/` corpus is immutable. When the documents were cross-checked while `docs/` was
derived, they disagreed with each other in five places. This supplement records each conflict,
which reading `docs/` adopts for now, and how it will be settled. The `docs/` files are the
specification; this file explains why they chose what they chose.

## 1. LTE band list

- `lte-imsi-catcher-detection.md` (sections 1.3, 3.8) repeats the upstream Rayhunter book's list:
  B2/B4/B5/B12/B13/B48/B66, and states that B48 (CBRS) is supported.
- `orbic-rc400l-hardware.md` (section 2) cites the FCC grant (FCC ID 2ABGH-RC400L), which lists
  only LTE B2/B4/B5/B13/B66 (plus WCDMA B2/B5).
- Adopted: the FCC list. B12 and B48 are unverified.
- Resolution: read the modem's band capability on the device (QMI/AT band query) in Phase 1 and
  record it in `docs/DEVICE-PROFILE.md`.

## 2. Analyzer versions for `no_nas_messages` and `test_analyzer`

- `rayhunter-upstream-overview.md` (section 5) gives version 1.
- `lte-imsi-catcher-detection.md` (section 1.4) gives "-".
- Adopted: 1, from the analyzer source at `v0.13.0`.
- Resolution: none needed beyond citing `upstream/lib/src/analysis/` in `docs/ANALYZERS.md`.

## 3. Newest firmware release

- `orbic-firmware-v1.2.0.md` lists Verizon releases up to `V1.3.2` (System Update 7,
  2025-08-19) and leaves out `V1.2.1` and `V1.2.6`.
- `community-installs.md` says no user has reported firmware newer than `1.3.0`, and does
  report `V1.2.1` and `V1.2.6`.
- These do not contradict each other: one counts vendor releases, the other counts user reports.
  Together, the known versions are 1.1.5, 1.2.0, 1.2.1, 1.2.2, 1.2.6, 1.2.8, 1.3.0 and 1.3.2.
- Adopted: block OTA updates (T-OR-018) until the effect of 1.3.x on the install path is known.

## 4. Discussion #669

- `orbic-firmware-v1.2.0.md` cites EFForg/rayhunter discussion #669 as a `V1.2.0` user report.
- `community-installs.md` cites it only for an `adb kill-server` tip.
- Adopted: not counted as evidence for `V1.2.0` install success.
- Resolution: re-read the discussion before Phase 2 and correct the citation in the next
  supplement if needed.

## 5. QMDL compression

- `qualcomm-diag-qmdl.md` (sections 6-7) says the writer always gzips QMDL output.
- `rayhunter-upstream-overview.md` (section 2) says recordings may be `.qmdl` or `.qmdl.gz`.
- Adopted: treat compression as possibly optional; tooling must accept both names.
- Resolution: read `upstream/daemon/src/qmdl_store.rs` at the pinned tag and inspect real
  recordings in Phase 2.

## Corrections made in `docs/` (not `ref-docs/`)

`docs/ARCHITECTURE.md` originally described `installer orbic` as the USB installer and showed
`/bin/rootshell` on every install. Since upstream v0.8.0, `installer orbic` is the network
installer (`orbic-network` is an alias), and only `installer orbic-usb` creates `/bin/rootshell`.
`docs/ARCHITECTURE.md` was corrected before the first commit.
