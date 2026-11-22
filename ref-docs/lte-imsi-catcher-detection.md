# Detecting Fake Base Stations from a UE: Heuristics Survey (2G/3G/LTE/5G-NSA)

- Retrieved: 2026-11-22
- Upstream ref: EFForg/rayhunter v0.13.0 (cd462a1), local read-only checkout at `./upstream`
- Scope: UE-side (single-device, passive-receive) detection heuristics for cell-site simulators
  (CSS, IMSI catchers, fake base stations, FBS) as they apply to Rayhunter on the Orbic RC400L.
  For each heuristic: mechanism, what is observable in Qualcomm DIAG / RRC / NAS logs, false-positive
  sources, upstream status (with file citations), and an upstreamability note. Ends with a ranked
  candidate list for our v1.2 analyzers.
- Convention: "unverified" marks a statement that could not be sourced from a primary document or
  the upstream code; "TBD (on-device)" marks something this project must measure on the RC400L.
- Companion docs: `ref-docs/qualcomm-diag-qmdl.md` (log codes, QMDL format),
  `ref-docs/rayhunter-upstream-overview.md`, `ref-docs/legal-and-ethics.md`.
- PDFs referenced as `pdfs/<file>` are local copies (gitignored); see `ref-docs/pdfs/README.md`.

---

## 1. What upstream Rayhunter can and cannot see today

This determines which heuristics are cheap (data already reaches the analyzer trait) and which need
plumbing first.

### 1.1 The analyzer input surface

`upstream/lib/src/analysis/information_element.rs` converts GSMTAP-wrapped DIAG log payloads into an
`InformationElement`. Only two families are decoded:

| Decoded into analyzers | Source DIAG log code | Notes |
| --- | --- | --- |
| LTE RRC: BCCH-BCH (MIB), BCCH-DL-SCH (SIB1, SI messages carrying SIB2..), PCCH (paging), DL/UL-CCCH, DL/UL-DCCH, MCCH, SC-MCCH and others | `LOG_LTE_RRC_OTA_MSG_LOG_C` 0xB0C0 | ASN.1 PER decoded by `telcom-parser` |
| LTE NAS (EMM/ESM), plain | `LOG_LTE_NAS_EMM_OTA_IN/OUT` 0xB0EC/0xB0ED, `ESM` 0xB0E2/0xB0E3 | `pycrate_rs`; only `LteNasSubtype::Plain` is accepted |

`InformationElement::GSM`, `::UMTS` and `::FiveG` exist as empty variants; nothing decodes into them.

### 1.2 Logged but not analyzable

`upstream/lib/src/diag_device.rs` (`LOG_CODES_FOR_RAW_PACKET_LOGGING`, lines 43-65) subscribes to more
than the analyzers consume:

| Log code | Content | Parsed? | Reaches analyzers? |
| --- | --- | --- | --- |
| 0x512F `LOG_GSM_RR_SIGNALING_MESSAGE_C` | 2G RR signalling | no | no |
| 0x5226 GPRS MAC | 2G packet signalling | no | no |
| 0x412F `WCDMA_SIGNALLING_MESSAGE`, 0x713A UMTS NAS | 3G RRC / NAS | no | no |
| 0xB821 `LOG_NR_RRC_OTA_MSG_LOG_C` | 5G NR RRC | no | no |
| 0xB17F ML1 serving cell meas and eval | RSRP / RSRQ / RSSI, PCI, EARFCN | yes, `lib/src/diag/diaglog/ml1.rs` | no |
| 0xB180 ML1 neighbor meas | per-neighbor PCI / RSRP / RSRQ | yes, `ml1.rs` | no |
| 0xB062 MAC RACH response | contains the RAR Timing Advance | converted to GSMTAP MAC (`lib/src/gsmtap/parser.rs:159`, `gsmtap/mac.rs`) | no (`InformationElement::try_from` rejects non-RRC/NAS GSMTAP types) |
| LL1 serving cell timing | `timing_advance`, `starting_ul_timing_advance` | yes, `lib/src/diag/diaglog/ll1.rs` | no |
| 0x11EB `LOG_DATA_PROTOCOL_LOGGING_C` | commented "User IP traffic" in `diag_device.rs:57` | no | no (privacy relevance: see `legal-and-ethics.md`) |

Implication: every signal-strength, PCI, and timing-advance heuristic in this survey requires an
`InformationElement` (or parallel trait input) extension before an analyzer can be written. The
upstream TODO in `imsi_requested.rs:231` ("we should improve this once we have PCI data") shows the
maintainers already want this.

### 1.3 RAT scope of the RC400L

The RC400L is a US LTE hotspot (upstream `doc/orbic.md`; its band table lists LTE bands
2/4/5/12/13/48/66). US 2G and 3G networks are shut down (AT&T UMTS and Verizon CDMA in 2022,
T-Mobile UMTS 2022, T-Mobile GSM later). Which legacy RATs the RC400L baseband will actually camp on
is TBD (on-device): the device's own `UECapabilityInformation` (UL-DCCH, already decoded) lists the
supported RATs and bands and is the authoritative check. Consequences for this project:

- 2G/3G heuristics matter mainly as LTE-side *pointers* toward legacy RATs (redirection, SIB6/7/8
  priorities, dedicated priorities), not as 2G/3G message analysis.
- 5G-NSA matters through the LTE anchor: EN-DC SCG configuration arrives inside LTE
  `RRCConnectionReconfiguration`, which upstream already inspects for EEA0
  (`null_cipher.rs:215-227`). NR RRC decoding (0xB821) is only relevant for other Rayhunter hardware.

### 1.4 Upstream analyzers (v0.13.0) at a glance

