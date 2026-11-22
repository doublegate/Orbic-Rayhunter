# Phase 7: Host pipeline, firmware level, soak, release (v1.7.0..v2.0.0)

Goal: a host-side data pipeline for captures, the aggressive firmware-level tier (only with a
verified backup and a documented reversal for every write), a rebase onto current upstream with a
72 h soak, and v2.0.0. Tickets T-OR-089..T-OR-105 ([ROADMAP.md](../ROADMAP.md)).

## Sprint 7.1: Host data pipeline (v1.7.0)

- [ ] T-OR-089: incremental `orx pull-logs` with a local archive index (recording, hashes,
      analyzer versions, alert summary)
- [ ] T-OR-090: batch re-analysis of the archive with `rayhunter-check` from our patched build
- [ ] T-OR-091: `orx scrub`: IMSI, IMEI/IMEISV, GUTI/S-TMSI replaced; cell IDs and GPS coarsened
      on request; message structure preserved; scrub report written
- [ ] T-OR-092: report generation and sanitized share bundles for EFF submissions
- [ ] T-OR-093: cross-decode a sample of recordings with SCAT and compare message counts with our
      parser (regression check)

Acceptance criteria: scrubber unit tests prove no original identifier survives (search for the
original byte patterns); re-analysis is reproducible (same input, same output).

Verification:

```sh
cargo test -p orx-cli scrub
cargo run -p orx-cli -- scrub <in>.qmdl -o <out>.qmdl --report scrub.txt
grep -c -a '<original-imsi-digits>' <out>.qmdl     # must print 0
```

### Research (REA/Ghidra/web)

- SCAT (`parsers/qualcomm/diagcmd.py`, `diagltelogparser.py`) and MobileInsight decoders as
  references; behavior with gzip QMDL input (unverified,
  [ref-docs/qualcomm-diag-qmdl.md](../ref-docs/qualcomm-diag-qmdl.md) section 7).
- Upstream `doc/faq.md` (what EFF wants in a submission).

## Sprint 7.2: Boot chain and init (v1.8.0, part 1)

- [ ] T-OR-094: read-only RE of `aboot`, `boot` and kernel command line from the backup images;
      document signing (test key) and boot order
- [ ] T-OR-095: `[LOCAL]` replace the whole-file `misc-daemon` overwrite with a minimal hook that
      preserves the stock script (design from the Phase 2 diff); reversal documented
- [ ] T-OR-099: `diagchar` driver RE: ioctl conventions, peripheral masks, interaction with other
      DIAG clients on this kernel

Acceptance criteria: findings in `re/notes/` with hashes and confidence; the new hook survives a
reboot and the uninstall restores the stock file byte for byte.

Verification:

```sh
sha256sum backups/<date>-V1.2.0/mtd*-aboot.img backups/<date>-V1.2.0/mtd*-boot.img
# device shell after install with the new hook
sha256sum /etc/init.d/misc-daemon     # equals the stock backup value
```

### Research (REA/Ghidra/web)

- REA `extract_firmware`, `inspect_firmware_regions`, `binary_overview` on `aboot` (Little Kernel)
  and `boot` (kernel 3.18 image, ramdisk); Ghidra on LK for fastboot commands and boot-mode
  selection.
- Kernel `drivers/char/diag/` behavior: Android MSM kernel `diagchar.h` and, if extractable, the
  kernel image symbols; QCSuper "The Diag protocol" document.
- mjg59 posts on aboot and test-key signing ([ref-docs/orbic-firmware-v1.2.0.md](../ref-docs/orbic-firmware-v1.2.0.md)
  section 7).

## Sprint 7.3: OTA suppression, services, recovery research (v1.8.0, part 2)

- [ ] T-OR-096: permanent OTA suppression from the T-OR-018 findings, with exact reversal
- [ ] T-OR-097: disable stock services not needed for the hotspot or Rayhunter, one at a time,
      each with a before/after RAM, CPU and battery measurement and a reversal
- [ ] T-OR-098: EDL/firehose research: identify a loader candidate for HWID `000480e1`, prove it
      read-only (enumerate, read a partition, compare with the backup) before any write is
      considered; hardware test point documented with photos of our unit

Acceptance criteria: OTA client absent after reboot with the active SIM for an extended period;
each disabled service has a measured benefit or is reverted; no NAND write over EDL in this
sprint.

Verification:

```sh
# device shell
ps w; ls -l /etc/rc*.d
# host, in EDL only, read-only
lsusb -d 05c6:9008
```

### Research (REA/Ghidra/web)

- Ghidra on the FOTA client and its init script (from T-OR-020) for the cleanest disable point.
- bkerler/edl and Loaders (`prog_nand_firehose_9x07.mbn`), Biktorgj/quectel_eg25_recovery
  (`NPRG9x07.mbn`, `ENPRG9x07.mbn`), qualcomm/qdlrs; Rayhunter discussion #993 (Moxee attempts)
  ([ref-docs/orbic-rc400l-hardware.md](../ref-docs/orbic-rc400l-hardware.md) section 4).
- XDA "Resetting Verizon Orbic Speed RC400L" thread (test point, Sahara IDs).

## Sprint 7.4: Upstream sync and soak (v1.9.0)

- [ ] T-OR-100: `scripts/sync-upstream.sh <latest>`; re-create failing patches on the new tag
- [ ] T-OR-101: drop `[UPSTREAM]` patches merged upstream; CHANGELOG entry per dropped patch
- [ ] T-OR-102: 72 h continuous recording soak; hourly samples into the PERFORMANCE.md soak table
- [ ] T-OR-103: v2.0.0-rc.1; full uninstall/restore rehearsal on the RC

Acceptance criteria: soak with no daemon exit, no storage exhaustion, hotspot reachable
throughout; restore rehearsal passes.

Verification:

```sh
scripts/sync-upstream.sh
scripts/apply-patches.sh && scripts/build.sh firmware
cargo run -p orx-cli -- verify
```

### Research (REA/Ghidra/web)

- Upstream release notes between v0.13.0 and the new tag; any Orbic-affecting issues opened since.

## Sprint 7.5: v2.0.0

- [ ] T-OR-104: final security audit (SECURITY-MODEL.md walked end to end) and docs sweep
- [ ] T-OR-105: release v2.0.0 per [docs/RELEASE-PROCESS.md](../docs/RELEASE-PROCESS.md)

Acceptance criteria: every `[UPSTREAM]` ticket merged upstream, open as a PR, or declined with
the reason recorded; release checklist passes.

Verification:

```sh
cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test
markdownlint-cli2 '**/*.md'
git tag -l v2.0.0 && gh release view v2.0.0
```

### Research (REA/Ghidra/web)

- None new; confirm no firmware, capture or identifier entered the repository history
  (`gitleaks detect` over the full history).
