# Legal and Ethical Considerations for Orbic-Rayhunter

- Retrieved: 2026-11-22
- Upstream ref: EFForg/rayhunter v0.13.0 (cd462a1), local read-only checkout at `./upstream`
- Scope: the legal and ethical envelope for installing and extending Rayhunter on an Orbic RC400L
  that the user owns, focused on United States law, plus handling of captured data and disclosure of
  findings.

> **This is not legal advice.** It is a research summary written by and for software developers to
> identify questions. Statutes are summarized, not quoted in full, and their application to a given
> situation depends on facts and jurisdiction. Laws change; state law varies a lot. Anyone deploying
> this project, especially outside the US, at protests, or in a professional or journalistic capacity,
> should consult a licensed attorney. EFF gives the same advice for running Rayhunter outside the US
> ("consult with an attorney in your country").

---

## 1. Project scope commitment: passive receive only

Orbic-Rayhunter observes the control-plane signalling between the user's own hotspot and the
networks it talks to, as reported by the hotspot's own Qualcomm DIAG interface. The project's hard
scope rules:

1. **No transmission beyond normal UE behavior.** The modem transmits only what a stock, FCC-certified
   UE transmits when attaching to a network. The project never configures the modem to act as a base
   station, jammer, repeater, or test transmitter, never changes RF parameters (bands, power, NV
   items that affect emissions), and never injects or replays over-the-air messages.