| Analyzer | File | Version | Default | Severity emitted |
| --- | --- | --- | --- | --- |
| Identity (IMSI/IMEI) requested in suspicious manner | `lib/src/analysis/imsi_requested.rs` | 5 (doc heading says v3) | on | High / Low / Informational |
| Connection Release / Redirected Carrier 2G Downgrade | `connection_redirect_downgrade.rs` | 1 | on | High (GERAN) / Informational (other) |
| LTE SIB 6/7 Downgrade | `priority_2g_downgrade.rs` | 2 | on | High / Informational |
| Null Cipher (RRC EEA0) | `null_cipher.rs` | 1 | on | High |
| NAS Null Cipher (EEA0) | `nas_null_cipher.rs` | 1 | on | High |
| Incomplete SIB | `incomplete_sib.rs` | 2 | on | Informational |
| Diagnostic (Marlin-derived IMSI-exposing message list) | `diagnostic.rs` | 1 | on | Informational |
| No NAS Messages | `no_nas_messages.rs` | - | off | (warning once per recording) |
| Test Analyzer | `test_analyzer.rs` | - | off | noisy |

Defaults are in `analyzer.rs:37-50` (`AnalyzerConfig::default`).

Two upstream documentation errors worth a small upstream PR (independent of new analyzers):

- `doc/heuristics.md` says "SIB6 is used for cell reselection to CDMA2000 systems". Per 3GPP TS
  36.331, SIB6 carries UTRA (FDD/TDD) reselection info and SIB8 carries CDMA2000; the code
  (`priority_2g_downgrade.rs:133-162`) correctly reads `carrier_freq_list_utra_fdd/tdd` from SIB6.
- The IMSI Requested section is headed "(v3)" while `get_version()` returns 5.

---

## 2. Threat model anchors from the literature

The most important recent result for detector design is Devilray (Oh et al., arXiv 2605.19232,
2026; `pdfs/2026-Oh-Devilray-FBS-Detection-Blind-Spots-arXiv.pdf`), which reports the first academic
analysis of a commercial FBS (C-FBS) sold to law enforcement for about USD 50K, observed through UE
diagnostic logs (SCAT) and the device's own logs. Key observed behaviors:

- Reconnaissance: scans nearby cells for PLMN, RAT, PCI, cell ID, ARFCN, TAC and signal strength.
- Automated configuration initializes PCI, TAC and cell ID with random values; operators can also
  clone legitimate cells.
- Broadcasts MIB and SIB1-5; fixed 5 MHz bandwidth due to hardware.
- IMSI catching via NAS Identity Request, then disconnect via NAS Reject. Reject cause is operator
  selectable (#13 roaming not allowed in TA, #22 congestion, #42 severe network failure); the
  Devilray authors found cause #9 was the only cause that consistently forced identity exposure in
  a single-cell setup across six COTS UEs from three baseband vendors.
- Location tracking by paging plus Identity Request to elicit uplink for RSRP/propagation-delay
  ranging.
- Downgrade/SMS injection by RRC redirection to a concurrent 2G cell, or LTE-to-3G to defeat
  "disable 2G" toggles; dual-channel multi-RAT operation.
- Frequency/RAT hopping with configurable dwell; adjustable power (2-10 m effective range without
  amplifiers).
- DoS via Attach Reject #22: UEs stayed offline up to 30 minutes, surviving airplane-mode toggles and
  reboots.

Devilray evaluated seven accessible detectors including Rayhunter and found gaps in all. The
Rayhunter-specific finding: its design "posits identity disclosure is driven exclusively by explicit
Identity Request or IMSI paging", creating a blind spot for IMSI catching via NAS Reject (their "IR"
instance). Devilray was shared with EFF to improve Rayhunter. The paper also notes the RC400L failed
mobile-data attach in their lab, so handover and reselection variants were not exercised against
Rayhunter.

Marlin (Tucker et al., NDSS 2025; `pdfs/2025-Tucker-Marlin-Identity-Exposing-Messages-NDSS.pdf`)
argues detectors should key on *causal* messages (those an IMSI catcher must send) rather than
*correlated* behaviors (cell config changes, which also occur with carrier reconfiguration or a
cell-on-wheels at an event). It enumerates 53 pre-authentication messages across 2G/3G/4G/5G-NSA that
can elicit an IMSI, measures over 400 hours on two continents (median under 3% of LTE connections
contain an IMSI-exposing message), and reports a statistically significant (p << 0.005) anomaly at a
high-profile public event. Upstream's `diagnostic.rs` already cites "the 'Marlin' paper" for its list.

EFF's own field results (Quintin, "Rayhunter: What We Have Found So Far", 2025-09-10): no CSS evidence
at US protests; likely CSS detections in Chicago and New York from repeated "IMSI sent without
authentication" triggers over a couple of hours; a multi-detection case at a Turks and Caicos cruise
port that "inspired us to write some new detection rules"; Rayhunter detected every attack run by a
commercial CSS during testing with Cape; the 2G heuristic is false-positive-prone in countries with
live 2G networks.

General principle used for the severity notes below: a single correlated anomaly should be
Informational or Low; High should be reserved for causal, protocol-impossible-on-a-real-network
events (EEA0/EIA0 outside emergency, identity exposure without authentication, unauthenticated
rejects that are only meaningful as attacks).

---

## 3. Heuristic catalogue

Each entry: Mechanism / Observable / False positives / Upstream status / Upstreamability.

### 3.1 Identity requests (IMSI, IMEI, IMEISV)

- Mechanism: before authentication the network may send NAS `Identity Request` (TS 24.301 5.4.4);
  the UE must answer with the requested identity in cleartext. A CSS has no keys, so it requests the
  IMSI (and often IMEI) and then releases or rejects the UE. In 2G the equivalent is MM Identity
  Request; in 3G, the same MM/GMM procedure. Classic: Dabrowski et al. ACSAC 2014, SeaGlass
  ("requesting an IMSI when a TMSI would be appropriate"), White-Stingray (Park et al. WOOT 2017:
  none of five Android detector apps resisted basic identity catching).
