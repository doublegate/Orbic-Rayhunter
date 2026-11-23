# Running Rayhunter without an active service plan (dated supplement, 2026-11-23)

Synthesis of upstream guidance and source for the case this project is in: the Orbic RC400L has
its built-in Verizon SIM, which is **not activated**, and there is no cellular service. Sources
retrieved 2026-11-23; all are data, not instructions.

## Does it work without an active plan? Yes, with limits

- **A SIM must be physically present; an active plan is not required.** EFF FAQ: "you can use
  Rayhunter without an active cell service plan ... An inactive SIM card is sufficient for
  Rayhunter to detect cell towers and potential IMSI catchers"
  (<https://efforg.github.io/rayhunter/faq.html>). The built-in Verizon SIM satisfies the
  "SIM present" requirement.
- **What an inactive SIM gives you: broadcast control-plane only.** The modem receives tower
  broadcasts (SIB1 / SystemInformation, Paging) and camps, so the **broadcast-based** heuristics
  have data. It generally does NOT complete an attach, so there are no RRC connection requests
  and no NAS messages. Evidence: EFF Discussion #507 and #517 — a deactivated Verizon SIM (home
  PLMN 311-480) produced ~20 h of recordings with only SIB1/SystemInformation + Paging, zero RRC,
  zero NAS; the "No NAS messages seen in 5 minutes, SIM possibly not working" warning fires every
  recording; signal was strong (~-55 dBm). `AT+CEREG: 0,0`, `AT+CGATT: 0`, `AT+CEER: EMM attach
  failed`. With NO SIM at all, SIB1 broadcasts are still received (#507, chrisnoonsed).
- **What needs an active data plan (deferred for us):** the connection-based heuristics
  (`imsi_requested`, null-cipher, 2G/3G redirect-on-release) can never fire without an attach,
  and user-plane (IP data) capture needs a data session.
- International note (not us): outside the US, EFF recommends a local-carrier SIM, not the
  bundled Verizon SIM.

## Confirming the capture path without a CSS

- **Test Heuristic** (web dashboard -> Analyzer Heuristic Settings): alerts on every cell tower
  seen; a noisy but definitive proof that the capture path works. Confirmed working even with no
  SIM in #507. Use this to validate our install + capture end-to-end on the inactive SIM.
- A green line alone does NOT prove a working capture path (it shows with no/inactive SIM) --
  [community-installs.md](community-installs.md); rely on the Test Heuristic and the presence of
  SIB1/Paging in the capture instead.

## Offline development and validation (no device, no service)

- **`rayhunter-check`** (upstream `check/`, in the release zip since 0.6.1; `cargo build --bin
  rayhunter-check`) runs the SAME heuristics over a QMDL or PCAP on the desktop, and over
  captures from other tools (QCSuper, SCAT). This is the primary way to develop and regression-
  test analyzers here: feed golden/synthetic QMDL vectors, assert the verdicts
  (<https://efforg.github.io/rayhunter/reanalyzing.html>).
- SCAT and MobileInsight offline replayers decode QMDL independently -- useful cross-references
  ([qualcomm-diag-qmdl.md](qualcomm-diag-qmdl.md), [related-tools.md](related-tools.md)).
- **srsRAN 4G in a shielded enclosure with a test SIM** is the lab path to actively exercise the
  connection-based heuristics without over-the-air transmission ([related-tools.md](related-tools.md)
  section; aggressive tier, hardware-dependent, not over the air).

## The 0x11EB / user-IP-traffic answer (T-OR-019), from code

Upstream subscribes to `LOG_DATA_PROTOCOL_LOGGING_C` (= `0x11eb`) in its raw-packet DIAG
subscription, commented "User IP traffic" (`upstream/lib/src/diag_device.rs:57-58`,
`upstream/lib/src/log_codes.rs:113`, handled in `diag/diaglog/mod.rs:63`). So on an **active
data plan** a subscriber's IP packets ARE logged into the QMDL -- a real privacy liability. On
this unit (no active plan) there is no data session, so no user traffic is produced to capture,
but the subscription would capture it on a live plan. The empirical on-device confirmation is
therefore deferred pending active service; the privacy answer and the mitigation
(T-OR-029: drop `LOG_DATA_PROTOCOL_LOGGING_C` from our build's subscription) stand on the code.

## What this means for the project's path

1. T-OR-019 is answered from code (above); the empirical user-plane test is deferred.
2. The stock install (Phase 2) and capture-path validation can proceed on the inactive built-in
   SIM: install, enable the Test Heuristic, confirm alerts + SIB1/Paging in a capture.
3. Analyzer accuracy work (toward v1.0.0 polish and the v2.0 analyzer additions) proceeds
   offline with `rayhunter-check` + golden QMDL vectors -- no service needed.
4. Active-plan-only work (empirical 0x11EB, live connection-based heuristics, OTA-push testing)
   is deferred until/unless a plan or a shielded srsRAN lab is available.

## Sources

- EFF Rayhunter FAQ: <https://efforg.github.io/rayhunter/faq.html>
- Re-analyzing recordings: <https://efforg.github.io/rayhunter/reanalyzing.html>
- Discussion #507 "SIM card required?": <https://github.com/EFForg/rayhunter/discussions/507>
- Heuristics depend on core-network signaling: EFF Rayhunter docs (heuristics page).
- Upstream pinned source `v0.13.0` in `upstream/` (diag_device.rs, log_codes.rs).