2. **No active probing of suspected cell-site simulators.** No attempt to locate, trigger, overload,
   or interfere with a suspected CSS or a legitimate cell. (Crocodile Hunter's authors make the same
   choice: their talk's answer to "why don't we transmit?" is "EFF lawyers".)
3. **Own device, own SIM.** Analysis targets signalling of the device the user owns, with a SIM the
   user is entitled to use. Broadcast system information (MIB/SIBs) and paging channels are received
   the same way every phone receives them.
4. **No building IMSI catchers for testing in the wild.** Upstream's FAQ: people have built IMSI
   catchers at home to test Rayhunter, "but we don't recommend that, since it violates FCC regulations
   and will probably upset your neighbors". Test against recorded captures, synthetic messages, or
   shielded-enclosure labs under an appropriate license (academic work such as Devilray, Marlin and
   LTEInspector ran active experiments only in shielded rooms or licensed experimental spectrum).

Everything below assumes these rules hold. Several legal conclusions change if they do not.

---

## 2. Radio law (FCC)

| Provision | Summary | Relevance |
| --- | --- | --- |
| 47 U.S.C. 301 | No person may transmit radio energy without a license or authorization | The hotspot transmits under the carrier's licenses and its own equipment authorization; operating a self-built eNodeB/BTS on cellular spectrum without a license violates this |
| 47 U.S.C. 333 | No willful or malicious interference with licensed or authorized radio communications | Jamming, overshadowing, or a fake base station that pulls UEs off the network would implicate this; passive logging does not |
| 47 U.S.C. 302a and FCC equipment authorization rules (47 CFR Part 2) | Devices that emit RF must be authorized; modifications that change RF characteristics can void the authorization | Rayhunter changes software on the application processor and reads DIAG logs; it should not alter modem RF configuration. Keep it that way |
| FCC enforcement on signal jammers | The FCC treats marketing, sale, or use of jammers as unlawful for the public | Relevant if anyone proposes "defensive" jamming of a suspected CSS: out of scope |
| 47 U.S.C. 605(a) | Restricts divulging or using the existence, contents, or meaning of intercepted radio communications not addressed to the receiver, with exceptions | Own-device signalling is addressed to the user's device; broadcast SIBs and paging are intended for reception by all UEs. Publishing other subscribers' identities or traffic would be a different matter |

Takeaway: a receive-only analyzer of the user's own UE signalling sits well inside FCC rules. The
risk surface opens only if the project ever transmits differently from a stock UE.

---

## 3. Interception and surveillance law

### 3.1 Federal Wiretap Act (18 U.S.C. 2510-2523, Title I of ECPA)

- 18 U.S.C. 2511(1) prohibits intentionally intercepting wire, oral, or electronic communications, and
  disclosing or using their contents when one knows they were illegally intercepted.
- 18 U.S.C. 2511(2)(d) exempts interception by a party to the communication, or where one party has
  consented (unless done for a criminal or tortious purpose). The federal standard is one-party
  consent.
- 18 U.S.C. 2510(8) defines "contents" as the substance, purport, or meaning of a communication;
  signalling metadata is generally treated under the Pen Register statute instead (below).

Application to Rayhunter on the user's own hotspot:

- Signalling between the user's own UE and the network: the user is a party, so the party exception
  applies to whatever, if anything, counts as a "communication" there.
- Broadcast system information and paging: transmitted to all UEs in the cell by design; not a
  private communication of a third party. (Paging records do contain other subscribers' temporary
  identities; see 5.1.)
- **Third-party traffic through the hotspot is the real exposure.** Upstream v0.13.0 subscribes to DIAG
  log code `LOG_DATA_PROTOCOL_LOGGING_C` (0x11EB), commented as "User IP traffic" in
  `upstream/lib/src/diag_device.rs:57-58`. EFF's public description says Rayhunter captures control
  traffic "but not user traffic, such as web requests"; what 0x11EB actually stores in QMDL on the
  RC400L is TBD (on-device). If it captures IP packet contents and anyone other than the owner uses the
  hotspot, logging their traffic without their consent could raise Wiretap Act and state-law issues.
  Recommendation for this project: verify on-device, and either drop 0x11EB from the subscription in
  our build or disable hotspot sharing while recording, and document the choice.

### 3.2 Pen Register and Trap and Trace statute (18 U.S.C. 3121-3127)

- 18 U.S.C. 3121(a) prohibits installing or using a pen register or trap-and-trace device without a
  court order, except as provided. 18 U.S.C. 3127(3)-(4) define these as devices or processes recording
  dialing, routing, addressing, or signalling information.
- 18 U.S.C. 3121(b)(3) exempts cases where the consent of the user of the service has been obtained.
- Application: recording one's own device's signalling with one's own consent falls within the consent
  exception. Recording signalling of other subscribers' devices would not. Cell-site simulators
  themselves are regulated by this statute and by DOJ policy (a 2015 DOJ policy requires a warrant for
  federal CSS use); that regulates the operator of a CSS, not a detector.

### 3.3 Fourth Amendment context (not a constraint on users)

Carpenter v. United States (2018) held that the government generally needs a warrant to obtain
historical cell-site location information. It does not regulate private detectors, but it is the
backdrop for why CSS detection data can matter in litigation; treat captures as potential evidence
(integrity, chain of custody, timestamps).

### 3.4 State law

State wiretap statutes can be stricter than federal law. Examples (not exhaustive):

| State | Statute | Note |
| --- | --- | --- |
| California | Penal Code 631, 632, 632.7 (California Invasion of Privacy Act) | All-party consent for confidential communications; 632.7 specifically covers intercepting or recording cellular/cordless communications |
| Illinois | 720 ILCS 5/14 (Eavesdropping) | All-party consent for private conversations |
| Pennsylvania | 18 Pa.C.S. 5701 et seq. | All-party consent; also regulates possession of intercepting devices |
| Washington | RCW 9.73.030 | All-party consent for private communications |
| Maryland, Massachusetts, Florida, others | various | All-party consent regimes |

For own-device control signalling these statutes are unlikely to bite (the user is the only "party"
on the UE side, and broadcast data is not a private conversation), but the third-party hotspot traffic
issue in 3.1 is more serious under all-party-consent states.

Several states also regulate *government* CSS use (warrant or disclosure requirements), for example
Washington (RCW 9.73.260), California (Government Code 53166, from SB 741, 2015) and Illinois (Citizen
Privacy Protection Act, 50 ILCS 707). These are relevant if Rayhunter findings feed a public-records
request or litigation, not as constraints on users.

### 3.5 Device modification

Installing Rayhunter requires root on a device the user owns. Considerations: the carrier's terms of
service and device-financing agreements (contractual, not criminal); the DMCA anti-circumvention rule
(17 U.S.C. 1201) with the Copyright Office's triennial exemptions for unlocking and jailbreaking wireless
devices (37 CFR 201.40; check the current cycle's text for hotspots); the Computer Fraud and Abuse Act
is not implicated by modifying one's own device but would be by accessing anyone else's. Never install
on a device the user does not own or control.

### 3.6 Outside the US

Some countries restrict possession of radio monitoring tools, use of diagnostic interfaces, or
encryption-related research; some criminalize documenting police activity. EFF explicitly asks
non-US users to consult a local attorney. The project should state this prominently in install docs.

---

## 4. Ethics: protests, bystanders and harm

- **False alarms cause harm.** A High alert at a protest can cause people to switch off phones, lose
  contact, or panic. Severities must follow the evidence (see the causal vs correlated distinction in
  `ref-docs/lte-imsi-catcher-detection.md` section 2). EFF's 2025 field report found no CSS evidence at
  US protests and recommended protesters weigh other surveillance (ALPR, data brokers, tower dumps,
  forensic extraction) more heavily; our UI and docs should not overstate CSS risk.
- **Bystanders.** The device inevitably receives paging records naming other subscribers' temporary
  identities and system information of cells nearby. Do not build features that extract, aggregate, or
  publish other people's identifiers.
- **Do not attribute.** A detection is evidence of anomalous network behavior, not proof of who operated
  a device. Public claims about a specific agency or company need corroboration and, ideally, review by
  EFF or a journalist with legal support.
- **Do not hunt.** Physically tracking down a suspected CSS operator can be dangerous and may itself be
  unlawful (e.g. interfering with law enforcement). Report instead (section 6).

---

## 5. Privacy of captured logs

### 5.1 What a capture contains

Rayhunter stores raw QMDL (DIAG) plus derived PCAP/NDJSON reports. Upstream's FAQ warns the downloaded
ZIP "may contain sensitive information such as your IMSI and the unique IDs of cell towers you were
near which could be used to ascertain your location at the time". In detail:

| Data | Where it appears | Sensitivity |
| --- | --- | --- |
| IMSI (SIM identity) | NAS Attach Request / Identity Response; IMSI paging | High: permanent subscriber identifier, linkable to the account holder |
| IMEI / IMEISV (device identity) | NAS Identity Response, Security Mode Complete | High: permanent device identifier |
| GUTI / S-TMSI (temporary) | NAS Attach Accept, GUTI Reallocation, paging | Medium-high: some carriers keep GUTIs for weeks (Hong and Yoon, NDSS 2026: up to 33 days), enabling tracking |
| Cell identities (ECGI, TAC, PCI, EARFCN) with timestamps | SIB1, ML1 logs | High as location history: cell IDs map to locations via public databases |
| GPS fixes | daemon GPS records when GPS mode is enabled (`upstream/daemon/src/gps.rs`) | High: precise location history |
| Other subscribers' S-TMSIs | PCCH paging records | Medium: third-party data |
| APN, assigned IP addresses, PDN configuration | NAS ESM | Medium |
| User IP traffic (possible) | 0x11EB data protocol logging (section 3.1) | High, and may include third parties |
| SIM home PLMN | read via AT from EF_HPLMNwAcT (`upstream/lib/src/sim/generic_at.rs`) | Low-medium |

### 5.2 Handling rules for this project

1. **Default local.** Captures stay on the device or the user's computer. No telemetry. (Upstream has
   none: EFF notes it does not know how many installs exist.) Our repository gitignores `*.qmdl`,
   `*.pcap`, `*.pcapng` and device backups.
2. **Minimize.** Drop log codes we do not analyze (0x11EB first). Do not put IMSI/IMEI/GUTI values into
   analyzer `Event.message` strings; refer to "your IMSI" and, where a comparison is needed, use a
   salted hash held in memory.
3. **Protect at rest.** The device web UI serves captures to anyone on its Wi-Fi; use a strong Wi-Fi
   password, disable Wi-Fi client mode when not needed, and consider encrypting exported archives.
4. **Sanitize before sharing.** Provide (or document) a scrubber that replaces IMSI/IMEI/GUTI and,
   optionally, coarsens cell identities and GPS, while preserving message structure for analysis.
   Note that scrubbing breaks some analyses (home-PLMN checks); keep a mapping key offline if needed.
5. **Retention.** Encourage deleting recordings that triggered nothing, and set a default cap.
6. **Consent.** If anyone else uses the hotspot while recording, tell them, or disable sharing.
7. **Legal process.** Captures can be subpoenaed or seized. Users at risk should consider that the
   device holds their location history.

---

## 6. Responsible disclosure of findings

Different findings go to different recipients:

| Finding | Where to send | Notes |
| --- | --- | --- |
| Suspected CSS activity (an alert you believe is real) | EFF: Signal username `ElectronicFrontierFoundation.90` (per upstream `doc/faq.md`), with capture date, location, device, model, Rayhunter version | Share sanitized data where possible; EFF warns captures contain your IMSI and location |
| Vulnerability in Rayhunter itself | GitHub private vulnerability reporting on EFForg/rayhunter (upstream `SECURITY.md`) | For our fork: our own SECURITY.md and private advisory channel |
| False positive in an upstream heuristic | Public upstream issue with a sanitized capture or synthetic reproduction | Not a security issue; public is fine |
| Carrier misconfiguration (e.g. EEA0/EIA0 in production, unprotected rejects) | The carrier's security/PSIRT contact, or the GSMA Coordinated Vulnerability Disclosure programme for industry-wide protocol issues | Give the carrier time to fix before publication; avoid publishing cell IDs that let others target the misconfigured site |
| Baseband/modem vulnerability (e.g. a crash found while parsing) | The chipset vendor PSIRT (Qualcomm for the RC400L) and the device OEM | Do not publish exploit material; this repository is public |
| Standards-level weakness | Researchers typically go through GSMA CVD and 3GPP SA3 via an operator or vendor member | Academic venues expect this before publication |

General rules: report privately first; give a reasonable embargo (90 days is common practice); never
include other people's identifiers in reports; keep exploit details and attack tooling out of this public
repository; publish detection signatures, not attack recipes.

---

## 7. EFF guidance summary

- Rayhunter is meant for "activists, journalists, and others" to collect data about CSS use; EFF asks
  for captures from protests and from outside the US, and warns 2G signatures are false-positive-prone
  where 2G is live (EFF, 2025-09-10 post; 2026-04-20 one-pager).
- Consult an attorney in your country before running it outside the US (EFF, 2025-09-10).
- Building an IMSI catcher to test it violates FCC rules (upstream FAQ).
- Captures contain your IMSI and location; share deliberately (upstream FAQ).
- Practical self-defense beyond CSS: disable 2G where the OS allows, airplane mode or a Faraday bag
  against tower dumps, strong passcodes against forensic extraction (EFF, 2025-09-10).

---

## Sources

Statutes and regulations (summarized, not quoted; read the current text):

- 47 U.S.C. 301 (license required). <https://www.law.cornell.edu/uscode/text/47/301>
- 47 U.S.C. 333 (willful or malicious interference). <https://www.law.cornell.edu/uscode/text/47/333>
- 47 U.S.C. 302a (devices that interfere with radio reception). <https://www.law.cornell.edu/uscode/text/47/302a>
- 47 U.S.C. 605 (unauthorized publication or use of communications). <https://www.law.cornell.edu/uscode/text/47/605>
- 47 CFR Part 2 (equipment authorization). <https://www.ecfr.gov/current/title-47/chapter-I/subchapter-A/part-2>
- FCC, "Jammer Enforcement". <https://www.fcc.gov/general/jammer-enforcement>
- 18 U.S.C. 2510 (definitions). <https://www.law.cornell.edu/uscode/text/18/2510>
- 18 U.S.C. 2511 (interception and disclosure prohibited). <https://www.law.cornell.edu/uscode/text/18/2511>
- 18 U.S.C. 3121 and 3127 (pen registers and trap and trace devices). <https://www.law.cornell.edu/uscode/text/18/3121> , <https://www.law.cornell.edu/uscode/text/18/3127>
- 17 U.S.C. 1201 and 37 CFR 201.40 (anti-circumvention and exemptions). <https://www.law.cornell.edu/uscode/text/17/1201> , <https://www.ecfr.gov/current/title-37/chapter-II/subchapter-A/part-201/section-201.40>
- Carpenter v. United States, 585 U.S. 296 (2018). <https://www.supremecourt.gov/opinions/17pdf/16-402_h315.pdf>
- U.S. Department of Justice, "Department of Justice Policy Guidance: Use of Cell-Site Simulator Technology" (2015). <https://www.justice.gov/opa/file/767321/download>
- California Penal Code 631, 632, 632.7. <https://leginfo.legislature.ca.gov/faces/codes_displayexpandedbranch.xhtml?tocCode=PEN&division=&title=15.&part=1.&chapter=1.5>.
- California Government Code 53166. <https://leginfo.legislature.ca.gov/faces/codes_displaySection.xhtml?sectionNum=53166.&lawCode=GOV>
- Illinois 720 ILCS 5/14 (Eavesdropping) and 50 ILCS 707 (Citizen Privacy Protection Act). <https://www.ilga.gov/legislation/ilcs/ilcs.asp>
- Pennsylvania 18 Pa.C.S. Chapter 57. <https://www.legis.state.pa.us/>
- Washington RCW 9.73.030 and 9.73.260. <https://app.leg.wa.gov/rcw/default.aspx?cite=9.73>

EFF and upstream:

- EFF, "Rayhunter: What We Have Found So Far" (2025-09-10). <https://www.eff.org/deeplinks/2025/09/rayhunter-what-we-have-found-so-far>
- EFF, "Meet Rayhunter: A New Open Source Tool from EFF to Detect Cellular Spying" (2025-03). <https://www.eff.org/deeplinks/2025/03/meet-rayhunter-new-open-source-tool-eff-detect-cellular-spying>
- EFF, "Rayhunter" one-pager, revised 2026-04-20. <https://www.eff.org/files/2026/04/23/rayhunter_-_usa.pdf>
- Quintin, "Detecting Fake 4G Base Stations in Real Time", Black Hat USA 2020 (slide "Why don't we transmit? EFF Lawyers"). <https://i.blackhat.com/USA-20/Wednesday/us-20-Quintin-Detecting-Fake-4G-Base-Stations-In-Real-Time.pdf>
- Upstream `doc/faq.md`, `SECURITY.md`, `lib/src/diag_device.rs`, `lib/src/sim/generic_at.rs`, `daemon/src/gps.rs` (EFForg/rayhunter v0.13.0, local)

Disclosure programmes:

- GSMA Coordinated Vulnerability Disclosure Programme. <https://www.gsma.com/security/gsma-coordinated-vulnerability-disclosure-programme/>
- Qualcomm Product Security. <https://www.qualcomm.com/company/product-security>

Research cited for context:

- Hong, Yoon, "Passive Multi-Target GUTI Identification via Visual-RF Correlation in LTE Networks", NDSS 2026. <https://www.ndss-symposium.org/wp-content/uploads/2026-f2487-paper.pdf>
- Oh et al., "Devilray", arXiv:2605.19232 (shielded-lab ethics practice). <https://arxiv.org/abs/2605.19232>
- Tucker et al., "Marlin", NDSS 2025 (ethics section). <https://cise.ufl.edu/~butler/pubs/ndss25-tucker-marlin.pdf>
