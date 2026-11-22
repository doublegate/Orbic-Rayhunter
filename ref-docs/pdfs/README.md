# Reference PDFs (local copies)

- Retrieved: 2026-11-22
- The PDFs in this directory are gitignored (`/ref-docs/pdfs/*.pdf` in the root `.gitignore`):
  free to read is not free to redistribute. This README is the tracked record of what each file
  is, where it came from, and what license the source states.
- All files were fetched with `curl` from author or publisher hosts that offer free access. Nothing
  paywalled was fetched. The files are untrusted data: they were only read (text extracted with a
  sandboxed `pdftotext`), never executed or opened in a viewer with scripting.
- To re-fetch: `curl -L -o <filename> <source URL>`. The SHA-256 prefix identifies the exact copy
  read for `ref-docs/lte-imsi-catcher-detection.md` and `ref-docs/legal-and-ethics.md`.

## License column rules

- The license is recorded only as stated on the source page or inside the PDF. "unknown" means no
  license or permission statement was found; it does not mean redistribution is allowed.
- arXiv "non-exclusive distribution license" (arxiv.org/licenses/nonexclusive-distrib/1.0/) grants
  arXiv, not third parties, the right to distribute. It does not permit redistribution here.
- "Redistribution permitted" is "yes" only where a license or notice explicitly grants it, with any
  conditions noted. Tracking any of these in git still needs an explicit `!` negation in
  `.gitignore` with the license noted, per the comment there.

## Files

