# Testing

Test layers, from host-only to on-device. CI runs layers 1-2 on every push
(`.github/workflows/ci.yml`, `docs.yml`, `security.yml`); layers 3-5 run locally and before every
release ([RELEASE-PROCESS.md](RELEASE-PROCESS.md)).

## 1. Host tests (this repository)

| Gate | Command |
|---|---|
| Format | `cargo fmt --all --check` |
| Lint | `cargo clippy --all-targets --locked -- -D warnings` |
| Unit tests | `cargo test --locked` (filter: `cargo test -p orx-cli <name>`) |
| Scripts | `shellcheck` (project scripts at full severity; vendored reviewer scripts at `-S error`) and `bash scripts/agy-review-selftest.sh` |
| Workflows | `actionlint` |
| Docs | `markdownlint-cli2 '**/*.md' '.github/**/*.md'` |

`orx` tests never touch real hardware: USB detection runs against a fake sysfs tree and network
probes against a local listener (`crates/orx-cli/src/usb.rs`, `probe.rs`). Every new subcommand
follows the same pattern: device interaction goes through a trait with a fake implementation in
tests, so backup, install and restore logic (including the fail-closed paths: missing backup, hash
mismatch, wrong USB mode) is tested without a device.

## 2. Patch-apply gate

```sh
git submodule update --init
scripts/apply-patches.sh          # stages build/rayhunter; fails if any patch does not apply
cd build/rayhunter
cargo fmt --all --check
cargo clippy
cargo test
npm run lint -w daemon/web && npm run check -w daemon/web && npm run test -w daemon/web   # if web code changed
cd - && scripts/build.sh devel    # armv7 daemon + rootshell into dist/
```

Upstream's CI runs `cargo clippy` without `-D warnings` and only for the host target, so
armv7-only code is not clippy-checked there
([ref-docs/rayhunter-upstream-overview.md](../ref-docs/rayhunter-upstream-overview.md) section 10).
Our patches must not add warnings; review clippy output rather than relying on the exit code.
On an upstream bump, `scripts/sync-upstream.sh <tag>` trial-applies the series
([PATCH-POLICY.md](PATCH-POLICY.md)).

## 3. Golden vectors for analyzers

A golden vector is a DIAG input plus the expected analyzer output. Policy:

1. **Synthetic first.** Build vectors from hand-constructed RRC/NAS messages encoded the way the
   analyzer receives them. Synthetic vectors contain no real identifiers and need no scrubbing.
   The harness is T-OR-051.
2. **Location.** Vectors that test upstream analyzers live inside the `[UPSTREAM]` patch, following
   upstream's own test layout (to be confirmed against `lib/` when T-OR-051 is authored), so the
   tests travel with the PR. Vectors for `[LOCAL]` behavior live in the corresponding patch.
3. **Real captures only scrubbed.** A real QMDL is used as a vector only after scrubbing and review:
   - replace IMSI, IMEI/IMEISV, GUTI/S-TMSI and other subscribers' S-TMSIs in paging with fixed
     dummy values of the same length and encoding;
   - replace or coarsen ECGI, TAC, PCI and EARFCN where the test does not depend on them;
   - drop GPS records (`*-gps.ndjson`) and any 0x11EB user-plane packets;
   - record what was scrubbed, by which tool version, and who reviewed it, next to the vector.
   Scrubbing can break home-PLMN logic; choose dummy PLMNs that preserve the tested condition
   ([ref-docs/legal-and-ethics.md](../ref-docs/legal-and-ethics.md) section 5.2).
4. **No proprietary or third-party data.** Published capture repositories are usable only under
   their licenses ([ref-docs/related-tools.md](../ref-docs/related-tools.md) section 4.3).
5. **Gitignore.** `*.qmdl` and `*.pcap*` are gitignored. Committing a reviewed vector needs an
   explicit `!path` negation in `.gitignore` in the same change, naming the scrub record.
6. **Changing expected output** is allowed only for an intentional, reviewed behavior change
   (analyzer version bump); the commit states why.
7. **Offline check.** Each vector also runs through `rayhunter-check`:

```sh
cargo run -p rayhunter-check -- -p <vector>.qmdl -j <out>.json -q   # inside build/rayhunter
```

## 4. On-device smoke list

Run after every install or daemon change (T-OR-023, `orx verify` automates the first items):

