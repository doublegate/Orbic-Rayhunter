# Contributing

Thanks for helping. A few rules keep this project safe to run on real hardware.

## Before you open a PR

- Run the gates: `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test`, `shellcheck` on the scripts you changed, `bash scripts/agy-review-selftest.sh`,
  and `scripts/apply-patches.sh` if you touched `patches/`.
- Update the matching `docs/` file in the same PR as any behavior change, and add an entry under
  `[Unreleased]` in `CHANGELOG.md`.
- Use [Conventional Commits](https://www.conventionalcommits.org/): `feat(orx): ...`,
  `fix(patches): ...`, `docs: ...`. Reference the roadmap ticket (`T-OR-NNN`).

## Patches against upstream Rayhunter

Changes to Rayhunter itself go in `patches/` as one focused patch per change, never as edits in
`upstream/`. Tag the subject `[UPSTREAM]` if it is generic and meant for EFForg/rayhunter, or
`[LOCAL]` if it only makes sense for this device. Procedure: [docs/PATCH-POLICY.md](docs/PATCH-POLICY.md).

## Never commit

- Firmware images, partition dumps or proprietary binaries extracted from the device.
- QMDL/PCAP captures, IMSI, IMEI, ICCID, MSISDN, the device admin password, or Wi-Fi keys.
  Scrub identifiers from logs you paste into issues.

## Device safety

Any procedure that writes to the device must be preceded by a verified backup
([docs/BACKUP-AND-RECOVERY.md](docs/BACKUP-AND-RECOVERY.md)) and must document how to undo it.
Nothing in this project transmits on licensed spectrum.

## Conduct

This project follows the [Code of Conduct](CODE_OF_CONDUCT.md).