| File | Work | Canonical source URL (fetched from) | License as stated | Redistribution permitted | SHA-256 (first 16) |
| --- | --- | --- | --- | --- | --- |
| `2015-Borgaonkar-LTE-and-IMSI-Catcher-Myths-BlackHatEU-wp.pdf` | Borgaonkar, Shaik, Asokan, Niemi, Seifert. LTE and IMSI Catcher Myths. Black Hat EU 2015 white paper | <https://www.blackhat.com/docs/eu-15/materials/eu-15-Borgaonkar-LTE-And-IMSI-Catcher-Myths-wp.pdf> | unknown | no (not stated) | 4340a5249af9b460 |
| `2016-Dabrowski-Messenger-Shoots-Back-RAID.pdf` | Dabrowski, Petzl, Weippl. The Messenger Shoots Back: Network Operator Based IMSI Catcher Detection. RAID 2016 (author copy; published by Springer, DOI 10.1007/978-3-319-45719-2_13) | <https://www.ics.uci.edu/~dabrowsa/dabrowski-raid16-providerICdetection101.pdf> | unknown (author copy carries no license statement) | no (not stated) | a12fb2d7e643a92c |
| `2016-Shaik-Practical-Attacks-4G-LTE-NDSS-arXiv.pdf` | Shaik, Borgaonkar, Asokan, Niemi, Seifert. Practical Attacks Against Privacy and Availability in 4G/LTE Mobile Communication Systems. NDSS 2016 (arXiv v3) | <https://arxiv.org/abs/1510.07563> (PDF <https://arxiv.org/pdf/1510.07563>) | arXiv page: non-exclusive distribution license 1.0. PDF first page carries the NDSS notice: "Permission to freely reproduce all or part of this paper for noncommercial purposes is granted provided that copies bear this notice and the full citation on the first page. Reproduction for commercial purposes is strictly prohibited without the prior written consent of the Internet Society..." | yes, noncommercial only, with notice and citation kept (per the NDSS notice in the PDF) | f82ea23b7217c129 |
| `2017-Li-FBS-Radar-NDSS.pdf` | Li, Wang, Wilson, et al. FBS-Radar: Uncovering Fake Base Stations at Scale in the Wild. NDSS 2017 (author-hosted) | <https://ccs.neu.edu/home/cbw/static/pdf/li-ndss17.pdf> (paper page <https://www.ndss-symposium.org/ndss2017/ndss-2017-programme/fbs-radar-uncovering-fake-base-stations-scale-wild/>) | NDSS notice in PDF: noncommercial reproduction permitted with notice and full citation; commercial reproduction prohibited without Internet Society consent. Copyright 2017 Internet Society | yes, noncommercial only, with notice and citation kept | 6fa5a8b19baab9ad |
| `2017-Mjolsnes-Easy-4G-LTE-IMSI-Catchers-arXiv.pdf` | Mjolsnes, Olimid. Easy 4G/LTE IMSI Catchers for Non-Programmers. arXiv 2017 | <https://arxiv.org/abs/1702.04434> | arXiv non-exclusive distribution license 1.0 | no | f157a49ceb223ecd |
| `2017-Ney-SeaGlass-PoPETs.pdf` | Ney, Smith, Cadamuro, Kohno. SeaGlass: Enabling City-Wide IMSI-Catcher Detection. PoPETs 2017(3) | <https://www.petsymposium.org/popets/2017/popets-2017-0027.php> (PDF .../popets-2017-0027.pdf) | Publisher page: "Copyright in PoPETs articles are held by their authors. This article is published under a Creative Commons Attribution-NonCommercial-NoDerivs 3.0 license." | yes, under CC BY-NC-ND 3.0 (attribution, noncommercial, no derivatives) | 03252b4c564883de |
| `2017-Park-White-Stingray-WOOT.pdf` | Park, Shaik, Borgaonkar, Martin, Seifert. White-Stingray: Evaluating IMSI Catchers Detection Applications. USENIX WOOT 2017 (copy hosted by University of Oxford) | <https://www.usenix.org/conference/woot17/workshop-program/presentation/park> (fetched from <https://www.cs.ox.ac.uk/files/9192/paper-final-woot-imsi.pdf>) | unknown. USENIX page states papers are "freely available to everyone" (Open Access commitment) but grants no license | no (not stated) | a592fecc570154f0 |
| `2018-Hussain-LTEInspector-NDSS.pdf` | Hussain, Chowdhury, Mehnaz, Bertino. LTEInspector. NDSS 2018 | <https://www.ndss-symposium.org/wp-content/uploads/2018/02/ndss2018_02A-3_Hussain_paper.pdf> | unknown (no license or permission notice found in extracted text) | no (not stated) | f0ea9cf413c19544 |
| `2019-Hussain-ToRPEDO-Paging-Privacy-NDSS.pdf` | Hussain, Echeverria, Chowdhury, Li, Bertino. Privacy Attacks to the 4G and 5G Cellular Paging Protocols Using Side Channel Information. NDSS 2019 (NSF PAR copy) | <https://par.nsf.gov/servlets/purl/10094854> (paper page <https://www.ndss-symposium.org/ndss-paper/privacy-attacks-to-the-4g-and-5g-cellular-paging-protocols-using-side-channel-information/>) | unknown | no (not stated) | 3540e9f453c0023d |
| `2020-Quintin-Detecting-Fake-4G-Base-Stations-BlackHat-slides.pdf` | Quintin. Detecting Fake 4G Base Stations in Real Time (Crocodile Hunter). Black Hat USA 2020 slides | <https://i.blackhat.com/USA-20/Wednesday/us-20-Quintin-Detecting-Fake-4G-Base-Stations-In-Real-Time.pdf> | unknown | no (not stated) | 65ce5995f331c7c1 |
| `2021-Echeverria-PHOENIX-NDSS.pdf` | Echeverria, Ahmed, Wang, Arif, Hussain, Chowdhury. PHOENIX. NDSS 2021 | <https://www.ndss-symposium.org/wp-content/uploads/ndss2021_4A-3_24390_paper.pdf> | unknown (no notice in extracted text; paper page states none) | no (not stated) | 62ad92c4ec50dc5b |
| `2021-Erni-AdaptOver-Overshadowing-arXiv.pdf` | Erni, Kotuliak, Leu, Roeschlin, Capkun. AdaptOver. ACM MobiCom 2022 (arXiv v3) | <https://arxiv.org/abs/2106.05039> | arXiv non-exclusive distribution license 1.0. PDF carries ACM notice "Copyright held by the owner/author(s)" | no | 97b91e95d1a3089c |
| `2021-Nakarmi-Murat-Multi-RAT-FBS-Detector-arXiv.pdf` | Nakarmi, Ersoy, Soykan, Norrman. Murat: Multi-RAT False Base Station Detector. arXiv 2021 | <https://arxiv.org/abs/2102.08780> | arXiv non-exclusive distribution license 1.0 | no | 12e16dd8395a8db5 |
| `2024-Gotta-Detect-Em-All-FBS-Multi-Step-Attack-arXiv.pdf` | Mubasshir, Karim, Bertino. Gotta Detect 'Em All: Fake Base Station and Multi-Step Attack Detection in Cellular Networks. USENIX Security 2025 (arXiv v4) | <https://arxiv.org/abs/2401.04958> | CC BY 4.0 (arXiv license link <http://creativecommons.org/licenses/by/4.0/>) | yes, with attribution | 5a07b96858670b38 |
| `2025-Erni-GLaDoS-Location-aware-DoS-USENIX.pdf` | Erni, Kotuliak, Baker, Martinovic, Capkun. GLaDoS: Location-aware Denial-of-Service of Cellular Networks. USENIX Security 2025 | <https://www.usenix.org/conference/usenixsecurity25/presentation/erni> (PDF <https://www.usenix.org/system/files/usenixsecurity25-erni.pdf>) | unknown. Page and PDF state open access is "sponsored by USENIX"; no license granted | no (not stated) | de001e06adcd9b47 |
| `2025-Tucker-Marlin-Identity-Exposing-Messages-NDSS.pdf` | Tucker, Bennett, Kotuliak, Erni, Capkun, Butler, Traynor. Detecting IMSI-Catchers by Characterizing Identity Exposing Messages in Cellular Traffic (Marlin). NDSS 2025 (author-hosted) | <https://cise.ufl.edu/~butler/pubs/ndss25-tucker-marlin.pdf> (alternate author copy <https://www.cise.ufl.edu/~tucker/docs/2025-marlin.pdf>) | unknown | no (not stated) | 9d05da1ccd9248eb |
| `2026-EFF-Rayhunter-USA.pdf` | EFF. Rayhunter one-pager, revised 2026-04-20 | <https://www.eff.org/files/2026/04/23/rayhunter_-_usa.pdf> | PDF carries no license statement. The eff.org site footer reads "Copyright (CC BY)"; whether that covers this file was not confirmed | not confirmed (treat as no until verified) | 92c3b9da1972c083 |
| `2026-Hong-Passive-GUTI-Identification-Visual-RF-NDSS.pdf` | Hong, Yoon. Passive Multi-Target GUTI Identification via Visual-RF Correlation in LTE Networks. NDSS 2026 | <https://www.ndss-symposium.org/wp-content/uploads/2026-f2487-paper.pdf> | unknown | no (not stated) | 5bde1b4b63534280 |
| `2026-Oh-Devilray-FBS-Detection-Blind-Spots-arXiv.pdf` | Oh, Kim, Bae, Oh, Park, Tucker, Bennett, Bae, Hong, Traynor, Kim. Devilray. arXiv 2026 (v3, 2026-11-02) | <https://arxiv.org/abs/2605.19232> | arXiv non-exclusive distribution license 1.0 | no | 60512abecb6185f4 |
| `Purification-FBS-Detection-and-Blacklisting-NSF-PAR.pdf` | Purification, Wuthier, Kim, Kim, Chang. Fake Base Station Detection and Blacklisting (NSF PAR copy; venue/year not confirmed, references go to 2024) | <https://par.nsf.gov/servlets/purl/10509865> | unknown | no (not stated) | 5af5592228c044ed |