- Observable: DL NAS `EMMIdentityRequest` with `id_type` (IMSI / IMEI / IMEISV / TMSI) on 0xB0EC;
  the UE's own `EMMIdentityResponse` on 0xB0ED carrying the actual mobile identity; ordering against
  `RRCConnectionRequest`, `EMMAttachRequest`, `EMMAuthenticationRequest/Response`,
  `SecurityModeCommand`, `RRCConnectionRelease`. A second, more causal observable: the UE's uplink
  `AttachRequest` whose EPS mobile identity is an IMSI rather than a GUTI.
- False positives: first attach after power-on with no stored GUTI; GUTI invalidated after long
  airplane mode; roaming to a network that cannot resolve the GUTI to the old MME; core network
  restarts; inactive SIM (upstream treats Attach Reject #8 as "likely valid"); airplane landings
  (upstream doc). IMEISV requests inside the Security Mode procedure (IMEISV request flag) are
  normal and must not be confused with an Identity Request.
- Upstream status: implemented. `imsi_requested.rs` (state machine: Unattached -> AttachRequest ->
  IdentityRequest -> AuthAccept/Disconnect; High when Identity Request is followed by Disconnect
  without auth on the home PLMN, or Identity Request after AuthAccept / without Attach; Low for
  roaming and #8; Informational after a 50-message timeout). PLMN context comes from SIB1 and the
  SIM's EF_HPLMNwAcT (`lib/src/sim/generic_at.rs`). `diagnostic.rs` emits Informational for every
  Identity Request. Gaps: (a) `id_type` is not used in severity (IMEI-only requests are a distinct
  CSS fingerprint); (b) the UE's uplink Identity Response / IMSI-type Attach Request is not checked,
  because `pycrate_rs` does not parse `epsid` (comment at `diagnostic.rs:65-70`); (c) the timeout is
  message-count based, not time based.
- Upstreamability: high for (a) and (b); both are small, testable changes. (b) may require an
  upstream `pycrate_rs` change.

### 3.2 Identity exposure via NAS Reject (GUTI invalidation)

- Mechanism: an unauthenticated network sends `Attach Reject`, `TAU Reject` or `Service Reject`
  with a cause that makes the UE delete its GUTI / mark the USIM invalid and then re-attach with
  its IMSI (or stop service). TS 24.301 5.5.1.2.5 / 5.5.3.2.5 / 5.6.1.5 and Annex A define the causes;
  rejects are accepted without integrity protection before a security context exists (TS 24.301
  4.4.4.2). Shaik et al. (NDSS 2016) showed persistent DoS with unprotected TAU Reject; Devilray
  found cause #9 ("UE identity cannot be derived by the network") the most reliable for single-cell
  IMSI catching; #10 ("implicitly detached") behaves similarly. Causes #3/#6/#7/#8 mark the USIM or
  EPS service invalid until power cycle / SIM reinsertion. #22 (congestion, with T3346 back-off) and
  #42 are DoS levers (Devilray: up to 30 min outage).
- Observable: DL NAS rejects with `emm_cause` (0xB0EC), followed within seconds by an uplink IMSI
  Attach (0xB0ED) or by silence (No NAS / no attach). Whether the reject arrived integrity
  protected may or may not be recoverable from DIAG: the plain-NAS log carries the decrypted
  message; whether the security header type is preserved is TBD (on-device).
- False positives: genuine #8 on unprovisioned SIMs (upstream already special-cases); #11/#13/#15 when
  roaming or on an MVNO with restricted roaming; #22 during real congestion (stadiums); #9/#10 after
  a genuine MME restart (rare, short, affects everyone).
- Upstream status: partially. `diagnostic.rs` lists the causes as Informational; `imsi_requested.rs`
  treats rejects only as a "Disconnect" transition after an Identity Request. A Reject-then-IMSI-Attach
  sequence *without* a preceding Identity Request is not flagged. This is the Devilray "IR" blind spot.
- Upstreamability: high. It closes a published blind spot that EFF is already evaluating; it fits
  the existing state machine pattern. Propose as a separate analyzer so versions and severities stay
  independent.

### 3.3 Authentication failure and authentication reject

- Mechanism: a CSS without access to the subscriber's HSS cannot produce a valid AUTN. If it sends an
  `Authentication Request` anyway (some do, to probe or to trigger fallbacks), the USIM rejects it and
  the UE replies `Authentication Failure` with cause #20 (MAC failure) or #21 (synch failure; also the
  signature of replayed vectors), or #26 (non-EPS authentication unacceptable). A network
  `Authentication Reject` before any successful authentication also invalidates the USIM.
- Observable: uplink `EMMAuthenticationFailure` with cause (0xB0ED); downlink
  `EMMAuthenticationReject` (0xB0EC).
- False positives: #21 occasionally occurs legitimately after SQN resynchronisation (followed
  immediately by a successful re-authentication); SIM swaps between devices. #20 on a home network
  is close to impossible absent an attack or a broken SIM.
- Upstream status: not implemented. `imsi_requested.rs` maps `EMMAuthenticationResponse` to
  AuthAccept but never looks at `AuthenticationFailure` or `AuthenticationReject`.
- Upstreamability: high. Causal, low-FP, trivial to test with synthetic NAS.

### 3.4 Downgrade and redirect (RRC release with redirection, 2G/3G pointers)

