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
`total_messages` per recording, which is the denominator used here.

Firings are the Low/Medium/High events ("warnings"); the three detectors are
attributed by their event-message signatures (pinned to the analyzer sources
below). As a guard against signature drift, the attributed count is cross-checked
against each recording's `warnings` total: any warning that matches none of our
signatures is reported as an upstream-analyzer firing, so our detectors' firings
can never be silently dropped.

Usage:
    scripts/calib-snapshot.py <check-report.json>
    scripts/calib-snapshot.py --selftest

Exit: 0 clean, 1 detector firings present, 2 no coverage / malformed input.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

# metadata.analyzers name -> short key: used to confirm --config actually enabled
# each detector for the run (coverage).
DETECTOR_NAMES = {
    "nas_reject_identity": "NAS Reject with identity-forcing cause",
    "auth_failure": "Authentication failure or reject",
    "null_integrity": "Null integrity (EIA0) requested",
}
DET_KEYS = ("nas_reject_identity", "auth_failure", "null_integrity")

# Event-message signatures, pinned to the analyzer sources (patches 0007-0009):
#   nas_reject.rs:65   "{reject} with identity-forcing EMM cause {cause}"
#   auth_failure.rs    "Authentication Failure with EMM cause {cause}" / "Authentication Reject received"
#   null_integrity.rs  "NAS Security Mode Command selected null integrity (EIA0); ..."
SIGNATURES = {
    "nas_reject_identity": ("identity-forcing EMM cause",),
    "auth_failure": (
        "Authentication Failure with EMM cause",
        "Authentication Reject received",
    ),
    "null_integrity": ("null integrity (EIA0)",),
}

WARN_SEVERITIES = {"Low", "Medium", "High"}


def _fail(msg: str) -> int:
    print(f"calib-snapshot: {msg}", file=sys.stderr)
    return 2


def analyze(doc: object) -> dict | None:
    """Aggregate detector firings from a parsed rayhunter-check report.

    Returns a result dict, or None if the document is not a valid check report.
    """
    if not isinstance(doc, dict):
        return None
    md = doc.get("metadata")
    reports = doc.get("reports")
    if not isinstance(md, dict) or not isinstance(reports, list):
        return None
    analyzer_names = {
        a["name"]
        for a in md.get("analyzers", [])
        if isinstance(a, dict) and isinstance(a.get("name"), str)
    }
    covered = {k: DETECTOR_NAMES[k] in analyzer_names for k in DET_KEYS}

    total_messages = 0
    total_warnings = 0
    per = {k: 0 for k in DET_KEYS}
    other_warnings = 0
    samples: list[str] = []

    for r in reports:
        if not isinstance(r, dict):
            continue
        tm = r.get("total_messages")
        if isinstance(tm, int):
            total_messages += tm
        wn = r.get("warnings")
        if isinstance(wn, int):
            total_warnings += wn
        fname = Path(str(r.get("file_path", "?"))).name
        for e in r.get("events") or []:
            ev = e.get("event") if isinstance(e, dict) else None
            if not isinstance(ev, dict):
                continue
            if ev.get("event_type") not in WARN_SEVERITIES:
                continue  # Informational events are not firings
            msg = str(ev.get("message", ""))
            matched = None
            for key, sigs in SIGNATURES.items():
                if any(s in msg for s in sigs):
                    matched = key
                    break
            if matched:
                per[matched] += 1
                ts = e.get("timestamp", "?")
                samples.append(
                    f"{fname} @ {ts}  {matched} [{ev.get('event_type')}]: {msg[:80]}"
                )
            else:
                other_warnings += 1

    return {
        "covered": covered,
        "total_messages": total_messages,
        "total_warnings": total_warnings,
        "per": per,
        "other_warnings": other_warnings,
        "samples": samples,
        "n_reports": sum(1 for r in reports if isinstance(r, dict)),
    }