Summary of explicit redistribution grants: `2024-Gotta-Detect-Em-All-...` (CC BY 4.0),
`2017-Ney-SeaGlass-PoPETs.pdf` (CC BY-NC-ND 3.0), and the two NDSS papers whose PDFs carry the
Internet Society noncommercial-reproduction notice (`2016-Shaik-...`, `2017-Li-FBS-Radar-...`).
Everything else: no grant found.

## Wanted but not downloaded

| Work | Reason |
| --- | --- |
| Dabrowski, Pianta, Klepp, Mulazzani, Weippl. IMSI-Catch Me If You Can: IMSI-Catcher-Catchers. ACSAC 2014. <https://eprints.cs.univie.ac.at/6489/1/DabrowskiEtAl-IMSI-Catcher-Catcher-ACSAC2014.pdf> | Host unreachable on two attempts (connection timeout) |
| Lin Huang. Forcing a Targeted LTE Cellphone into an Eavesdropping Network. HITB 2016 | The HITB materials URL returned HTML, not a PDF; the only PDF found was on a third-party mirror (not an author/publisher host), so it was skipped |
| Yang et al. Hiding in Plain Signal (SigOver). USENIX Security 2019 | Not fetched (time); open access at <https://www.usenix.org/conference/usenixsecurity19/presentation/yang-hojoon> |
| Park et al. LTE security disabled (WiSec 2019); Shaik et al. capability exposure (WiSec 2019) | ACM venues; open copies not located in this pass |
| 3GPP TS 24.301, 36.331, 33.401, 36.304 | Freely available from 3GPP (<https://www.3gpp.org/specifications>); large, not needed locally yet |
