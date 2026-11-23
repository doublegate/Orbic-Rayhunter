## [Unreleased]

### Added

- (nothing yet)

## [0.7.0] - 2026-11-23

Adds `orx verify` and the shared backup-gate, so a backup can be proven as a rollback before a device write. Also the first release routed through a pull request (CI, docs, security and the Antigravity reviewer all gate it), and it carries the agy-review fixes below. orx-cli stays zero-dependency.

### Added

- `orx verify --dir <dir>` (T-OR-035, Sprint 4.3): re-reads a backup directory's `MANIFEST.txt`
  and re-hashes each `<fs>.tar`, reporting ok / missing / too-small / size-mismatch / hash-mismatch
  per filesystem, then a gate line. The shared `verify::gate()` (the Rust counterpart of
  `orbic-ota-block.py`'s `_backup_gate`) requires rootfs `/` to be `ok`/`ok-live` AND its archive
  to verify, and is what the upcoming `orx install`/`uninstall` will call before any device write.
  Manifest parsing reads tokens by key, so both the Python and Rust manifest layouts verify. No
  device access; fully unit-tested (43 orx-cli tests). It fails closed throughout: `parse_manifest`
  errors on a malformed row (< 3 fields, a non-`key=value` token, an empty key, a non-`u64` `size=`,
  a non-64-hex `sha256=`, a duplicate `size=`/`sha256=`, or any control character);
  a row without a valid `size=` + 64-hex `sha256=` is rejected unchecked; a recorded state other
  than `ok`/`ok-live` fails the entry; a duplicate rootfs `/` row is ambiguous; an archive that is
  a symlink or non-regular file is rejected (checked before open and re-checked on the open handle,
  portably -- no arch-specific flag); and `orx verify` + `verify::gate()` share one gate policy over
  verdicts computed once (the ~87 MB rootfs is hashed once). Hardening from the PR's Codex +
  Antigravity review.

### Fixed

Address all eight findings from an adversarial `agy` (Antigravity) review of the v0.1.0-v0.6.0
changes that were pushed straight to `main` without a PR review:

- **Backup write-gate now verifies the archive, not just the manifest text** (`orbic-ota-block.py`
  `_backup_gate`): the `root.tar` beside the manifest must exist, clear the 1 KiB floor, and match
  the `size=`/`sha256=` the manifest records; tokens are parsed by key so both the Python and Rust
  manifest layouts are accepted. A stale, hand-edited, or deleted archive no longer passes.
- **`orx backup` emits a gate-compatible `ok-live`** (was `ok-live (tar exit N)`, whose trailing
  text made every live-rootfs manifest fail the gate's last-token check).
- **A failed `orx backup` no longer leaves a gate-passable `MANIFEST.txt`**: on a target failure it
  removes any stale `MANIFEST.txt` and writes `MANIFEST.incomplete.txt`, so a partial backup cannot
  unlock device writes.
- **`orbic-ota-block.py --apply` always persists the undo manifest**, even on a mid-loop failure
  (the device is never mutated without an undo record), refuses to overwrite an existing
  `.orx-disabled` saved binary, and `--undo` refuses to clobber a binary already present at the
  original path and warns on a restored-hash mismatch.
- **`backup::unescape` no longer corrupts non-ASCII mountpoints** (decodes bytes once via
  `from_utf8_lossy` instead of casting each UTF-8 byte to `char`; octal predicate is `0-7`).
- **`scripts/build.sh firmware` cleans up its multi-GB temp target dir** on exit (the container
  chowns it back to the host user so the trap can remove it).
- **`release.yml` matches the CHANGELOG header literally** (`index()`), not as a regex whose dots
  and `+` could match unintended versions.
- **`probe::json_str_field` matches a key, not a same-named value**, and refuses `\uXXXX` rather
  than emitting a bogus literal; **`transport` `read_until` errors on the 64 MiB cap** instead of
  returning a misleading "no marker".

All fixes carry unit tests (34 orx-cli tests + the Python gate self-test); every gate green.

## [0.6.0] - 2026-11-23

Adds `orx backup`, the first `orx` device operation and the write-gate every later command depends on. It ports the proven Python backup to native Rust on the Sprint 4.1 transport (streaming tar + inline SHA-256), and is validated end-to-end on the device. orx-cli stays zero-dependency; 33 unit tests.

### Added

- `orx backup` (T-OR-034, Sprint 4.2): archives every writable device filesystem over the root
  shell with `tar`, streamed to a `0600` `<fs>.tar` on the host while hashing inline, and writes
  a `MANIFEST.txt` in the exact format the write-gate reads. Each archive's state is `ok`, or
  `ok-live` (only with `--allow-live`, for a writable fs whose tar exits non-zero); a sub-1 KiB
  result is rejected so a failed tar cannot masquerade as a backup. The device-correct busybox
  `tar` invocation (`-C <mp> <--exclude> -cf - . 2>/dev/null`, no `--ignore-failed-read`) and the
  nested-mount excludes are unit-tested against the fake shell, and the command was validated
  end-to-end on the unit: root.tar 86.7 MB, data/usrdata/cache all `ok`, the inline SHA-256
  matching an independent `sha256sum`.
  Output files are written to an `O_EXCL` temp (fresh `0600`, no symlink follow) and renamed
  over the target, so a backup of device data (which can contain IMEI/IMSI) is never written
  through a planted symlink (flagged by the automated security review).

- `transport`: a streaming `Shell::run_to(command, sink)` that writes a command's output to a
  writer as it arrives (never buffering), for a `tar`-of-filesystem that would not fit in memory.
  Its core is a unit-tested `MarkerSplitter` that separates the payload from the trailing
  exit-status marker correctly across any chunk boundary (including a marker split mid-sequence),
  so the end marker can never corrupt a streamed backup. Foundation for `orx backup`/`pull-logs`.

## [0.5.0] - 2026-11-23

First `orx` tooling release (Sprint 4.1). `orx doctor` gains `--json`, a reported Rayhunter version and documented exit codes, and a tested, dependency-free device transport layer (shell + checksummed file transfer) lands as the foundation for the backup/install/pull-logs/uninstall commands in later Phase 4 sprints. orx-cli remains zero-dependency; 23 unit tests, all device I/O behind a trait with an in-memory fake.

### Added

- `orx-cli` `transport` module (T-OR-033, Sprint 4.1): a `Shell` trait (run a command with an
  exit-code marker, transfer a file) over the device's TCP-24 root shell, mirroring the proven
  Python tools' wire protocol. The verified transfer (`send_file`) accepts a file only when the
  device's own `sha256sum` matches the bytes sent, and rejects unsafe remote paths before
  touching the device. A `TcpShell` real implementation and an in-memory `FakeShell` let every
  caller be unit-tested with no hardware; the failure paths (hash mismatch, dropped write, unsafe
  path, read timeout) and the real `TcpShell` framing are covered (23 orx-cli tests).
- `orx-cli` `sha256` module: a std-only, streaming SHA-256 (no dependency added) for verifying
  device transfers against a device-side `sha256sum`, validated against the FIPS 180-4 known
  vectors (incl. the one-million-'a' case). Foundation for the Sprint 4.1 transport (T-OR-033).
- Hardening (from the automated security review of doctor v2): `probe::http_get` now enforces a
  single total deadline across all reads (a per-read timeout let a slow-drip peer stall
  indefinitely), and `orx doctor` validates the device-reported Rayhunter version against a
  strict `[0-9A-Za-z.+-]{1,64}` token before printing it (a compromised daemon or local MITM
  could otherwise inject terminal escape sequences).
- `orx doctor` v2 (T-OR-032, Sprint 4.1): `--json` machine-readable output, the running
  Rayhunter version read from `/api/system-stats` when `:8080` answers, and documented exit
  codes (0 found, 1 nothing found, 2 usage/host error). A minimal std-only HTTP/1.0 client and a
  dependency-free JSON string-field reader were added to `probe` (orx-cli stays zero-dependency),
  both unit-tested with a loopback fake server. `rayhunter` reachability now also counts toward a
  success exit.

## [0.4.0] - 2026-11-23

Makes our build produce the **production** daemon and records the reproducibility result. `release.yml` now ships the `firmware`-profile daemon (pq-tls, stripped, opt-level z, fat LTO -- ~6.1 MB), the device binary, instead of the `firmware-devel` build (~16 MB) that v0.1.0-v0.3.0 shipped. `scripts/build.sh firmware` builds it reproducibly in the same `messense/rust-musl-cross` container upstream uses, so it needs only docker/podman + npm, no host cross toolchain. Measured: our build reproduces the upstream release daemon to within 384 bytes (same profile/toolchain/lockfile/pq-tls); not bit-identical (embedded build paths differ).

### Changed

- `release.yml` builds and ships the production `firmware` daemon + rootshell (was
  `firmware-devel`). The release notes' build-provenance section now records the daemon profile
  and the reproducibility result.
- `scripts/build.sh firmware` now runs the cargo build inside the
  `messense/rust-musl-cross:armv7-musleabihf` container (no host `arm-linux-musleabihf` cross
  toolchain required); the web UI is still built on the host. The target dir is a scratch path
  outside the tree so the container's root-owned artifacts never land in `build/rayhunter`.

### Added

- `re/notes/reproducible-build.md` (T-OR-030): method, hashes and interpretation of the
  our-build-vs-upstream-release daemon comparison. `docs/BUILD.md` reproducibility section
  updated to match.

## [0.3.0] - 2026-11-23

Installs stock upstream Rayhunter v0.13.0 on the device and validates the end-to-end capture path. The network installer deploys the daemon, config, `rayhunter_daemon` init and the hooked `misc-daemon` exactly as the install-footprint RE predicted; after a reboot the daemon records and the Phase 1 OTA block persists. Validated on the inactive built-in SIM without an active plan -- `rayhunter-check` analyzes a live capture clean, confirming capture + offline analysis work on this unit. Upstream pin: EFForg/rayhunter `v0.13.0` (`cd462a1`). The install is the reversible footprint from Phase 1's RE; the Phase 1 backup is the rollback.

### Added

- Phase 2: upstream Rayhunter v0.13.0 installed and validated on the device (T-OR-021/T-OR-022).
  The network installer deployed the daemon + `config.toml` + `rayhunter_daemon` init + the
  hooked `misc-daemon`, as the install-footprint RE predicted; after reboot the web UI/API answer
  on :8080, the daemon records, and the OTA-block renames persisted. A live QMDL (1282 messages)
  analyzed clean by `rayhunter-check` (0 warnings), with an `EMM Attach Reject` confirming the
  inactive SIM reaches towers -- end-to-end capture + analysis without an active plan.

## [0.2.0] - 2026-11-23

Completes Phase 1: on-device reconnaissance plus the reversible OTA-update block. The carrier updater (`oma_dm`/`dmclient`/`mdm-daemon`) is reverse-engineered and reversibly disabled; the 0x11EB user-IP-traffic privacy question is answered from the upstream source; and the no-active-service path is established (an inactive SIM captures broadcast control-plane, enough to validate the capture path; analyzer accuracy work runs offline with `rayhunter-check`). Builds on v0.1.0 (foundation + recon + verified backup). Upstream pin: EFForg/rayhunter `v0.13.0` (`cd462a1`). Nothing in this release modifies the device beyond the reversible OTA block, which is backup-gated and has an undo manifest.

### Added

- `scripts/orbic-ota-block.py`: reversibly disables the on-demand OTA/FOTA updater
  (`oma_dm`/`dmclient`/`mdm-daemon`) by renaming them to `.orx-disabled`. The first tool that
  writes to the device: a fixed target allowlist (no arbitrary paths), a verified-backup gate,
  an undo manifest and `--undo`, and an audit line. Status (read-only) is the default; `--apply`
  requires `--backup-manifest`. Applied on this unit (T-OR-018).
- `re/notes/ota-mechanism.md` and `re/notes/install-footprint.md`: read-only RE of the backed-up
  firmware (T-OR-020). Auto-FOTA is off on this unit, there is no boot-time update daemon, and
  the on-demand updater is triggered only by the stock UI's `goahead` `/goform/Fota*` actions or
  a carrier-pushed OMA-DM session. The upstream install footprint is a `misc-daemon` hook plus
  files under `/data`, fully file-level reversible.
- `ref-docs/running-without-active-service-2026-11-23.md`: how to run Rayhunter without an active
  plan. An inactive SIM (the built-in Verizon SIM here) captures broadcast control-plane only
  (SIB1/Paging) -- enough for the broadcast heuristics and to validate the capture path -- while
  connection-based heuristics and user-plane capture need an active data session. Analyzer
  accuracy work proceeds offline with `rayhunter-check` + golden QMDL vectors.
- T-OR-019 (0x11EB privacy check) answered from the upstream source: it subscribes to
  `LOG_DATA_PROTOCOL_LOGGING_C` ("User IP traffic"), so an active plan would log user IP packets
  to QMDL; the empirical on-device test is deferred (no active plan), the mitigation (T-OR-029)
  justified. Recorded in docs/DEVICE-PROFILE.md and docs/TESTING.md.

### Changed

- `scripts/orbic-ui-read.py` refuses all read-shaped `Fota*` endpoints (not only
  `FotaGetVersionInfo`), since each drives the device OTA flow.
- `docs/DEVICE-PROFILE.md` and `docs/BACKUP-AND-RECOVERY.md` record the measured OTA mechanism
  and the refined, evidence-based OTA-block plan (T-OR-018).

## [0.1.0] - 2026-11-23

Foundation plus Phase 1 on-device reconnaissance and a verified pre-write backup. The repository,
CI, docs and the research corpus are in place; the host `orx` tool and the read-only device tools
(`orbic-shell.py`, `orbic-ui-read.py`, `orbic-backup.py`) are shipped and self-tested; the
RC400L's layout is measured and every writable filesystem is backed up and verified. Nothing in
this release installs or modifies anything on the device. Upstream pin: EFForg/rayhunter
`v0.13.0` (`cd462a1`).

### Added

- `scripts/orbic-ui-read.py`: a read-only client for the stock GoAhead web UI. It logs in with
  the same encoding as upstream's installer (verified against four reference vectors from
  upstream's own `encode_password`), calls only an allowlist of read endpoints, and **refuses**
  `FotaGetVersionInfo` (which would ask the device to check for an OTA update). The admin
  password is read from a mode-600 file, never argv, and identifier fields are length-redacted
  on stdout.
- `scripts/orbic-shell.py`: drives the upstream root shell (busybox `nc` on TCP 24) one command
  at a time with per-command markers and exit status. It is read-only by policy, decided entirely
  on shlex TOKENS (so quoting cannot disguise a program or flag): a fail-closed allowlist of pure
  readers, with every arbitrary-execution program (`awk`, `command`, `find`, `sed`, `dd`,
  `nanddump`, `mount`) excluded, control characters and command substitution rejected, and output
  redirects validated per token. No recon run can write flash before the Sprint 1.3 backup, and
  there is no write-enable override. `scripts/recon/` holds the inventory command lists.
- `scripts/orbic-backup.py`: the Sprint 1.3 backup tool (T-OR-015..T-OR-017). It archives each
  mounted `ubifs` filesystem with `tar` on the device, streamed to the host over the port-24 root
  shell (the device firewall blocks a connect-back), and verifies each as a complete, readable tar
  with a two-pass SHA-256 (byte-identical for a quiescent fs, readable-only for a live one like
  `/data`), promoting it to a mode-0600 `<label>.tar` only on success. **Not** a raw NAND image:
  `nanddump`/`dd` of `/dev/mtd*` deadlocks this device (raw MTD contends with the live UBI and
  wedges the NAND), and tar through the VFS is both safe and the correct rollback for a Rayhunter
  install, which only changes the mounted volumes.
- Phase 1 Sprint 1.2 on-device recon (T-OR-013/T-OR-014) recorded in docs/DEVICE-PROFILE.md:
  kernel 3.18.48 and command line, the 17-partition MTD table and UBI layout, mounts and free
  space, the available backup tools (`nanddump`, `nandwrite`, `flash_erase`, `ubinfo`,
  `sha256sum`), listening sockets, the absence of a persistent FOTA daemon, and the stock
  `misc-daemon` SHA-256 baseline. The one device change from opening the shell (`telnetd` 1->0
  in the remote-access config) is noted; identifiers are redacted.
- Repository scaffold: GPL-3.0-or-later license (ADR 0002), overlay layout (ADR 0001), upstream
  EFForg/rayhunter submodule pinned to `v0.13.0` (`cd462a1`), Rust 1.98.0 toolchain pin matching
  upstream.
- `orx` host tool (`crates/orx-cli`, `unsafe_code = "forbid"`, no dependencies) with
  `orx doctor [--ip <addr>]`: Qualcomm USB discovery through sysfs, classifying `05c6:f626`
  (stock RNDIS + CDC), `05c6:f601` (debug ADB + DIAG + AT) and `05c6:9008` (EDL), plus TCP
  reachability of the stock web UI (`:80`) and Rayhunter (`:8080`). Exit 1 when no device and no
  stock UI are found, 2 on usage or sysfs errors. Five unit tests.
- Build tooling: `scripts/apply-patches.sh` (exports the committed submodule tree with
  `git archive` and applies `patches/series` with `git apply --check` first, never touching
  `upstream/`), `scripts/build.sh devel|firmware` (upstream web frontend plus upstream cargo
  aliases for `armv7-unknown-linux-musleabihf`, artifacts and `SHA256SUMS` in `dist/`),
  `scripts/sync-upstream.sh [<tag>]` (pinned vs latest, trial patch apply),
  `scripts/gen-assets.sh` (derived icons, favicons, banners and SVG renders).
- Device configuration template `device/config/config.orbic.toml` (upstream keys only; double-tap
  recording and larger free-space floors as documented deviations).
- CI, with every action pinned to a commit SHA: `ci.yml` (host crate fmt, clippy `-D warnings`,
  tests; shellcheck; patch series apply plus armv7 daemon and rootshell build with artifact
  upload), `docs.yml` (markdownlint, lychee), `security.yml` (gitleaks 8.30.1, cargo-audit,
  CodeQL for Actions and Rust), `upstream-watch.yml` (weekly new-tag issue with trial patch
  apply), `release.yml` (tag-driven build, checksums, notes from this file, never duplicates a
  release). Dependabot for Actions and Cargo; the submodule is moved deliberately.
- Community files: README, CONTRIBUTING, SECURITY (private vulnerability reporting),
  CODE_OF_CONDUCT (Contributor Covenant 2.1), issue forms, PR template, CODEOWNERS, AGENTS.md and
  CLAUDE.md.
- Research corpus `ref-docs/`: upstream overview, Orbic install procedures, community install
  reports, RC400L hardware, firmware V1.2.0, Qualcomm DIAG/QMDL, related tools, LTE IMSI-catcher
  detection literature with a ranked analyzer backlog, legal and ethics notes, and a dated
  reconciliation supplement for five cross-document conflicts. `ref-docs/pdfs/README.md` lists 20
  locally downloaded papers with source and stated license.
- Specification `docs/`: architecture, device profile, install, backup and recovery,
  configuration, analyzers, testing, performance, security model, patch policy, build,
  reverse-engineering methodology, release process, ADRs 0001-0002.
- Planning: `ROADMAP.md` (v0.1.0 to v2.0.0, tickets T-OR-001 to T-OR-105), `PROJECT-STATUS.md`,
  and eight phase plans in `to-dos/`.
- Artwork: icon, logo, hero banner and social card generated with `agy`; four SVG diagrams
  (architecture dark and light, detection pipeline, release roadmap); README header, badges,
  research findings, layout and documentation index, and Mermaid build-pipeline and install
  safety-gate diagrams.

### Security

- Antigravity reviewer re-synced from template `1451f73`: agy runs in a bubblewrap jail
  (`AGY_ISOLATION=bwrap`) with an empty private `HOME` (no MCP servers, skills, hooks or
  repositories), read-only `/usr` and `/etc`, and a D-Bus proxy exposing only its own keyring
  item. Its network is a TLS-intercepting egress proxy that allows only exact Google hosts and
  rebuilds every request. Credential-shaped output is redacted before posting, and anything
  redaction cannot remove stops the post. agy's own `--sandbox` was verified not to confine
  reads.
- `.gitignore` excludes device backups, firmware images, captures (`*.qmdl`, `*.qmdl.gz`,
  `*.ndjson`, `*.pcap`), Ghidra projects, extracted firmware, `wpa_sta.conf`, `.env` files and
  downloaded papers whose licenses do not permit redistribution.
- Device identifiers are kept out of the repository; test fixtures use a synthetic USB serial.
- `upstream-watch.yml` validates upstream tag names against strict SemVer and passes them to
  the shell through `env` instead of expression interpolation; every `actions/checkout` sets
  `persist-credentials: false`.
- Antigravity reviewer re-synced from the hardened template: agy runs from a private sandbox
  directory outside the checkout (large-diff handoff included), with an allowlisted environment
  (`env -i` plus the session variables its keyring-held login needs, so no job token or
  workflow `env:` value reaches it under any name), and post-agy `git` calls ignore hooks. The interim
  `AGY_DIFF_MODE: inline` workaround is removed, and with it the unused `/.agy-review-work/`
  ignore line.

### Added (CI)

- Antigravity PR reviewer (`antigravity-review.yml`, `scripts/agy-review*.sh`, `scripts/_agy_*.sh`,
  `.github/AGY-REVIEWER.md`), vendored from the `antigravity-pr-review` template: reviews every
  same-repository PR on open, reopen and push, and on `/agy-review`, using the self-hosted `agy`
  runner and the `agy` CLI's OAuth session. Project review rules in `.github/agy-review.md`.
  Fork PR workflows now require approval for all outside contributors.
- The reviewer's offline self-test runs in the `shellcheck` CI job.

### Changed

- CI is path-aware: `ci`, `docs` and `security` each begin with a `detect changes` job
  (`dorny/paths-filter`) and skip jobs whose inputs did not change, using job-level `if:` so
  required checks still report as passing. `gitleaks` always runs; scheduled and manual runs
  run every job. The detection jobs (`detect changes (ci|docs|security)`) are themselves
  required checks, so a detection failure blocks the merge instead of silently skipping every
  gated check (fail closed). See `docs/CI.md`.
- CI caching: Rust caches saved only from `main` (restored by PRs and releases) for the host
  crates and the patched upstream's third-party dependencies, rustup toolchain plus armv7
  target, npm, the SHA-256-verified gitleaks binary and lychee link results. Pull-request runs cancel superseded runs; every job has a timeout;
  build artifacts are retained 14 days.
- `scripts/apply-patches.sh` stages upstream plus patches in a scratch directory and syncs it
  into `build/rayhunter` by content (`rsync --checksum --delete`), never touching `target/` or
  `node_modules/`: incremental local builds and restored CI caches survive `scripts/build.sh`,
  and only changed files get a fresh mtime so Cargo never reuses a stale patched artifact.
  Requires `rsync`.
- Non-PR workflow runs get a per-run concurrency group, so a pending run on `main` is never
  replaced (and its changes never left unvalidated) by a later push.
- `cargo-audit` runs the pinned `cargo-audit` 0.23.0 CLI (installed by `taiki-e/install-action`)
  with `--deny warnings` instead of `rustsec/audit-check`, whose latest release targets the
  deprecated Node.js 20 runtime and required `checks`/`issues` write permissions.
- The docs `links` job also runs on any deletion or rename, since local link targets are not
  only Markdown.
- Rust caches use a `shared-key` so `release.yml` restores the caches `ci.yml` saves.
- `docs.yml` gains a weekly link-rot run.
- `main` protection moved from a classic branch protection rule to the `main` repository ruleset:
  same required checks (now pinned to the GitHub Actions app), linear history, no force pushes or
  deletions, PRs with resolved threads and squash merges; repository admins keep bypass.

### Fixed

- `orx doctor` printed `serial=?` for the RC400L in normal mode, which exposes no `serial` sysfs
  attribute and carries the serial in its product string (`MDM9207-MTP _SN:<serial>`). It now
  falls back to the `_SN:` token. The token is accepted only directly after a word-start `_SN:`
  marker, as 1-64 ASCII alphanumerics; a real `serial` attribute still takes precedence. The
  product string is split once when the device is read (`split_product()`): `product` keeps only
  the model text and the serial lives only in `serial`, so neither the doctor output nor a `Debug`
  print carries the serial twice (T-OR-106). Measured on the unit during the T-OR-012
  host-side recon.
- `scripts/apply-patches.sh` silently skipped every patch when staging inside this repository
  (the default `build/` layout): `git apply` discovered the superproject's `.git` and ignored
  patched paths outside its subdirectory while exiting 0. It now runs with
  `GIT_CEILING_DIRECTORIES` so it acts as plain `patch(1)`. Found by the Antigravity review; the
  series was still empty, so no build was affected. New regression test `tests/apply-patches.sh`
  (patch applied in and outside the repo, removal restores upstream with a fresh mtime,
  `target/` survives, missing and non-applying patches fail) runs in CI before every build.
- Change-detection jobs fetch full history, so on a push `dorny/paths-filter` can diff against
  `github.event.before` instead of treating every file as added in a shallow clone.
- `markdownlint` and `links` now cover every Markdown file, including `.github/` and the
  subdirectory READMEs; `lychee.toml` changes trigger only `links`; CodeQL also runs on
  `Cargo.lock` and `rust-toolchain.toml` changes.
- The armv7 build installs the upstream toolchain and target before `rust-cache` probes `rustc`;
  `release.yml` restores the rustup cache and no longer restores the dev-profile host cache.
- The reviewer's first-install bootstrap steps are removed (CodeQL `actions/untrusted-checkout`).
- The `review` job skips with a visible warning, instead of failing red, while the reviewer
  scripts are not yet on the default branch (the PR that installs it); any failure inside the
  reviewer still fails the job.
- `scripts/apply-patches.sh` canonicalizes the destination and patch directory: git ignores
  relative `GIT_CEILING_DIRECTORIES` entries, so a relative `$1` would again have skipped every
  patch. Tests now cover relative paths and assert that unchanged files keep their mtime.
- The patched-upstream Rust cache is no longer keyed on `patches/**` (rust-cache stores only
  third-party dependencies, so a patch edit discarded it for nothing); the lychee cache uses a
  per-run key and the scheduled run re-checks every link; scheduled and manual runs skip the
  unused full-history checkout; the exec-bit gate covers `tests/*.sh`.

- Scripts are committed with mode `100755`; the first CI run failed with `Permission denied`
  because the repository uses `core.fileMode=false`. CI now fails early if a script loses its
  executable bit.
