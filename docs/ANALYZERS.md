# Analyzers

Specification of the detection heuristics in our build: the nine upstream analyzers we ship
unchanged, and the new analyzers planned for v1.2 (T-OR-051..T-OR-064, [ROADMAP.md](../ROADMAP.md)).
The research behind every entry is
[ref-docs/lte-imsi-catcher-detection.md](../ref-docs/lte-imsi-catcher-detection.md); upstream
internals are in [ref-docs/rayhunter-upstream-overview.md](../ref-docs/rayhunter-upstream-overview.md)
section 5.

## How analyzers work upstream

- Each analyzer implements the `Analyzer` trait in `lib/src/analysis/analyzer.rs`
  (`get_name`, `get_description`, `get_version`, `analyze_information_element`) and is toggled by
  an `[analyzers]` key ([CONFIGURATION.md](CONFIGURATION.md)).
- Input is an `InformationElement` decoded from DIAG: LTE RRC from log code 0xB0C0 and plain LTE
  NAS (EMM/ESM) from 0xB0EC/0xB0ED/0xB0E2/0xB0E3. GSM, UMTS and 5G variants exist but nothing
  decodes into them
  ([ref-docs/lte-imsi-catcher-detection.md](../ref-docs/lte-imsi-catcher-detection.md) section 1.1).
- Logged but not reaching analyzers: ML1 serving and neighbor measurements (0xB17F, 0xB180), MAC
  RACH response with timing advance (0xB062), LL1 timing, 2G/3G signalling, and 0x11EB user-plane
  data (section 1.2 of the same file). Signal, PCI and timing-advance heuristics therefore need
  plumbing first (T-OR-058).
- Severity (`EventType`): `Informational`, `Low`, `Medium`, `High`. Anything above Informational
  turns the display red and triggers a `Warning` notification (5 minute debounce).
- Results are stored per recording as ndjson (`REPORT_VERSION = 2`) with each analyzer's version,
  and can be recomputed with `POST /api/analysis/{name}` or `rayhunter-check`.

## Upstream analyzers shipped (v0.13.0)

| Config key | File | Version | Default | Ours | Detects |
|---|---|---|---|---|---|
| `imsi_requested` | `imsi_requested.rs` | 5 | on | on | NAS Identity Request for IMSI/IMEI in a suspicious sequence (new cell, identity request, no authentication, release); home PLMN from the SIM since v0.13.0 |
| `connection_redirect_2g_downgrade` | `connection_redirect_downgrade.rs` | 1 | on | on | `RRCConnectionRelease` redirecting to GERAN (High); other targets Informational |
| `lte_sib6_and_7_downgrade` | `priority_2g_downgrade.rs` | 2 | on | on | SIB6/SIB7 giving 3G/2G higher reselection priority than LTE |
| `null_cipher` | `null_cipher.rs` | 1 | on | on | RRC security mode or reconfiguration selecting EEA0 |
| `nas_null_cipher` | `nas_null_cipher.rs` | 1 | on | on | NAS security mode command selecting EEA0 |
| `incomplete_sib` | `incomplete_sib.rs` | 2 | on | on | SIB1 scheduling without the full SIB chain (Informational) |
| `diagnostic_analyzer` | `diagnostic.rs` | 1 | on | on | Informational list of identity-exposing messages (Marlin-derived) |
| `no_nas_messages` | `no_nas_messages.rs` | 1 | off | off | five minutes of DIAG traffic with no NAS message (missing or inactive SIM) |
| `test_analyzer` | `test_analyzer.rs` | 1 | off | off | every SIB1; smoke test only |

Known upstream gaps and false-positive notes:

- `imsi_requested` does not use the requested identity type, does not check the UE's uplink
  Identity Response or IMSI-type Attach Request (`epsid` is not parsed), and times out by message
  count rather than time (section 3.1 of the research file). Spurious warnings in weak signal are
  reported on the Orbic (#543).
- `connection_redirect_2g_downgrade` ignores `idleModeMobilityControlInfo` and
  `MobilityFromEUTRACommand` (section 3.4).
- `lte_sib6_and_7_downgrade` does not check SIB8 (CDMA2000) and resets at each SIB1 (section 3.5).
- No analyzer checks EIA0 or "security never activated" (section 3.6).
- Devilray (arXiv 2605.19232) shows Rayhunter misses IMSI catching through NAS Reject without an
  Identity Request (section 2).
- `doc/heuristics.md` misdescribes SIB6 as CDMA2000 and labels IMSI Requested "v3" while the code
  returns 5; fixed by T-OR-027.

## Severity policy for new analyzers

High is reserved for causal events that a legitimate network should not produce (EEA0/EIA0
outside emergency, identity exposure without authentication, unauthenticated rejects that only
make sense as attacks). A single correlated anomaly (cell parameters, signal, timing) is
Informational or Low and may only raise the severity of a causal alert on the same cell
([ref-docs/lte-imsi-catcher-detection.md](../ref-docs/lte-imsi-catcher-detection.md) section 2;
[ref-docs/legal-and-ethics.md](../ref-docs/legal-and-ethics.md) section 4 on false-alarm harm).
New analyzers ship off by default until negative baselines on US carriers (T-OR-061) support the
chosen severities.

Implementation rules (section 5 of the research file):

- One `Analyzer` per heuristic, with its own `get_version()`, `AnalyzerConfig` flag and a
  `doc/heuristics.md` entry.
- Timeouts use packet timestamps, not message counts, so re-analysis is reproducible.
- Never place IMSI, IMEI, GUTI or S-TMSI values in `Event.message`; compare identities through a
  salted hash held in memory.
- Golden vectors per analyzer ([TESTING.md](TESTING.md)).

## Planned analyzers (v1.2)

Ranked as in [ref-docs/lte-imsi-catcher-detection.md](../ref-docs/lte-imsi-catcher-detection.md)
section 5 by causal strength, false-positive risk on US carriers, data availability, evidence from a
commercial cell-site simulator, and upstreamability.

| Rank | Ticket | Analyzer (proposed key) | Key observable | Data today? | Proposed severity | FP risk | Flag |
|---|---|---|---|---|---|---|---|
| 1 | T-OR-052 | Unauthenticated NAS Reject (`nas_reject_identity`) | Attach/TAU/Service Reject with cause #3/#6/#7/#9/#10/#11/#12/#13/#15/#22/#42 before any successful authentication, then IMSI attach | yes (NAS) | High for #9/#10 followed by IMSI attach on home PLMN; Medium #3/#6/#7; Low #11-#15/#22/#42 | low-medium | [UPSTREAM] |
| 2 | T-OR-053 | Authentication failure / reject (`auth_failure`) | UE `AuthenticationFailure` #20/#21/#26; network `AuthenticationReject` | yes (NAS) | High #20; Medium #21 without immediate re-auth; High reject before success | low | [UPSTREAM] |
| 3 | T-OR-054 | Own identity disclosure (`identity_disclosure`) | UE `IdentityResponse` with IMSI/IMEI; `AttachRequest` with IMSI-type identity; no authentication within T seconds | yes, `epsid` parsing gap | High on home PLMN without following auth; else Low | low | [UPSTREAM] (may need a `pycrate_rs` change) |
| 4 | T-OR-055 | Null integrity and missing security (`null_integrity`) | RRC/NAS SMC with EIA0; DRB setup or measurement reconfiguration with no prior SMC | yes (RRC/NAS) | High EIA0 outside emergency; Medium no SMC | low / medium | [UPSTREAM] |
| 5 | T-OR-056 | IMSI paging (`imsi_paging`) | PCCH `PagingRecord` with `ue-Identity` = imsi, compared with the SIM IMSI by salted hash | yes (PCCH); IMSI via AT | High if ours; Low if another IMSI | low | [UPSTREAM] |
| 6 | T-OR-057 | Dedicated-priority and sunset-RAT downgrade (`legacy_rat_downgrade`) | `idleModeMobilityControlInfo` favoring legacy RATs; redirect to UTRA/CDMA2000 where decommissioned; `MobilityFromEUTRACommand`; SIB8 priorities | yes (RRC) | High in sunset markets (MCC policy table); Informational elsewhere | low in US | [UPSTREAM] |
| 7 | T-OR-059 | Cell identity consistency (`cell_consistency`) | same ECGI with different TAC or PCI; TAC change while GPS is stationary; MCC mismatch; short-lived cells, all within one recording | partial (PCI needs T-OR-058) | Low alone; raises 1-6 on the same cell | medium-high | [UPSTREAM] (partially) |
| 8 | T-OR-058 | Measurement plumbing plus RSRP/TA outlier feature | 0xB17F/0xB180 RSRP per PCI; RACH TA near 0 on a new, strongest cell | no (parsed, not plumbed) | Informational / corroborative | high alone | [UPSTREAM] (open an issue first: changes a public enum) |
| 9 | T-OR-062 | SIB reselection-parameter anomalies (stretch) | SIB3 priority 7 with empty SIB5, low `q-RxLevMin`, small `q-Hyst`, missing SIB4/SIB5 | yes | Informational / corroborative | high | [UPSTREAM] (maybe) |
| 10 | T-OR-060 | Recording-level identity-exposure summary | identity-exposing events per connection over a recording (Marlin baseline: median under 3 percent of LTE connections) | yes | report only | n/a | [UPSTREAM] |
| 11 | T-OR-063 | Emergency-alert spoofing context (stretch) | PCCH etws/cmas indication plus SIB10-12 from an already flagged cell | yes | Low | low (conditional) | [UPSTREAM] (maybe) |
| 12 | T-OR-064 | RRC Connection Reject DoS context (stretch) | `waitTime` at maximum, `deprioritisationReq` | yes | Informational | medium | [UPSTREAM] (maybe) |

The proposed config keys are placeholders until the upstream PR discussion settles names. The
test harness all of these depend on is T-OR-051; severity calibration is T-OR-061. EFF has been
given Devilray; asking whether its attack profiles can be run against candidates 1-6 before
release is part of T-OR-061.

## Out of scope on this hardware

2G A5/x and 3G UEA0 checks, MM-level 2G identity requests and other 2G/3G message analysis: the
analyzer input has no GSM/UMTS decoding, US 2G/3G networks are shut down, and the RC400L's actual
legacy-RAT support is TBD (on-device) from its own `UECapabilityInformation`
([ref-docs/lte-imsi-catcher-detection.md](../ref-docs/lte-imsi-catcher-detection.md) sections 1.3,
3.14). Overshadowing attacks that do not create a new cell (SigOver, AdaptOver) are not detectable
by cell-based heuristics (section 3.11).