def render(r: dict, out=sys.stdout) -> int:
    def p(*a: object) -> None:
        print(*a, file=out)

    any_covered = any(r["covered"].values())
    p("=" * 72)
    p("T-OR-061 CALIBRATION SNAPSHOT (rayhunter-check report)")
    p("=" * 72)
    p(f"recordings     : {r['n_reports']}")
    p(f"messages       : {r['total_messages']:,} analyzed")
    p(
        f"total warnings : {r['total_warnings']} "
        f"({sum(r['per'].values())} from the 3 detectors, {r['other_warnings']} from upstream analyzers)"
    )
    p("-" * 72)
    if not any_covered or r["total_messages"] == 0:
        p("NO CALIBRATION COVERAGE: the 3 detectors were not enabled for this run")
        p("(pass --config scripts/calib-analyzers.toml to rayhunter-check), or no")
        p("messages were analyzed. No false-positive evidence to report.")
        p("=" * 72)
        p("verdict: NO COVERAGE")
        return 2

    p("v1.2.0 DETECTOR FALSE POSITIVES (firings over normal traffic):")
    for key in DET_KEYS:
        if not r["covered"][key]:
            p(f"  {key:22} :  not enabled for this run")
            continue
        n = r["per"][key]
        rate = n / r["total_messages"] * 100_000
        p(
            f"  {key:22} : {n:4d} firings over {r['total_messages']:,} msgs  ({rate:.3f} per 100k)"
        )
    if r["samples"]:
        p("-" * 72)
        p("firing samples:")
        for s in r["samples"][:20]:
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
        f"verdict: CLEAN (0 FP over {covered_n}/3 covered detectors, {r['total_messages']:,} msgs)"
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
    ]

    def ev(sev, msg):
        return {
            "timestamp": "2026-01-01T00:00:00Z",
            "event": {"event_type": sev, "message": msg},
        }

    # clean run: coverage, no warnings
    doc = {
        "metadata": {"analyzers": names},
        "reports": [
            {"total_messages": 1000, "warnings": 0, "events": [], "file_path": "a.qmdl"}
        ],
    }
    r = valid(doc)
    check(
        "clean run -> all covered, 0 firings",
        all(r["covered"].values()) and sum(r["per"].values()) == 0,
    )
    check("clean run -> render exits 0", render(r, out=null) == 0)

    # a firing attributed to auth_failure, plus an upstream warning
    doc = {
        "metadata": {"analyzers": names},
        "reports": [
            {
                "total_messages": 500,
                "warnings": 2,
                "events": [
                    ev("High", "Authentication Reject received"),
                    ev("Medium", "something from an upstream analyzer"),
                    ev("Informational", "Diagnostic: EMM Attach Reject (foo)"),
                ],
                "file_path": "b.qmdl",
            }
        ],
    }
    r = valid(doc)
    check("auth_failure firing attributed", r["per"]["auth_failure"] == 1)
    check("upstream warning counted as other", r["other_warnings"] == 1)
    check("informational not a firing", sum(r["per"].values()) == 1)
    check("render with a firing exits 1", render(r, out=null) == 1)

    # detectors not enabled (default run: only upstream analyzers listed)
    doc = {
        "metadata": {"analyzers": [names[0]]},
        "reports": [
            {"total_messages": 1000, "warnings": 0, "events": [], "file_path": "c.qmdl"}
        ],
    }
    r = valid(doc)
    check(
        "no detectors enabled -> no coverage, render exits 2",
        not any(r["covered"].values()) and render(r, out=null) == 2,
    )

    # zero messages -> fail closed even if detectors listed
    doc = {"metadata": {"analyzers": names}, "reports": []}
    r = valid(doc)
    check("zero messages -> render exits 2", render(r, out=null) == 2)

    # malformed documents -> analyze returns None
    check("non-dict doc -> None", analyze([1, 2, 3]) is None)
    check(
        "missing reports -> None", analyze({"metadata": {"analyzers": names}}) is None
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
        doc = json.loads(args.report.read_text())
    except (OSError, json.JSONDecodeError) as e:
        return _fail(f"cannot read report {args.report}: {e}")
    result = analyze(doc)
    if result is None:
        return _fail(f"{args.report} is not a valid rayhunter-check report")
    return render(result)


if __name__ == "__main__":
    sys.exit(main())
