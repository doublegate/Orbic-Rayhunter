#!/usr/bin/env python3
"""T-OR-061 calibration snapshot: false-positive table for the v1.2.0 detectors.

Reads a `rayhunter-check` JSON report (produced over pulled QMDL with the
detectors enabled via `--config`, see scripts/calib-run.sh) and prints a
per-detector false-positive table. Every detector firing over normal carrier
traffic is a false positive to understand before deciding an analyzer's default
on/off.

Why the check report and not the daemon's live NDJSON: the daemon only writes an
analysis row for a *skipped* or *firing* message (`if !row.is_empty()`), so its
NDJSON has no count of cleanly-analyzed messages and therefore no denominator for
a false-positive rate. `rayhunter-check` processes every message and records
`total_messages` per recording, used here as the denominator.

Firings are the Low/Medium/High events ("warnings"). Each event carries the
`analyzer` name that produced it (patch 0010), so attribution is exact -- no
message-text matching. As a correctness guard the attributed firings are
cross-checked against each recording's declared `warnings` total; a mismatch, or
any schema violation, fails the run closed (exit 2) rather than reporting a
possibly-incomplete CLEAN verdict.

Usage:
    scripts/calib-snapshot.py <check-report.json>
    scripts/calib-snapshot.py --selftest

Exit: 0 clean, 1 detector firings present, 2 no coverage / mismatch / malformed.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

# metadata.analyzers name -> short key (the three v1.2.0 detectors under
# calibration). Attribution is by these exact names, which the check report
# copies from each analyzer's get_name().
DETECTOR_NAMES = {
    "NAS Reject with identity-forcing cause": "nas_reject_identity",
    "Authentication failure or reject": "auth_failure",
    "Null integrity (EIA0) requested": "null_integrity",
    "Permanent identity disclosure": "identity_disclosure",
    "Inter-RAT downgrade command / sunset-RAT advertisement": "rat_downgrade",
}
KEY_FROM_NAME = {v: k for k, v in DETECTOR_NAMES.items()}
DET_KEYS = (
    "nas_reject_identity",
    "auth_failure",
    "null_integrity",
    "identity_disclosure",
    "rat_downgrade",
)

WARN_SEVERITIES = {"Low", "Medium", "High"}
VALID_SEVERITIES = WARN_SEVERITIES | {"Informational"}


def _fail(msg: str) -> int:
    print(f"calib-snapshot: {msg}", file=sys.stderr)
    return 2


def analyze(doc: object) -> dict | None:
    """Aggregate detector firings from a parsed rayhunter-check report.

    Returns a result dict, or None if the document violates the report schema
    (fail closed -- a malformed report must not yield a clean verdict).
    """
    if not isinstance(doc, dict):
        return None
    md = doc.get("metadata")
    reports = doc.get("reports")
    if not isinstance(md, dict) or not isinstance(reports, list):
        return None
    analyzers = md.get("analyzers")
    if not isinstance(analyzers, list):
        return None
    # Fail closed on any malformed analyzer entry rather than filtering it out
    # (a filtered entry would shift nothing here, but a missing name is a sign of
    # a report we do not understand).
    analyzer_names: set[str] = set()
    for a in analyzers:
        if not isinstance(a, dict) or not isinstance(a.get("name"), str):
            return None
        analyzer_names.add(a["name"])
    covered = {k: KEY_FROM_NAME[k] in analyzer_names for k in DET_KEYS}

    total_messages = 0
    skipped_messages = 0
    declared_warnings = 0
    per = {k: 0 for k in DET_KEYS}
    other_warnings = 0
    samples: list[str] = []
    unmatched: list[str] = []

    for r in reports:
        if not isinstance(r, dict):
            return None  # fail closed on a malformed report entry
        tm = r.get("total_messages")
        sk = r.get("skipped")
        wn = r.get("warnings")
        events = r.get("events")
        if (
            not isinstance(tm, int)
            or not isinstance(sk, int)
            or not isinstance(wn, int)
            or not isinstance(events, list)
            or not (0 <= sk <= tm)
        ):
            return None
        total_messages += tm
        skipped_messages += sk
        declared_warnings += wn
        fname = Path(str(r.get("file_path", "?"))).name
        rec_warnings = 0
        for e in events:
            if not isinstance(e, dict):
                return None
            ev = e.get("event")
            if not isinstance(ev, dict):
                return None
            sev = ev.get("event_type")
            if sev not in VALID_SEVERITIES:
                return None  # unknown severity -> do not assume Informational
            if sev not in WARN_SEVERITIES:
                continue  # Informational events are not firings
            rec_warnings += 1
            name = e.get("analyzer")
            if not isinstance(name, str) or name not in analyzer_names:
                return None  # a warning we cannot attribute -> fail closed
            msg = str(ev.get("message", ""))
            key = DETECTOR_NAMES.get(name)
            if key is not None:
                per[key] += 1
                ts = e.get("timestamp", "?")
                samples.append(f"{fname} @ {ts}  {key} [{sev}]: {msg[:80]}")
            else:
                other_warnings += 1
                unmatched.append(f"{fname}  [{name}] {msg[:80]}")
        # Per-recording cross-check: the warning events must account for exactly
        # this recording's declared warnings (a global total could net out a
        # surplus in one recording against a deficit in another).
        if rec_warnings != wn:
            return None

    return {
        "covered": covered,
        "total_messages": total_messages,
        "skipped_messages": skipped_messages,
        "analyzed_messages": total_messages - skipped_messages,
        "declared_warnings": declared_warnings,
        "per": per,
        "other_warnings": other_warnings,
        "samples": samples,
        "unmatched": unmatched,
        "n_reports": len(reports),
    }


def render(r: dict, out=sys.stdout) -> int:
    def p(*a: object) -> None:
        print(*a, file=out)

    p("=" * 72)
    p("T-OR-061 CALIBRATION SNAPSHOT (rayhunter-check report)")
    p("=" * 72)
    p(f"recordings     : {r['n_reports']}")
    p(
        f"messages       : {r['analyzed_messages']:,} analyzed "
        f"({r['skipped_messages']:,} skipped of {r['total_messages']:,} total -- excluded)"
    )
    p(
        f"warnings       : {r['declared_warnings']} declared "
        f"({sum(r['per'].values())} from the 3 detectors, {r['other_warnings']} from upstream analyzers)"
    )
    p("-" * 72)

    # analyze() has already failed closed (returned None -> exit 2) on any
    # per-recording warning-count mismatch or schema violation, so a result here
    # is internally consistent: declared warnings == detector firings + other.
    any_covered = any(r["covered"].values())
    if not any_covered or r["analyzed_messages"] == 0:
        p("NO CALIBRATION COVERAGE: the 3 detectors were not enabled for this run")
        p("(pass --config scripts/calib-analyzers.toml to rayhunter-check), or no")
        p("messages were actually analyzed (all skipped). No evidence to report.")
        p("=" * 72)
        p("verdict: NO COVERAGE")
        return 2

    p("v1.2.0 DETECTOR FALSE POSITIVES (firings over normal traffic):")
    for key in DET_KEYS:
        if not r["covered"][key]:
            p(f"  {key:22} :  not enabled for this run")
            continue
        n = r["per"][key]
        rate = n / r["analyzed_messages"] * 100_000
        p(
            f"  {key:22} : {n:4d} firings over {r['analyzed_messages']:,} msgs  ({rate:.3f} per 100k)"
        )
    if r["samples"]:
        p("-" * 72)
        p("detector firing samples:")
        for s in r["samples"][:20]:
            p("   -", s)
    if r["unmatched"]:
        p("-" * 72)
        p(
            f"upstream-analyzer warnings ({len(r['unmatched'])}; shown so drift is never hidden):"
        )
        for s in r["unmatched"][:20]:
            p("   -", s)
    p("=" * 72)
    our = sum(r["per"].values())
    covered_n = sum(1 for k in DET_KEYS if r["covered"][k])
    if our:
        p(
            f"verdict: {our} detector firings -- investigate ({covered_n}/3 detectors covered)"
        )
        return 1
    p(
        f"verdict: CLEAN (0 FP over {covered_n}/3 covered detectors, {r['analyzed_messages']:,} analyzed msgs)"
    )
    return 0


def selftest() -> int:
    import io

    ok = True
    null = io.StringIO()

    def check(label: str, cond: bool) -> None:
        nonlocal ok
        ok = ok and cond
        print(f"  {'ok  ' if cond else 'FAIL'}  {label}")

    def valid(doc: object) -> dict:
        res = analyze(doc)
        assert res is not None
        return res

    names = [
        {
            "name": "Identity (IMSI or IMEI) requested in suspicious manner",
            "version": 5,
        },
        {"name": "NAS Reject with identity-forcing cause", "version": 0},
        {"name": "Authentication failure or reject", "version": 0},
        {"name": "Null integrity (EIA0) requested", "version": 0},
        {"name": "Permanent identity disclosure", "version": 0},
        {
            "name": "Inter-RAT downgrade command / sunset-RAT advertisement",
            "version": 0,
        },
        {"name": "Null Cipher", "version": 1},
    ]

    def ev(analyzer, sev, msg):
        return {
            "timestamp": "2026-01-01T00:00:00Z",
            "analyzer": analyzer,
            "event": {"event_type": sev, "message": msg},
        }

    def rep(total, warnings, events, skipped=0, fp="a.qmdl"):
        return {
            "total_messages": total,
            "skipped": skipped,
            "warnings": warnings,
            "events": events,
            "file_path": fp,
        }

    # clean run: coverage, no warnings
    doc = {"metadata": {"analyzers": names}, "reports": [rep(1000, 0, [])]}
    r = valid(doc)
    check(
        "clean run -> all covered, 0 firings",
        all(r["covered"].values()) and sum(r["per"].values()) == 0,
    )
    check("clean run -> render exits 0", render(r, out=null) == 0)

    # a firing attributed to auth_failure by analyzer name, plus an upstream warning
    doc = {
        "metadata": {"analyzers": names},
        "reports": [
            rep(
                500,
                2,
                [
                    ev(
                        "Authentication failure or reject",
                        "High",
                        "Authentication Reject received",
                    ),
                    ev("Null Cipher", "Medium", "null cipher suggested"),
                    ev("Diagnostic", "Informational", "Diagnostic: foo"),
                ],
            )
        ],
    }
    r = valid(doc)
    check(
        "auth_failure firing attributed by analyzer name", r["per"]["auth_failure"] == 1
    )
    check("upstream warning counted as other", r["other_warnings"] == 1)
    check(
        "informational not a firing", sum(r["per"].values()) + r["other_warnings"] == 2
    )
    check("firing render exits 1", render(r, out=null) == 1)

    # per-recording warning-count mismatch (declares a warning with no event) -> None
    check(
        "warning declared but no event -> None",
        analyze({"metadata": {"analyzers": names}, "reports": [rep(1000, 1, [])]})
        is None,
    )
    # a warning event whose analyzer is unknown / missing -> fail closed
    check(
        "warning with unknown analyzer -> None",
        analyze(
            {
                "metadata": {"analyzers": names},
                "reports": [rep(10, 1, [ev("Some Renamed Analyzer", "High", "x")])],
            }
        )
        is None,
    )
    check(
        "warning with missing analyzer field -> None",
        analyze(
            {
                "metadata": {"analyzers": names},
                "reports": [
                    rep(
                        10,
                        1,
                        [
                            {
                                "timestamp": "t",
                                "event": {"event_type": "High", "message": "x"},
                            }
                        ],
                    )
                ],
            }
        )
        is None,
    )
    # unknown severity string -> fail closed (not silently treated as informational)
    check(
        "unknown event_type -> None",
        analyze(
            {
                "metadata": {"analyzers": names},
                "reports": [rep(10, 0, [ev("Null Cipher", "Critical", "x")])],
            }
        )
        is None,
    )
    # malformed analyzer entry in metadata -> fail closed
    check(
        "malformed analyzer entry -> None",
        analyze(
            {"metadata": {"analyzers": [{"version": 1}]}, "reports": [rep(10, 0, [])]}
        )
        is None,
    )
    # per-recording check is not netted across recordings
    check(
        "cross-recording net-out -> None",
        analyze(
            {
                "metadata": {"analyzers": names},
                "reports": [
                    rep(10, 1, [], fp="a"),
                    rep(10, 0, [ev("Null Cipher", "High", "x")], fp="b"),
                ],
            }
        )
        is None,
    )

    # detectors not enabled -> no coverage
    doc = {"metadata": {"analyzers": [names[0]]}, "reports": [rep(1000, 0, [])]}
    r = valid(doc)
    check("no detectors enabled -> render exits 2", render(r, out=null) == 2)

    # zero messages -> fail closed
    r = valid({"metadata": {"analyzers": names}, "reports": []})
    check("zero messages -> render exits 2", render(r, out=null) == 2)

    # all messages skipped -> analyzed == 0 -> fail closed, not a clean verdict
    r = valid(
        {"metadata": {"analyzers": names}, "reports": [rep(1000, 0, [], skipped=1000)]}
    )
    check("all skipped -> render exits 2 (NO COVERAGE)", render(r, out=null) == 2)

    # skipped excluded from the denominator (analyzed = total - skipped)
    r = valid(
        {"metadata": {"analyzers": names}, "reports": [rep(1000, 0, [], skipped=400)]}
    )
    check("analyzed = total - skipped", r["analyzed_messages"] == 600)
    check(
        "skipped > total -> None",
        analyze(
            {"metadata": {"analyzers": names}, "reports": [rep(10, 0, [], skipped=11)]}
        )
        is None,
    )
    check(
        "non-int skipped -> None",
        analyze(
            {
                "metadata": {"analyzers": names},
                "reports": [
                    {"total_messages": 10, "skipped": None, "warnings": 0, "events": []}
                ],
            }
        )
        is None,
    )

    # schema violations -> analyze returns None (fail closed)
    check("non-dict doc -> None", analyze([1, 2, 3]) is None)
    check(
        "analyzers not a list -> None",
        analyze({"metadata": {"analyzers": 5}, "reports": []}) is None,
    )
    check(
        "events not a list -> None",
        analyze(
            {
                "metadata": {"analyzers": names},
                "reports": [
                    {"total_messages": 1, "skipped": 0, "warnings": 0, "events": 7}
                ],
            }
        )
        is None,
    )
    check(
        "non-int total_messages -> None",
        analyze(
            {
                "metadata": {"analyzers": names},
                "reports": [
                    {"total_messages": None, "skipped": 0, "warnings": 0, "events": []}
                ],
            }
        )
        is None,
    )
    check(
        "non-dict report entry -> None",
        analyze({"metadata": {"analyzers": names}, "reports": [42]}) is None,
    )

    print("SELFTEST:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


def main() -> int:
    ap = argparse.ArgumentParser(description="T-OR-061 calibration FP snapshot")
    ap.add_argument("report", nargs="?", type=Path, help="rayhunter-check JSON report")
    ap.add_argument(
        "--selftest", action="store_true", help="run synthetic self-tests and exit"
    )
    args = ap.parse_args()

    if args.selftest:
        return selftest()
    if args.report is None:
        ap.error("report is required (or pass --selftest)")
    try:
        doc = json.loads(args.report.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as e:
        return _fail(f"cannot read report {args.report}: {e}")
    result = analyze(doc)
    if result is None:
        return _fail(f"{args.report} is not a valid rayhunter-check report")
    return render(result)


if __name__ == "__main__":
    sys.exit(main())