- Mechanism: `RRCConnectionRelease` (DL-DCCH) can carry `redirectedCarrierInfo` (GERAN, UTRA-FDD/TDD,
  CDMA2000 HRPD/1xRTT, or another EUTRA carrier) and `idleModeMobilityControlInfo` (dedicated
  per-RAT reselection priorities with timer T320). Both are sent before or without AS security on
  some flows and are trusted by the UE. Lin Huang (HITB 2016) demonstrated forcing a target LTE phone
  onto a fake 2G network this way; Devilray's C-FBS does LTE-to-2G and LTE-to-3G (the latter to
  bypass "disable 2G"). `MobilityFromEUTRACommand` (inter-RAT handover / CS fallback) is a second
  path.
- Observable: decoded RRC on 0xB0C0. Ordering matters: a redirect that follows an Identity Request
  without authentication, or a redirect from a cell that never ran Security Mode, is much stronger.
- False positives: CSFB voice in markets with live 2G/3G; operator load balancing to another EUTRA
  carrier (very common; must not alert); emergency-call handling. In the US, any redirect or dedicated
  priority toward UTRA or CDMA2000 is now anomalous because those networks are decommissioned; GERAN
  is anomalous except where 2G still runs.
- Upstream status: partial. `connection_redirect_downgrade.rs` flags `RedirectedCarrierInfo::Geran`
  as High and every other target as Informational; `idleModeMobilityControlInfo` and
  `MobilityFromEUTRACommand` are not inspected.
- Upstreamability: good for `idleModeMobilityControlInfo` (same message, same code path). A
  "sunset-RAT" severity bump for UTRA/CDMA2000 needs a region/PLMN policy (MCC-keyed table) that
  upstream may prefer as configuration.

### 3.5 SIB inter-RAT priority downgrade (SIB6/7/8)

- Mechanism: an LTE cell can advertise legacy frequencies with a higher `cellReselectionPriority`
  than any LTE frequency (SIB3 serving, SIB5 inter-frequency), steering idle UEs to the legacy RAT.
  SIB6 = UTRA, SIB7 = GERAN, SIB8 = CDMA2000 (TS 36.331).
- Observable: SI messages on BCCH-DL-SCH.
- False positives: networks that still prefer 3G for voice; partial SIB reception (comparison across a
  SIB1 boundary).
- Upstream status: implemented for SIB6 and SIB7 (`priority_2g_downgrade.rs`, credited to "heuristic
  T7 from Shinjo Park's 'Why We Cannot Win'"). SIB8 (CDMA2000 bandClassList priorities) is not
  checked. The comparison is reset at each SIB1, so a SIB6/7 received in a different SI window than
  SIB3/SIB5 can be missed (upstream TODO at line 51).
- Upstreamability: SIB8 coverage is a small, clear addition.

### 3.6 Null or weak ciphering and integrity (EEA0 / EIA0; A5/0, UEA0)

- Mechanism: TS 33.401 permits EIA0 only for unauthenticated emergency sessions; EEA0 is allowed by
  the standard but operators should not use it. A CSS without keys either skips AS/NAS security,
  selects EEA0/EIA0, or redirects to 2G where A5/0 or A5/1 is possible. Shinjo Park et al. (WiSec 2019,
  "LTE security disabled", not downloaded) found real commercial networks misconfigured with null
  algorithms, which is the main FP risk.
- Observable: RRC `SecurityModeCommand` `cipheringAlgorithm` and `integrityProtAlgorithm`; handover
  and SCG security configs in `RRCConnectionReconfiguration`; NAS `SecurityModeCommand`
  `nas_sec_algo` (ciphering and integrity nibbles). A related observable is the absence of any
  Security Mode procedure before user-plane DRB setup or before `RRCConnectionReconfiguration`
  carrying measurement config.
- False positives: emergency calls; test networks; documented carrier misconfiguration.
- Upstream status: EEA0 implemented in RRC (`null_cipher.rs`, including intra-LTE/inter-RAT
  handover, 5GC handover variants and EN-DC SCG ciphering) and NAS (`nas_null_cipher.rs`). EIA0 is
  not checked anywhere; "security never activated" is not checked. 2G A5/x and 3G UEA0 are out of
  reach (no GSM/UMTS decoding, see 1.1).
- Upstreamability: high for EIA0 (parallel to the existing EEA0 checks). "Security never activated"
  is a stateful sequence check; moderate.

### 3.7 Attach / TAU / Service reject causes as DoS indicators

Covered mechanically in 3.2; listed separately because the purpose differs. Causes #3, #6, #7, #8
(USIM or EPS invalid), #11, #12, #13, #15 (forbidden PLMN/TA lists), #22 with large T3346, and #42
deny service. Shaik et al. 2016 and LTEInspector (Hussain et al., NDSS 2018) document these as
persistent DoS; GLaDoS (Erni et al., USENIX Security 2025) shows protocol-level DoS now deployed
commercially for area denial. A UE-side analyzer can only report it (it cannot refuse it), but the
report helps users understand a sudden loss of service near a suspected CSS. Purification et al.
("Fake Base Station Detection and Blacklisting", UCCS/ETRI, ~2024) propose UE-side detection from
request count and connection-setup duration when an FBS abandons the procedure.

### 3.8 SIB content anomalies (beyond inter-RAT priority)

- Mechanism: a CSS wants UEs to select it and stay: high serving priority, low `q-RxLevMin`
  (accept weak signal), small hysteresis, absent or empty neighbor lists (SeaGlass and Dabrowski:
  "empty neighbor lists", "high cell reselect offset"), short or missing SIB chains (Rayhunter
  "Incomplete SIB"; Devilray C-FBS broadcasts SIB1-5 only), PLMN lists that do not match the country,
  bandwidth constrained by hardware (Devilray: fixed 5 MHz).
- Observable: MIB `dl-Bandwidth`; SIB1 `plmn-IdentityList`, `trackingAreaCode`, `cellIdentity`,
  `q-RxLevMin`, `schedulingInfoList`, `cellBarred`; SIB3 `cellReselectionServingFreqInfo`
  (priority, `threshServingLow`, `s-NonIntraSearch`, `q-Hyst`); SIB4 intra-frequency neighbor and
  black lists; SIB5 inter-frequency carriers.
- False positives: very high. Real networks vary widely; small cells, DAS, private LTE (band 48
  CBRS is in the RC400L band list), and cells-on-wheels look "unusual" by construction. 5 MHz LTE
  carriers exist in US low-band deployments.
- Upstream status: only SIB1 scheduling count (`incomplete_sib.rs`, Informational) and SIB6/7
  priorities.
- Upstreamability: individually low (FP-prone). Valuable as *features* combined with a causal
  trigger: e.g. raise an Identity Request alert one level when the same cell also has an incomplete
  SIB chain and no SIB4/SIB5 neighbors.

### 3.9 Paging anomalies

- Mechanism: (a) paging with IMSI (`PagingUE-Identity` `imsi`) forces an IMSI-based attach and is
  legitimately used only for network-side recovery (TS 24.301 5.6.2.2.2); (b) repeated paging of the
  UE's S-TMSI to elicit uplink for presence testing or ranging (Shaik et al. 2016 "smart paging";
  ToRPEDO, Hussain et al. NDSS 2019, recovers paging occasions and even IMSI via brute force);
  Devilray's C-FBS uses paging plus Identity Request for location tracking; (c) PCCH `etws-Indication`
  / `cmas-Indication` paired with fake SIB10-12 emergency alerts.