| # | Check | How |
|---|---|---|
| 1 | Daemon binary is ours | device `sha256sum /data/rayhunter/rayhunter-daemon` equals `dist/SHA256SUMS` |
| 2 | Daemon running | `ps w \| grep rayhunter-daemon` in the shell |
| 3 | API answers | `curl -s http://192.168.1.1:8080/api/system-stats` |
| 4 | Recording active | `curl -s http://192.168.1.1:8080/api/qmdl-manifest` shows a current entry; display status line green |
| 5 | QMDL grows | two `qmdl-manifest` reads one minute apart show a larger size |
| 6 | Analyzer path | temporarily enable `test_analyzer`, confirm SIB1 events in `/api/analysis-report/{name}`, disable it again |
| 7 | Start/stop | `curl -X POST .../api/stop-recording`, then `start-recording`; new entry appears |
| 8 | Download | `curl -s -o r.zip http://192.168.1.1:8080/api/zip/<name>` and `unzip -l r.zip` |
| 9 | Notification (if configured) | `curl -X POST http://192.168.1.1:8080/api/test-notification` |
| 10 | Log clean | `curl -s http://192.168.1.1:8080/api/log` shows no DIAG init error |
| 11 | Reboot persistence | `reboot`; the daemon and a new recording come back without intervention |
| 12 | Hotspot intact | a client joins the AP; `http://192.168.1.1/` loads; USB tethering works |
| 13 | Free-space guard | `df -h /data` above `min_space_to_continue_recording_mb` |
| 14 | Uninstall/restore | releases that change the install path only: [INSTALL.md](INSTALL.md) step 6 |

### On-device privacy check: 0x11EB (T-OR-019)

Question: does traffic of hotspot clients land in QMDL through `LOG_DATA_PROTOCOL_LOGGING_C`
(0x11EB)? Upstream subscribes to it and comments it "User IP traffic"
([ref-docs/qualcomm-diag-qmdl.md](../ref-docs/qualcomm-diag-qmdl.md) section 4.1,
[ref-docs/legal-and-ethics.md](../ref-docs/legal-and-ethics.md) section 3.1). Procedure, after the
verified backup and the OTA block, with the active SIM providing data:

1. Start a fresh recording.
2. From a hotspot client, fetch a unique marker, for example
   `curl -s http://example.com/?orx-marker-<random>` and a DNS lookup of a unique name.
3. Stop the recording, download the QMDL, `gunzip` if compressed.
4. On the host, count 0x11EB log packets and search for the marker:
   `grep -c -a 'orx-marker-<random>' <file>.qmdl` and decode with SCAT or `rayhunter-check -P`
   to PCAP.
5. Record the result in [DEVICE-PROFILE.md](DEVICE-PROFILE.md). If user traffic is present,
   T-OR-029 removes 0x11EB from our build and the generic option becomes T-OR-067.

The capture from this test contains real identifiers; delete it after recording the result.

**Answered from code (2026-11-23), empirical test deferred.** The privacy question is already
settled by the upstream source: `upstream/lib/src/diag_device.rs:57-58` subscribes to
`LOG_DATA_PROTOCOL_LOGGING_C` (`log_codes.rs:113` = `0x11eb`), commented "User IP traffic", so on
an active data plan a subscriber's IP packets ARE logged into the QMDL. This unit has no active
plan (built-in Verizon SIM, not activated), so no data session occurs and no user traffic is
produced to capture; the live confirmation and the connection-based heuristics are deferred until
an active plan or a shielded srsRAN lab is available
([ref-docs/running-without-active-service-2026-11-23.md](../ref-docs/running-without-active-service-2026-11-23.md)).
The mitigation (T-OR-029: drop `LOG_DATA_PROTOCOL_LOGGING_C` from our build's subscription) is
justified on the code regardless.

## 5. Optional shielded lab (srsRAN)

For exercising analyzers with real signalling. Only inside an RF-shielded enclosure (Faraday box
or shield tent) with cabled attenuators, a test PLMN such as 001/01 and a programmable test SIM.
Transmitting on licensed spectrum outside such a setup is illegal in the US without
authorization; nothing in this project transmits over the air
([ref-docs/legal-and-ethics.md](../ref-docs/legal-and-ethics.md) sections 1-2,
[ref-docs/related-tools.md](../ref-docs/related-tools.md) section 3).

| Item | Choice |
|---|---|
| Stack | srsRAN 4G (eNodeB + EPC), AGPL-3.0 |
| SDR | any srsRAN-supported device on the cabled path |
| Device | the RC400L with the test SIM, shielded with the SDR |
| Scenarios | identity request before authentication (`imsi_requested`), `RRCConnectionRelease` with redirect, EEA0/EIA0 security mode, Attach Reject with cause #9/#22, IMSI paging, SIB1-only cell |
| Expected | each scenario raises exactly the analyzer under test; a clean attach raises nothing above Informational |

Captures from the lab carry only test identities and are the preferred source of real-format
golden vectors. Devilray reported the RC400L failing mobile-data attach in their lab
([ref-docs/lte-imsi-catcher-detection.md](../ref-docs/lte-imsi-catcher-detection.md) section 2);
whether it attaches to our lab network is TBD (on-device).
