# Phase 5: Alerting, analyzers, storage (v1.1.0..v1.3.0)

Goal: extend upstream's notification path, close the published detection blind spots, and make
storage self-limiting. Tickets T-OR-045..T-OR-068 ([ROADMAP.md](../ROADMAP.md)). Nearly all work
is `[UPSTREAM]`: before each ticket, search EFForg/rayhunter issues and PRs and link the result
in the patch body.

## Sprint 5.1: Alerting

Host-side enablement shipped first (no firmware rebuild needed): upstream v0.13.0 already delivers
`Warning`/`LowBattery` notifications to `ntfy_url` with debounce + backoff, so these just surface and
drive that path over `:8080`.

- [x] T-OR-044 (v1.1.0): `orx alert status`/`test` -- show the configured `ntfy_url` + enabled types
      and send a test notification.
- [x] T-OR-107 (v1.1.1): `orx alert set --ntfy-url <url>|--clear [--apply]` -- write/clear `ntfy_url`
      via a faithful read-modify-write of `/api/config` (dry-run unless `--apply`; validated live
      with a full before/after config diff showing only `ntfy_url` changed).

The daemon sink EXTENSIONS below are `[UPSTREAM]` v1.2.0 patches (`patches/0001-0004`, host-validated
on upstream's own gates; on-device validation batched with the analyzers phase):

- [x] T-OR-045 (patch 0001): ntfy `Title`, `Priority`, `Tags` headers from the notification type
      (opt-in `ntfy_headers`); absent config keeps today's plain POST.
- [x] T-OR-046 (patch 0002): optional `ntfy_auth` Authorization header (bearer/basic); secret,
      redacted + preserved + dropped-on-url-change.
- [x] T-OR-047 (patch 0003): generic `webhook_url` JSON sink `{source, type, message, timestamp}`,
      no identifiers; multi-sink worker; webhook_url redacted, error URLs stripped from logs.
- [ ] T-OR-048: MQTT sink -- DEFERRED past v1.2.0 (heavy armv7 dependency; needs upstream agreement).
- [x] T-OR-049 (patch 0004): Gotify sink (`gotify_url`/`gotify_token`, X-Gotify-Key, priority by type).
- [ ] T-OR-050: persistent retry queue on `/data` -- DEFERRED past v1.2.0 (overlaps v1.3.0 storage).

Acceptance criteria: each sink tested against a local server in unit or integration tests; a
payload test asserts no IMSI/IMEI/GUTI pattern; existing `ntfy_url` behavior byte-identical when
new keys are absent; [docs/CONFIGURATION.md](../docs/CONFIGURATION.md) updated.

Verification:

```sh
scripts/apply-patches.sh
(cd build/rayhunter && cargo test -p rayhunter-daemon notifications)
curl -X POST http://192.168.1.1:8080/api/test-notification
```

### Research (REA/Ghidra/web)

- Upstream `daemon/src/notifications.rs` (queue of 10, debounce, retry, 10 s timeout) as the
  extension point.
- Upstream issues #942, #770, #778 and discussion #1173 (ntfy problems, no-internet case);
  discussion #777 (Raspberry Pi bridge to MQTT/Home Assistant) for requirements.
- ntfy and Gotify HTTP API documentation (current versions) for header and body formats.

## Sprint 5.2: Analyzer harness and causal NAS analyzers (v1.2.0, part 1)

- [x] T-OR-051: harness for synthetic RRC/NAS vectors and golden outputs (patch 0006, PR #19);
      the real-capture scrubber is deferred to T-OR-061 (needs on-device captures)
- [x] T-OR-052: NAS Reject identity-forcing analyzer (patch 0007, PR #20; stateless #9/#10)
- [x] T-OR-053: authentication failure / reject analyzer (patch 0008, PR #21; stateless)
- [ ] T-OR-054: own identity disclosure analyzer -- **BLOCKED on T-OR-110**. This `pycrate-rs`
      leaves the mobile-identity structs (`EPSID`, `ID`) empty and `Type4LV` keeps only the length,
      so IMSI-vs-IMEI-vs-GUTI is unreadable; a blind "any IdentityResponse" detector would be too
      noisy to ship. Not a live blind spot -- upstream `imsi_requested` already flags the request
      side. Build after T-OR-110, in the T-OR-061 device session.
- [x] T-OR-055: EIA0 null-integrity analyzer -- NAS half landed (patch 0009, `null_integrity`:
      NAS SMC EIA0 -> High). RRC-SMC-EIA0 and DRB-before-SMC ("security never activated") parts
      deferred to **T-OR-111** (need RRC/ASN.1 vectors in the harness).
- [ ] T-OR-111: T-OR-055 follow-ups -- (a) RRC SMC EIA0 + DRB/measurement-without-SMC (needs RRC
      vectors); (b) stateful emergency suppression (downgrade EIA0 SMC after an emergency
      `AttachRequest`). Until (b), the stateless detector accepts emergency calls as a known FP.

Acceptance criteria: each analyzer has its own version, config flag (default off), heuristics
doc entry, and positive and negative golden vectors; timeouts use packet timestamps.

> **T-OR-110 (enabling, do during the T-OR-061 device session):** hand-add EPS mobile-identity IE
> parsing (type nibble + BCD digits, TS 24.301 9.9.3.12) to `EPSID`/`ID` in EFForg/pycrate-rs (or
> fix its generator), bump the Cargo.lock pin, and validate against real DIAG captures. Unblocks
> T-OR-054 and the identity-correlation part of T-OR-108; sharpens T-OR-056. Written just
> before/during the device session so real captures validate it -- not built blind.

Verification:

```sh
scripts/apply-patches.sh
(cd build/rayhunter && cargo test -p rayhunter analysis)
(cd build/rayhunter && cargo run -p rayhunter-check -- -p <vectors-dir> -j out.json -q)
```

### Research (REA/Ghidra/web)

- Devilray (arXiv 2605.19232, local PDF) section on NAS Reject IMSI catching and cause #9.
- 3GPP TS 24.301 sections 5.4.2, 5.4.4, 5.5.1.2.5, 5.5.3.2.5, 5.6.1.5, 4.4.4.2 and Annex A.
- 3GPP TS 33.401 (EIA0 permitted only for unauthenticated emergency sessions).
- Upstream `lib/src/analysis/imsi_requested.rs`, `diagnostic.rs` (`epsid` gap at lines 65-70),
  `null_cipher.rs`, `nas_null_cipher.rs`, `information_element.rs`.

## Sprint 5.3: RRC/paging analyzers and plumbing (v1.2.0, part 2)

> These are the RRC-dependent detectors. Upstream Rayhunter unit-tests NONE of its RRC analyzers,
> so there is no synthetic-RRC-vector pattern and hand-encoding UPER PDUs blind is error-prone.
> They are all blocked on **T-OR-112** (RRC golden-vector harness), which is best seeded from real
> captures in the T-OR-061 device session. This is the off-device detector boundary: the
> NAS-decodable tranche (T-OR-051/052/053/055) is done; the rest needs the device.

- [ ] T-OR-112: RRC golden-vector harness -- extend `test_util` with PCCH/DL-DCCH/SIB builders
      (wrap bytes -> `LteInformationElement` via `lte_rrc` decode), seeded from T-OR-061 captures
- [ ] T-OR-056: IMSI paging analyzer; IMSI read from the SIM via AT and held only as a salted
      in-memory hash -- **blocked on T-OR-112** (PCCH = RRC) and the device IMSI (AT, T-OR-061)
- [ ] T-OR-057: dedicated-priority and sunset-RAT downgrade, SIB8; MCC policy table as config --
      **blocked on T-OR-112** (RRC)
- [ ] T-OR-058: measurement plumbing (new `InformationElement` input for ML1 and RACH TA);
      upstream issue opened before the patch
- [ ] T-OR-059: recording-scoped cell identity consistency
- [ ] T-OR-060: recording-level identity-exposure summary in the report

Acceptance criteria: as Sprint 5.2; T-OR-058 merged or agreed upstream before T-OR-059 depends on
PCI data.

Verification:

```sh
(cd build/rayhunter && cargo test -p rayhunter analysis && cargo clippy)
```

### Research (REA/Ghidra/web)

- Upstream `lib/src/diag/diaglog/ml1.rs`, `ll1.rs`, `lib/src/gsmtap/parser.rs:159`,
  `lib/src/sim/generic_at.rs`, `analysis/priority_2g_downgrade.rs`,
  `connection_redirect_downgrade.rs`.
- 3GPP TS 36.331 (`RRCConnectionRelease`, `idleModeMobilityControlInfo`, SIB8, PCCH) and TS 36.304.
- Marlin (NDSS 2025) message list and baseline; SeaGlass (PoPETs 2017) impermanence signatures.
- MobileInsight and SCAT decoders for 0xB17F/0xB180/0xB062 layouts to cross-check `ml1.rs`.

## Sprint 5.4: Calibration and stretch analyzers (v1.2.0, part 3)

- [~] T-OR-061: device session STARTED 2026-11-26 -- patched daemon (patches 0001-0009) installed;
      all 3 detectors enabled on-device and live in the capture pipeline. Calibration tooling landed
      (`scripts/calib-run.sh` + `scripts/calib-snapshot.py`, from the daemon's analysis NDJSON).
      Initial offline baseline (via `rayhunter-check` with a **local, uncommitted** patch enabling the
      default-off detectors -- the committed check runs only default-on analyzers until T-OR-113):
      0 firings over ~600k messages of real Verizon traffic (5 Attach Rejects seen, all cause #7 --
      correctly not flagged). Treat as indicative pending T-OR-113 / the live-NDJSON soak. Soak continues.
- [ ] T-OR-061: negative baselines (days of normal use on Verizon; AT&T and T-Mobile SIMs if
      available); false-positive rate per analyzer; choose severities; decide default on/off
- [ ] T-OR-113: `rayhunter-check --config`/`--all-analyzers` so archived QMDL can be re-scored with
      default-off detectors (the offline calibration path); the live daemon already runs them
- [ ] T-OR-061: ask EFF whether Devilray profiles can be run against candidates 1-6
- [ ] T-OR-110: write the `pycrate-rs` EPS mobile-identity parsing against the real DIAG captures
      taken here (ground-truth IMSI/IMEI/GUTI bytes), then build T-OR-054 and the T-OR-108
      identity-correlation on top. Done here, in the device session, because the parser needs real
      data to validate and this is the only place that data exists.
- [ ] T-OR-062: SIB reselection-parameter features (stretch)
- [ ] T-OR-063: emergency-alert spoofing context (stretch)
- [ ] T-OR-064: RRC Connection Reject DoS context (stretch)
- [ ] open upstream PRs for each merged-ready analyzer; update [docs/ANALYZERS.md](../docs/ANALYZERS.md)

Acceptance criteria: per-analyzer false-positive counts recorded with capture hours; no analyzer
enabled by default without data.

Verification:

```sh
cargo run -p orx-cli -- pull-logs --out ~/orbic-captures/
(cd build/rayhunter && cargo run -p rayhunter-check -- -p ~/orbic-captures/ -j baseline.json -q)
```

### Research (REA/Ghidra/web)

- EFF "Rayhunter: What We Have Found So Far" (2025-09-10) for field false-positive experience.
- Upstream issue #543 (spurious identity-request warnings in low signal on the Orbic).

## Sprint 5.5: Storage and retention (v1.3.0)

- [ ] T-OR-065: on-device retention (size and age caps; oldest unflagged first; current recording
      never deleted)
- [ ] T-OR-066: root cause of the #1192 guard failure; `rayhunter.log` rotation
- [ ] T-OR-067: configurable DIAG log-code subscription (generalizes T-OR-029)
- [ ] T-OR-068: measure storage growth per hour (idle and active LTE) and set our caps and
      free-space thresholds from data

Acceptance criteria: fill test to the cap keeps the hotspot and web UI working; growth rates in
[docs/PERFORMANCE.md](../docs/PERFORMANCE.md); CONFIGURATION.md thresholds no longer provisional.

Verification:

```sh
# device shell, during the fill test
df -h /data; du -sk /data/rayhunter/qmdl; ls -l /data/rayhunter/rayhunter.log
# host
curl -s http://192.168.1.1:8080/api/system-stats
```

### Research (REA/Ghidra/web)

- Upstream `daemon/src/qmdl_store.rs` (manifest, `recover`), `daemon/src/diag.rs` (disk checks).
- Issues #607, #819, #152, #510 (storage management history).