- Observable: PCCH on 0xB0C0 (already decoded as `LteInformationElement::PCCH`). Matching the UE's own
  identity requires the current S-TMSI (from the last GUTI Reallocation / Attach Accept, both NAS
  downlink) and, for IMSI paging, the IMSI (obtainable from the SIM via AT, as upstream already does
  for EF_HPLMNwAcT). Hong & Yoon (NDSS 2026) show GUTIs persisting up to 33 days on some carriers,
  which makes S-TMSI paging a practical tracking handle.
- False positives: IMSI paging after a genuine MME failure (rare, affects many UEs at once);
  S-TMSI paging is normal for every incoming call, SMS or push notification, so (b) is Low at most.
- Upstream status: not implemented (PCCH is decoded but no analyzer reads it). Devilray describes
  Rayhunter as monitoring IMSI paging; we found no such code in v0.13.0.
- Upstreamability: IMSI paging addressed to our IMSI: high (causal, rare). Any IMSI paging on the
  serving cell (not ours): Low/Informational. Handle the IMSI as a salted hash in memory and never
  write it into reports.

### 3.10 TAC / cell-identity churn and impermanence

- Mechanism: CSS appear briefly, at places where no cell existed, with random or cloned identities,
  often a TAC different from all neighbors to force a TAU (SeaGlass "geographic inconsistency",
  "impermanence", "multi-location transmissions"; Crocodile Hunter "cells moving / new cells / cells
  changing parameters"; Devilray randomized PCI/TAC/cell ID or clones).
- Observable: SIB1 ECGI (`cellIdentity` 28-bit = eNB ID + cell), TAC, PLMN; PCI and EARFCN from ML1
  (0xB17F) or from RRC measurement config; UE `TrackingAreaUpdateRequest` (0xB0ED); GPS fixes when
  the daemon GPS mode is on (`daemon/src/gps.rs`).
- False positives: driving; small cells; carrier re-homing; CoWs at events (Marlin's caution);
  multiple PLMNs on one eNB.
- Upstream status: not implemented. The analyzer trait (`analyzer.rs:118-121`) warns analyzers to keep
  memory small; a persistent cell database belongs in the daemon, not in `lib` analyzers.
- Upstreamability: moderate. A bounded, recording-scoped version ("TAC change with no position
  change within N minutes", "same ECGI seen with two TACs or two PCIs") fits upstream. A persistent
  per-device cell history (and comparison against OpenCelliD / FCC ASR / WiGLE) is likely fork-only.

### 3.11 Signal-strength outliers

- Mechanism: a CSS must out-power the legitimate network locally; it shows up as a cell much stronger
  than its neighbors, or a known ECGI stronger than its historical level at that location (SeaGlass
  models power and height per base station; Crocodile Hunter "cells that change signal strength").
  Devilray's C-FBS adjusts power for 2-10 m range, so very high RSRP from an unknown cell is
  plausible; AdaptOver (Erni et al., MobiCom 2022) and SigOver (Yang et al., USENIX Security 2019)
  show overshadowing attacks that need only a few dB over the legitimate signal and do not create a
  new cell at all, defeating cell-based detection.
- Observable: 0xB17F serving RSRP/RSRQ/RSSI and 0xB180 neighbor RSRP per PCI (parsed in `ml1.rs`,
  not exposed to analyzers).
- False positives: standing next to a small cell or window; indoor/outdoor transitions; antenna
  orientation; the RC400L in a bag.
- Upstream status: not implemented; data parsed but not plumbed.
- Upstreamability: plumbing (new `InformationElement` variant) should be proposed upstream as an
  issue first; it changes a public enum. The outlier analyzer itself should start as corroborative
  (raises confidence of a causal alert on the same cell) rather than alerting alone.

### 3.12 Timing advance

- Mechanism: TA in the Random Access Response encodes round-trip distance (16 Ts units, about 78 m per
  step). A CSS near the UE yields TA near 0 while claiming the identity of a macro cell kilometres
  away; Dabrowski et al. (RAID 2016) used device round-trip timing from the operator side.
- Observable: 0xB062 MAC RACH response (converted to GSMTAP MAC), LL1 serving cell timing
  (`ll1.rs`).
- False positives: legitimate small cells and indoor systems also give TA near 0; TA alone says
  nothing about legitimacy without a reference location for the cell.
- Upstream status: not implemented; partially parsed.
- Upstreamability: low as a standalone alert; useful as a feature alongside 3.10/3.11.

### 3.13 Statistical rate of identity-exposing connections (Marlin-style)

- Mechanism: count, per time window, the fraction of RRC connections that include any of the
  IMSI-exposing messages, against a baseline (Marlin: median under 3% of LTE connections).
- Observable: the existing decoded RRC/NAS stream.
- False positives: a single UE has few connections, so per-device ratios are noisy; Marlin's
  statistics come from sniffing all UEs in a cell with an SDR. For Rayhunter this is best used as a
  recording-level summary ("N identity-exposing events in M connections"), not a live alert.
- Upstream status: not implemented (the Diagnostic analyzer lists the same messages one by one).
- Upstreamability: moderate, as a report-summary feature.

### 3.14 Other observables worth tracking

- `UECapabilityEnquiry` before AS security: exposes device capabilities and enables bidding-down
  (Shaik et al., WiSec 2019, not downloaded). Many real networks send it early; Low/Informational.
- `RRCConnectionReject` with long `waitTime` or `deprioritisationReq`: DoS lever; Informational.
- `DLInformationTransfer` carrying NAS before Security Mode beyond the permitted set: PHOENIX
  (Echeverria et al., NDSS 2021) signatures detect several n-day control-plane attacks with
  lightweight runtime monitors on exactly this kind of ordering property.
- EMM Information with a spoofed network name / time zone: cosmetic, Informational.
- 2G-specific (out of reach on this hardware but documented for completeness): A5/0 or missing Cipher
  Mode Command, IMSI paging, MM Identity Request for IMEI, empty neighbor list, low T3212, LAC
  inconsistency, silent SMS (SnoopSnitch, AIMSICD, Dabrowski 2014, SeaGlass, FBS-Radar's five
  metrics in Li et al. NDSS 2017 for SMS-spam FBSes).

---

## 4. Approaches outside the single-UE model (context only)

| Approach | Example | Why it matters to us |
| --- | --- | --- |
| SDR wide-area scanning of all cells | Crocodile Hunter (EFF, 2020; srsLTE-based MIB/SIB1 decoding, trilateration, anomaly list) | Sees cells our UE never camps on; Rayhunter only sees its serving cell and neighbors it measures |
| Crowdsourced vehicular sensors | SeaGlass (Ney et al., PoPETs 2017; 15 rideshare cars, two cities) | Source of most correlated-signature definitions; needs a city-scale baseline |
| Crowdsourced phone telemetry | FBS-Radar (Li et al., NDSS 2017; ~100M users) | Shows correlated heuristics work at scale for 2G SMS-spam FBS, less so for targeted LE CSS |
| Network-side detection | Dabrowski et al. RAID 2016 (operator plausibility checks, RTT, cipher offline checks); Murat (Nakarmi et al., Ericsson 2021; measurement-report-based, adopted into 3GPP) | Carriers can see what a UE cannot; motivates responsible-disclosure ties to carriers |
| ML over L3 traces | FBSDetector (Mubasshir, Karim, Bertino; USENIX Security 2025; 96% accuracy, 2.96% FPR, 21 multi-step attacks) | Labeled datasets; but ML models are hard to explain to Rayhunter users and to keep low-FP across carriers |
| Runtime verification | PHOENIX (NDSS 2021) | Signature language for ordered message properties; maps well onto Rayhunter's state-machine analyzers |
| Adversarial baselines | Devilray (2026) | Evaluate our analyzers against its variants if access is granted (released to vetted researchers) |

---

## 5. Candidate new analyzers for v1.2 (ranked)

Ranking weighs (1) causal strength (Marlin's criterion), (2) expected false-positive rate on US
carriers, (3) whether the data already reaches analyzers (no plumbing), (4) evidence from a commercial
CSS or a published blind spot, and (5) upstreamability.

| Rank | Candidate analyzer | Key observable | Data available today? | Suggested severity | FP risk | Upstreamable | Rationale |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | Unauthenticated NAS Reject with identity-forcing or service-denying cause | Attach/TAU/Service Reject cause #3/#6/#7/#9/#10/#11/#12/#13/#15/#22/#42 before any successful authentication in the connection; correlate with following IMSI Attach | yes (NAS) | High for #9/#10 followed by IMSI attach on home PLMN; Medium for #3/#6/#7; Low for #11-#15/#22/#42 | low-medium | yes | Closes the Devilray "IR" blind spot in Rayhunter; matches observed C-FBS disconnect behavior |
| 2 | Authentication failure / reject | UE `AuthenticationFailure` #20/#21/#26; network `AuthenticationReject` | yes (NAS) | High (#20), Medium (#21 without immediate successful re-auth), High (reject before success) | low | yes | Causal proof the network lacks valid keys; not inspected anywhere upstream |
| 3 | Own permanent-identity disclosure | UE `IdentityResponse` with IMSI/IMEI; UE `AttachRequest` with IMSI-type EPS mobile identity; correlate with lack of subsequent auth | yes, but `epsid` parsing gap (`diagnostic.rs:65-70`) | High when no authentication follows within T seconds on home PLMN; else Low | low | yes (may need pycrate_rs fix) | Marlin's causal approach applied to the uplink: catches every path to exposure, including ones we have not enumerated |
| 4 | Null integrity (EIA0) and "security never activated" | RRC SMC `integrityProtAlgorithm` = eia0; NAS SMC integrity = EIA0; DRB setup or measurement reconfiguration with no prior SMC | yes (RRC/NAS) | High (EIA0 outside emergency); Medium (no SMC) | low (EIA0), medium (no SMC) | yes | Natural extension of `null_cipher.rs` / `nas_null_cipher.rs`; EIA0 is standard-forbidden except emergency |
| 5 | IMSI paging | PCCH `PagingRecord` with `ue-Identity` = imsi; compare salted hash with SIM IMSI | yes (PCCH), IMSI via AT | High if ours; Low if another IMSI | low | yes | Rare in legitimate networks; listed by Marlin; claimed by Devilray to be part of Rayhunter's model but absent in v0.13.0 |
| 6 | Dedicated-priority and sunset-RAT downgrade | `RRCConnectionRelease.idleModeMobilityControlInfo` favoring GERAN/UTRA/CDMA2000; `redirectedCarrierInfo` to UTRA/CDMA2000 in markets where those are decommissioned; `MobilityFromEUTRACommand` to legacy RAT; SIB8 CDMA2000 priority | yes (RRC) | High in sunset markets; Informational elsewhere | low in US | yes (needs MCC policy table) | Covers Devilray LTE-to-3G variant and paths `connection_redirect_downgrade.rs` and `priority_2g_downgrade.rs` skip |
| 7 | Cell identity consistency (recording-scoped) | SIB1 ECGI/TAC/PLMN; same ECGI with different TAC or PCI; TAC change with stationary GPS; MCC not matching SIM home country with no roaming; cells observed for under N minutes | partial (SIB1 yes; PCI needs ML1 plumbing; GPS in daemon) | Low alone; raises severity of 1-6 on the same cell | medium-high | partially | SeaGlass/Crocodile Hunter impermanence and Devilray random/cloned identities; corroboration, not alarm |
| 8 | Measurement plumbing plus RSRP/TA outlier feature | ML1 0xB17F/0xB180 RSRP per PCI, MAC RACH TA; flag new cell strongest by large margin with TA near 0 | no (parsed, not plumbed) | Informational / corroborative | high alone | plumbing: propose upstream as an issue first | Enables 7 and all physical-layer work; C-FBS power behavior and SeaGlass power modeling |
| 9 | SIB reselection-parameter anomalies | SIB3 priority 7 with empty SIB5, low `q-RxLevMin`, small `q-Hyst`, absent SIB4/SIB5 | yes | Informational / corroborative | high | maybe | SeaGlass/Dabrowski "keep" signatures; feature for a combined score |
| 10 | Recording-level identity-exposure summary | count of identity-exposing events per connection over the recording | yes | report only | n/a | yes | Marlin baseline (median under 3%) gives users context for a single alert |
| 11 | Emergency-alert spoofing context | PCCH etws/cmas indication plus SIB10-12 from a cell already flagged | yes | Low | low (conditional) | maybe | Fake WEA/ETWS is a documented FBS capability; cheap when conditioned on other alerts |
| 12 | RRC Connection Reject DoS context | `waitTime` at maximum, `deprioritisationReq` | yes | Informational | medium | maybe | Explains service loss near a CSS (GLaDoS, Purification et al.) |

Cross-cutting implementation notes for v1.2:

- Write each candidate as a separate `Analyzer` with its own `get_version()`, an `AnalyzerConfig`
  flag, and a `doc/heuristics.md`-style entry, matching upstream conventions (`analyzer.rs:21-51`).
- Use packet timestamps, not message counts, for timeouts (as `no_nas_messages.rs` already does), so
  reanalysis is reproducible.
- Add golden-vector tests per analyzer from synthetic messages; collect real negative baselines on
  Verizon/AT&T/T-Mobile before choosing severities.
- Never place IMSI, IMEI, or GUTI values in `Event.message`; reports are shared (see
  `legal-and-ethics.md` section 5).
- Ask EFF whether Devilray profiles can be run against candidate analyzers 1-6 before release.

---

## Sources

Upstream code and docs (local, EFForg/rayhunter v0.13.0, GPL-3.0):

- `upstream/doc/heuristics.md`, `upstream/doc/faq.md`, `upstream/doc/orbic.md`
- `upstream/lib/src/analysis/{analyzer,imsi_requested,connection_redirect_downgrade,priority_2g_downgrade,null_cipher,nas_null_cipher,incomplete_sib,diagnostic,no_nas_messages,information_element}.rs`
- `upstream/lib/src/diag_device.rs`, `upstream/lib/src/log_codes.rs`, `upstream/lib/src/diag/diaglog/{ml1,ll1}.rs`, `upstream/lib/src/gsmtap/parser.rs`, `upstream/lib/src/sim/generic_at.rs`, `upstream/daemon/src/gps.rs`

Papers and talks (local copies listed in `ref-docs/pdfs/README.md`):

- Oh, Kim, Bae, et al. "Devilray: A Reconfigurable Adversarial Baseline for Revealing Blind Spots in Fake Base Station Detection." arXiv:2605.19232v3, 2026. <https://arxiv.org/abs/2605.19232>
- Tucker, Bennett, Kotuliak, Erni, Capkun, Butler, Traynor. "Detecting IMSI-Catchers by Characterizing Identity Exposing Messages in Cellular Traffic" (Marlin). NDSS 2025. <https://cise.ufl.edu/~butler/pubs/ndss25-tucker-marlin.pdf>
- Ney, Smith, Cadamuro, Kohno. "SeaGlass: Enabling City-Wide IMSI-Catcher Detection." PoPETs 2017(3). <https://www.petsymposium.org/popets/2017/popets-2017-0027.php>
- Dabrowski, Pianta, Klepp, Mulazzani, Weippl. "IMSI-Catch Me If You Can: IMSI-Catcher-Catchers." ACSAC 2014. <https://eprints.cs.univie.ac.at/6489/> (PDF host unreachable at retrieval; not downloaded)
- Dabrowski, Petzl, Weippl. "The Messenger Shoots Back: Network Operator Based IMSI Catcher Detection." RAID 2016. <https://www.ics.uci.edu/~dabrowsa/dabrowski-raid16-providerICdetection101.pdf>
- Shaik, Borgaonkar, Asokan, Niemi, Seifert. "Practical Attacks Against Privacy and Availability in 4G/LTE Mobile Communication Systems." NDSS 2016. <https://arxiv.org/abs/1510.07563>
- Borgaonkar, Shaik, Asokan, Niemi, Seifert. "LTE and IMSI Catcher Myths." Black Hat EU 2015 white paper. <https://www.blackhat.com/docs/eu-15/materials/eu-15-Borgaonkar-LTE-And-IMSI-Catcher-Myths-wp.pdf>
- Hussain, Chowdhury, Mehnaz, Bertino. "LTEInspector: A Systematic Approach for Adversarial Testing of 4G LTE." NDSS 2018. <https://www.ndss-symposium.org/wp-content/uploads/2018/02/ndss2018_02A-3_Hussain_paper.pdf>
- Hussain, Echeverria, Chowdhury, Li, Bertino. "Privacy Attacks to the 4G and 5G Cellular Paging Protocols Using Side Channel Information" (ToRPEDO). NDSS 2019. <https://par.nsf.gov/servlets/purl/10094854>
- Park, Shaik, Borgaonkar, Martin, Seifert. "White-Stingray: Evaluating IMSI Catchers Detection Applications." USENIX WOOT 2017. <https://www.usenix.org/conference/woot17/workshop-program/presentation/park>
- Quintin. "Detecting Fake 4G Base Stations in Real Time" (Crocodile Hunter). Black Hat USA / DEF CON 28, 2020. <https://i.blackhat.com/USA-20/Wednesday/us-20-Quintin-Detecting-Fake-4G-Base-Stations-In-Real-Time.pdf> ; code <https://github.com/efforg/crocodilehunter>
- Li, Wang, Wilson, et al. "FBS-Radar: Uncovering Fake Base Stations at Scale in the Wild." NDSS 2017. <https://ccs.neu.edu/home/cbw/static/pdf/li-ndss17.pdf>
- Echeverria, Ahmed, Wang, Arif, Hussain, Chowdhury. "PHOENIX: Device-Centric Cellular Network Protocol Monitoring using Runtime Verification." NDSS 2021. <https://www.ndss-symposium.org/wp-content/uploads/ndss2021_4A-3_24390_paper.pdf>
- Nakarmi, Ersoy, Soykan, Norrman. "Murat: Multi-RAT False Base Station Detector." arXiv:2102.08780, 2021. <https://arxiv.org/abs/2102.08780>
- Mubasshir, Karim, Bertino. "Gotta Detect 'Em All: Fake Base Station and Multi-Step Attack Detection in Cellular Networks." USENIX Security 2025; arXiv:2401.04958. <https://arxiv.org/abs/2401.04958>
- Erni, Kotuliak, Leu, Roeschlin, Capkun. "AdaptOver: Adaptive Overshadowing Attacks in Cellular Networks." ACM MobiCom 2022; arXiv:2106.05039. <https://arxiv.org/abs/2106.05039>
- Erni, Kotuliak, Baker, Martinovic, Capkun. "GLaDoS: Location-aware Denial-of-Service of Cellular Networks." USENIX Security 2025. <https://www.usenix.org/conference/usenixsecurity25/presentation/erni>
- Hong, Yoon. "Passive Multi-Target GUTI Identification via Visual-RF Correlation in LTE Networks." NDSS 2026. <https://www.ndss-symposium.org/wp-content/uploads/2026-f2487-paper.pdf>
- Purification, Wuthier, Kim, Kim, Chang. "Fake Base Station Detection and Blacklisting." (NSF PAR copy, ~2024). <https://par.nsf.gov/servlets/purl/10509865>
- Mjolsnes, Olimid. "Easy 4G/LTE IMSI Catchers for Non-Programmers." arXiv:1702.04434, 2017. <https://arxiv.org/abs/1702.04434>
- Yang, Bae, Son, Kim, Kim, Kim. "Hiding in Plain Signal: Physical Signal Overshadowing Attack on LTE." USENIX Security 2019. <https://www.usenix.org/conference/usenixsecurity19/presentation/yang-hojoon> (not downloaded)
- Lin Huang. "Forcing a Targeted LTE Cellphone into an Eavesdropping Network." HITB SecConf 2016 Amsterdam. <https://archive.conference.hitb.org/hitbsecconf2016ams/sessions/forcing-a-targeted-lte-cellphone-into-an-eavesdropping-network> (cited by upstream `connection_redirect_downgrade.rs`; slides not downloaded)

EFF posts:

- Quintin. "Meet Rayhunter: A New Open Source Tool from EFF to Detect Cellular Spying." 2025-03. <https://www.eff.org/deeplinks/2025/03/meet-rayhunter-new-open-source-tool-eff-detect-cellular-spying>
- Quintin. "Rayhunter: What We Have Found So Far." 2025-09-10. <https://www.eff.org/deeplinks/2025/09/rayhunter-what-we-have-found-so-far>
- EFF. "Rayhunter" one-pager, revised 2026-04-20. <https://www.eff.org/files/2026/04/23/rayhunter_-_usa.pdf>

Standards (cited by section; not downloaded):

- 3GPP TS 24.301 (EPS NAS: identification, authentication, reject handling, EMM causes Annex A)
- 3GPP TS 36.331 (LTE RRC: RRCConnectionRelease, SIB1-SIB8, paging)
- 3GPP TS 33.401 (EPS security architecture: EEA0/EIA0 usage)
- 3GPP TS 36.304 (LTE idle-mode cell selection and reselection)
