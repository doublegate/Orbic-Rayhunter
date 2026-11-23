# Phase 3: Reproducible patched build deployed (v0.4.0)

Goal: our build from `patches/` runs on the device, identifies itself, can be rolled back, and its
relationship to the upstream release binary is measured. Tickets T-OR-026..T-OR-031
([ROADMAP.md](../ROADMAP.md)). References: [docs/BUILD.md](../docs/BUILD.md),
[docs/PATCH-POLICY.md](../docs/PATCH-POLICY.md).

## Sprint 3.1: Reproducibility

- [x] T-OR-026/T-OR-030: `scripts/build.sh firmware` with the empty series, in the
      `messense/rust-musl-cross:armv7-musleabihf` container (2026-11-23). build.sh now runs the
      cargo build in that container, so no host cross toolchain is needed
- [x] T-OR-026: compared to the upstream v0.13.0 release daemon (from the release zip's
      `rayhunter-daemon/` folder)
- [x] T-OR-026/T-OR-030: result recorded in [re/notes/reproducible-build.md](../re/notes/reproducible-build.md)
      and [docs/BUILD.md](../docs/BUILD.md) -- within 384 bytes of upstream, same
      profile/toolchain/lockfile/pq-tls; not bit-identical (embedded build paths differ;
      `--remap-path-prefix` is the upstreamable follow-up)
- [x] T-OR-031: `release.yml` now ships the production `firmware` daemon (was `firmware-devel`),
      so `orx-cli install` / the v1.0.0 install deploys our production build

Acceptance criteria: comparison recorded with commands; byte identity achieved or each source of
difference identified. MET (difference identified: embedded build paths).

Verification:

```sh
scripts/build.sh firmware
ls -l dist/ && sha256sum dist/rayhunter-daemon
llvm-readelf -S dist/rayhunter-daemon > ours.sections
llvm-readelf -S <upstream-daemon> > upstream.sections && diff ours.sections upstream.sections
cmp dist/rayhunter-daemon <upstream-daemon> && echo identical
```

### Research (REA/Ghidra/web)

- REA `compare_artifacts` / `binary_overview` on our daemon versus the upstream release daemon
  (build IDs, embedded paths, embedded web assets).
- Upstream `installer/build.rs` (`FIRMWARE_PROFILE`, how binaries are embedded) and
  `.github/workflows/release.yml` (exact build environment of the release).

## Sprint 3.2: First patches

- [ ] T-OR-027: `[UPSTREAM]` patch fixing `doc/heuristics.md` (SIB6 carries UTRA, SIB8 carries
      CDMA2000; IMSI Requested heading version matches `get_version()` = 5); open the upstream PR
- [ ] T-OR-028: `[LOCAL]` patch adding a build identifier (for example a version suffix) visible in
      `/api/system-stats`
- [ ] T-OR-029: only if T-OR-019 found user traffic: `[LOCAL]` patch removing 0x11EB from
      `LOG_CODES_FOR_RAW_PACKET_LOGGING`, with a unit test asserting its absence
- [ ] each patch: `git format-patch` per PATCH-POLICY.md, listed in `patches/series`

Acceptance criteria: `scripts/apply-patches.sh` applies the series; upstream gates pass in
`build/rayhunter`; CI builds the patched daemon.

Verification:

```sh
scripts/apply-patches.sh
(cd build/rayhunter && cargo fmt --all --check && cargo clippy && cargo test)
scripts/build.sh devel
```

### Research (REA/Ghidra/web)

- Upstream `daemon/src/stats.rs` (where runtime metadata and version are reported) for T-OR-028.
- Upstream open PRs touching `doc/heuristics.md`, to avoid a duplicate of T-OR-027.

## Sprint 3.3: Deploy, roll back, release

- [ ] T-OR-030: manual deploy of `dist/rayhunter-daemon` per INSTALL.md step 4 manual equivalent;
      previous binary kept as `rayhunter-daemon.prev`
- [ ] T-OR-030: smoke list passes with our binary; build identifier shown
- [ ] T-OR-030: roll back to the release binary and verify; then redeploy ours
- [ ] T-OR-031: release dry run (`release.yml` on a test tag in a fork or with `workflow_dispatch`),
      then tag v0.4.0 per RELEASE-PROCESS.md

Acceptance criteria: device runs our build after reboot; rollback verified by hash; v0.4.0 assets
attached and checksummed.

Verification:

```sh
# device shell
sha256sum /data/rayhunter/rayhunter-daemon     # equals dist/SHA256SUMS entry
# host
curl -s http://192.168.1.1:8080/api/system-stats | grep -i version
gh release view v0.4.0
```

### Research (REA/Ghidra/web)

- Upstream `make.sh` dev loop (daemon stop, push, reboot) as the reference for the manual deploy
  sequence; note it is ADB-based and adapt it to the network shell.
