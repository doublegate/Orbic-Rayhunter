# Orbic-Rayhunter Review Style Guide

Rules the Antigravity reviewer enforces on Orbic-Rayhunter pull requests. The project is an
overlay on EFF Rayhunter (a passive IMSI-catcher detector, Rust, GPL-3.0) for the Orbic RC400L
hotspot (Qualcomm MDM9207, armv7, firmware `ORB400L_V1.2.0_BVZRT`).

Authoritative context: `AGENTS.md` (layout, commands, device rules), `docs/ARCHITECTURE.md`,
`docs/PATCH-POLICY.md`, `docs/CI.md`, `docs/SECURITY-MODEL.md`, `docs/BACKUP-AND-RECOVERY.md`,
and the ADRs in `docs/adr/`. `ref-docs/` is an immutable research corpus: corrections belong in a
new dated supplement, never in an edit to an existing file.

## Priorities (in order)

1. **Device safety.** No software unbrick path exists for this device. Any procedure, script or
   `orx` command that writes to the device must be gated on a verified, checksummed backup and
   must document how to undo it. The stock `/etc/init.d/misc-daemon` must be preserved.
2. **Privacy of captures.** QMDL/PCAP/NDJSON captures and logs can contain IMSI, IMEI, ICCID,
   phone numbers and location. They must never be committed, uploaded or logged unredacted.
3. **Security.** Validate external input at boundaries (device responses, HTTP, sysfs, upstream
   tag names, workflow inputs); no secrets in code, logs or errors; fail closed.
4. **Correctness of the overlay.** `upstream/` is never edited; Rayhunter changes are patch files
   listed in `patches/series`, tagged `[UPSTREAM]` or `[LOCAL]`.
5. Tests and docs accompany behavior changes (`docs/` is the specification; `CHANGELOG.md`
   `[Unreleased]` is updated in the same PR).

## Conventions

- Conventional Commits; reference the roadmap ticket (`T-OR-NNN`) where one exists.
- Rust: toolchain 1.98.0, `unsafe_code = "forbid"`, clippy with `-D warnings`, `cargo fmt`.
  No `unwrap`/`expect` on device, network or file input.
- Shell: `set -euo pipefail`, shellcheck clean, scripts committed as mode `100755`.
- Workflows: every action pinned to a full commit SHA, `persist-credentials: false` on checkout,
  untrusted values passed through `env:` (never interpolated into `run:`), path-aware jobs gated
  with job-level `if:` (never trigger-level `paths:`), required checks kept fail-closed.
- No emojis in code, comments, commits or docs.

## What to flag as BLOCKING

- Any edit inside `upstream/`, or a patch that does not apply with `scripts/apply-patches.sh`.
- A device write without the backup gate, or a change that removes a restore path.
- Committed firmware images, partition dumps, captures, device identifiers (IMEI, IMSI, ICCID,
  USB serials), admin passwords or Wi-Fi keys; `.gitignore` changes that would admit them.
- Anything that transmits on licensed spectrum or makes the project non-passive.
- Unvalidated external input reaching a shell, filesystem path, `adb`/`nc` command or workflow
  `run:` line; a workflow change that lets a skipped or failed job satisfy a required check.
- An action pinned to a tag or branch instead of a SHA; a token persisted where untrusted code
  runs.
- Behavior changes without the matching `docs/` and `CHANGELOG.md` update.

## What to keep as SUGGESTION / NITPICK

- Naming, structure and readability.
- Missing tests for non-critical paths.
- Performance ideas without a measurement (`docs/PERFORMANCE.md`: baseline first).
- Wording in `ref-docs/` (immutable: suggest a dated supplement instead of an edit).
