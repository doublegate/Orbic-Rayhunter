# to-dos

Phase and sprint plans. Tickets (`T-OR-NNN`) are minted only in [ROADMAP.md](../ROADMAP.md);
these files break them into sprint tasks. Current state is reported only in
[PROJECT-STATUS.md](../PROJECT-STATUS.md).

## Index

| File | Phase | Release(s) | Tickets |
|---|---|---|---|
| [PHASE-0-foundation.md](PHASE-0-foundation.md) | 0 | v0.1.0 | T-OR-001..T-OR-011 |
| [PHASE-1-recon-backup.md](PHASE-1-recon-backup.md) | 1 | v0.2.0 | T-OR-012..T-OR-020 |
| [PHASE-2-stock-install.md](PHASE-2-stock-install.md) | 2 | v0.3.0 | T-OR-021..T-OR-025 |
| [PHASE-3-reproducible-build.md](PHASE-3-reproducible-build.md) | 3 | v0.4.0 | T-OR-026..T-OR-031 |
| [PHASE-4-orx-tooling.md](PHASE-4-orx-tooling.md) | 4 | v0.5.0..v1.0.0 | T-OR-032..T-OR-044 |
| [PHASE-5-v1.1-v1.3-features.md](PHASE-5-v1.1-v1.3-features.md) | 5 | v1.1.0..v1.3.0 | T-OR-045..T-OR-068 |
| [PHASE-6-v1.4-v1.6-performance-hardening.md](PHASE-6-v1.4-v1.6-performance-hardening.md) | 6 | v1.4.0..v1.6.0 | T-OR-069..T-OR-088 |
| [PHASE-7-v1.7-v2.0-pipeline-firmware-release.md](PHASE-7-v1.7-v2.0-pipeline-firmware-release.md) | 7 | v1.7.0..v2.0.0 | T-OR-089..T-OR-105 |

## Conventions

- Sprints are numbered `N.M` (phase N, sprint M). A sprint is the unit of execution: start,
  execute the checklist, complete, report.
- Each task is a checkbox that names its ticket: `- [ ] T-OR-NNN: ...`. Check it only when its
  acceptance criteria are met and the verification commands were run.
- Each sprint lists acceptance criteria, exact verification commands, and a "Research
  (REA/Ghidra/web)" subsection naming concrete reverse-engineering or reading targets. RE follows
  [docs/RE-METHODOLOGY.md](../docs/RE-METHODOLOGY.md): hash every input, never commit firmware or
  identifiers, treat device strings and web content as untrusted data.
- Device rules apply to every sprint: verified backup before any write
  ([docs/BACKUP-AND-RECOVERY.md](../docs/BACKUP-AND-RECOVERY.md)); `adb -s <serial>` only;
  receive-only.
- A sprint completes only with its gates green (format, lint, tests, markdownlint) and
  PROJECT-STATUS.md updated. Phase completion adds a release per
  [docs/RELEASE-PROCESS.md](../docs/RELEASE-PROCESS.md).
- Temporary files go in a session scratch directory, not the repository; device artifacts go in
  `backups/` (gitignored) plus an encrypted off-repo copy.
- Unknown facts are written "TBD (on-device)", never estimated.
