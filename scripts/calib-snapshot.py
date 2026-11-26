#!/usr/bin/env python3
"""T-OR-061 calibration snapshot: false-positive table for the v1.2.0 detectors.

Parses the analysis NDJSON that the daemon writes alongside each recording (as
pulled by `orx pull-logs`) and prints a per-detector false-positive table. Every
detector firing over normal carrier traffic is a false positive to understand
before deciding an analyzer's default on/off.

It reads the daemon's OWN analysis output, so it reflects exactly what ran
on-device -- no re-analysis, no rebuild. Coverage is tracked PER DETECTOR: a
recording counts toward a detector's denominator only if that detector appears
in the recording's analyzer header (i.e. it was enabled when the daemon analyzed
it). Recordings captured before a detector was enabled contribute no coverage
for it, and the tool fails closed (exit 2) when NO detector-enabled traffic was
analyzed, rather than printing a misleading "clean" verdict.

Only messages that were actually analyzed count toward the denominator: rows the
daemon skipped (`skipped_message_reason`) and malformed rows are counted and
reported separately, never folded into the FP rate.

Each NDJSON file is a header line {analyzers,rayhunter,report_version} followed
by one row per message {packet_timestamp, skipped_message_reason, events:[...]}.
`events` is indexed to the header's `analyzers` array; a non-null entry is a
firing by that analyzer.

Usage:
    scripts/calib-snapshot.py <captures-dir> [--all]
    scripts/calib-snapshot.py --selftest

<captures-dir> holds <name>.ndjson files (see docs/ANALYZERS.md, "Field
calibration"). --all also tabulates the upstream analyzers across every
recording. Exit: 0 clean, 1 firings present, 2 no coverage / usage error.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

# Detector header names -> short keys (the three v1.2.0 detectors under
# calibration). The strings are the analyzers' get_name() values from patches
# 0007 (nas_reject_identity), 0008 (auth_failure), 0009 (null_integrity); keep
# them in sync if an analyzer is renamed, or matching silently breaks.
DETECTORS = {
    "NAS Reject with identity-forcing cause": "nas_reject_identity",
    "Authentication failure or reject": "auth_failure",
    "Null integrity (EIA0) requested": "null_integrity",
}
DET_KEYS = ("nas_reject_identity", "auth_failure", "null_integrity")


def _message(ev: object) -> str:
    if isinstance(ev, dict):
        inner = ev.get("event") if isinstance(ev.get("event"), dict) else ev
        return str((inner or {}).get("message", ""))
    return str(ev)


def _severity(ev: object) -> str:
    if isinstance(ev, dict):
        inner = ev.get("event") if isinstance(ev.get("event"), dict) else ev
        return str((inner or {}).get("event_type", "?"))
    return "?"


def analyze(files: list[Path], want_all: bool = False) -> dict:
    """Aggregate detector firings and per-detector coverage over NDJSON files.

    Streams each file line by line (soak captures can be large). Returns a result
    dict; see `render` for the fields consumed.
    """
    per = {k: {"covered_msgs": 0, "firings": 0} for k in DET_KEYS}
    other: dict[str, int] = {}
    samples: list[str] = []
    recs_total = recs_covered = recs_unreadable = 0
    skipped_rows = malformed_rows = analyzed_rows = 0

    for f in files:
        recs_total += 1
        names: list[str] = []
        det_idx: dict[int, str] = {}
        header_ok = False
        rec_analyzed = 0
        # The NDJSON comes from the device and is an untrusted boundary: read
        # bytes and decode per line so a garbled byte (e.g. a flash cut on power
        # loss) is contained to one line, and validate every shape before use.
        unreadable = False
        try:
            fh = f.open("rb")
        except OSError:
            recs_unreadable += 1
            continue
        with fh:
            for raw in fh:
                try:
                    line = raw.decode("utf-8").strip()
                except UnicodeDecodeError:
                    if not header_ok:
                        unreadable = True
                        break
                    malformed_rows += 1
                    continue
                if not line:
                    continue
                if not header_ok:
                    try:
                        header = json.loads(line)
                    except json.JSONDecodeError:
                        unreadable = True
                        break
                    if not isinstance(header, dict) or not isinstance(
                        header.get("analyzers"), list
                    ):
                        unreadable = True
                        break
                    names = [
                        a["name"]
                        for a in header["analyzers"]
                        if isinstance(a, dict) and isinstance(a.get("name"), str)
                    ]
                    det_idx = {
                        i: DETECTORS[n] for i, n in enumerate(names) if n in DETECTORS
                    }
                    if det_idx:
                        recs_covered += 1
                    header_ok = True
                    continue
                try:
                    row = json.loads(line)
                except json.JSONDecodeError:
                    malformed_rows += 1
                    continue
                if not isinstance(row, dict):
                    malformed_rows += 1
                    continue
                if row.get("skipped_message_reason") is not None:
                    skipped_rows += 1
                    continue
                events = row.get("events")
                if not isinstance(events, list):
                    # not skipped, but no analyzable events array -> schema-invalid
                    malformed_rows += 1
                    continue
                analyzed_rows += 1
                rec_analyzed += 1
                for i, ev in enumerate(events):
                    if ev is None:
                        continue
                    if i in det_idx:
                        key = det_idx[i]
                        per[key]["firings"] += 1
                        ts = row.get("packet_timestamp") or "?"
                        samples.append(
                            f"{f.name} @ {ts}  {key} [{_severity(ev)}]: {_message(ev)[:80]}"
                        )
                    elif want_all:
                        name = names[i] if i < len(names) else f"#{i}"
                        other[name] = other.get(name, 0) + 1
        # An empty or header-less file provides no evidence -> unreadable.
        if not header_ok:
            unreadable = True
        if unreadable:
            recs_unreadable += 1
            continue
        # Credit this recording's analyzed messages to each detector it enabled.
        for key in det_idx.values():
            per[key]["covered_msgs"] += rec_analyzed

    return {
        "per": per,
        "other": other,
        "samples": samples,
        "recs_total": recs_total,
        "recs_covered": recs_covered,
        "recs_unreadable": recs_unreadable,
        "skipped_rows": skipped_rows,
        "malformed_rows": malformed_rows,
        "analyzed_rows": analyzed_rows,
    }


def render(r: dict, want_all: bool, out=sys.stdout) -> int:
    per = r["per"]
    any_cov = any(per[k]["covered_msgs"] > 0 for k in DET_KEYS)

    def p(*a: object) -> None:
        print(*a, file=out)

    p("=" * 72)
    p("T-OR-061 CALIBRATION SNAPSHOT (daemon NDJSON analysis)")
    p("=" * 72)
    p(
        f"recordings       : {r['recs_total']}  "
        f"({r['recs_covered']} detector-enabled, "
        f"{r['recs_total'] - r['recs_covered'] - r['recs_unreadable']} pre-enablement, "
        f"{r['recs_unreadable']} unreadable)"
    )
    p(
        f"messages         : {r['analyzed_rows']:,} analyzed  "
        f"({r['skipped_rows']:,} skipped, {r['malformed_rows']:,} malformed rows -- excluded)"
    )
    p("-" * 72)
    if not any_cov:
        p("NO CALIBRATION COVERAGE: none of the 3 detectors were enabled for any")
        p("analyzed message. Enable them on the device and capture traffic first")
        p('(see docs/ANALYZERS.md, "Field calibration").')
        p("=" * 72)
        p("verdict: NO COVERAGE")
        return 2

    p("v1.2.0 DETECTOR FALSE POSITIVES (firings over each detector's own coverage):")
    total_firings = 0
    for key in DET_KEYS:
        cov = per[key]["covered_msgs"]
        fir = per[key]["firings"]
        total_firings += fir
        if cov == 0:
            p(f"  {key:22} :  no coverage (not enabled in any analyzed recording)")
        else:
            rate = fir / cov * 100_000
            p(f"  {key:22} : {fir:4d} firings over {cov:,} msgs  ({rate:.3f} per 100k)")
    if r["samples"]:
        p("-" * 72)
        p("firing samples:")
        for s in r["samples"][:20]:
            p("   -", s)
    if want_all and r["other"]:
        p("-" * 72)
        p("other (upstream) analyzer firings:")
        for name, n in sorted(r["other"].items(), key=lambda kv: -kv[1]):
            p(f"  {n:5d}  {name}")
    p("=" * 72)
    covered = sum(1 for k in DET_KEYS if per[k]["covered_msgs"] > 0)
    if total_firings:
        p(
            f"verdict: {total_firings} detector firings -- investigate ({covered}/3 detectors covered)"
        )
        return 1
    p(f"verdict: CLEAN (0 FP over {covered}/3 covered detectors)")
    return 0


def selftest() -> int:
    """Synthetic NDJSON cases covering the correctness fixes."""
    import io
    import tempfile

    ok = True
    null = io.StringIO()

    def check(label: str, cond: bool) -> None:
        nonlocal ok
        ok = ok and cond
        print(f"  {'ok  ' if cond else 'FAIL'}  {label}")

    hdr10 = {
        "analyzers": [
            {"name": "Null Cipher", "version": 1},
            {"name": "NAS Reject with identity-forcing cause", "version": 0},
            {"name": "Authentication failure or reject", "version": 0},
            {"name": "Null integrity (EIA0) requested", "version": 0},
        ],
        "rayhunter": {},
        "report_version": 1,
    }
    hdr_old = {
        "analyzers": [{"name": "Null Cipher", "version": 1}],
        "rayhunter": {},
        "report_version": 1,
    }

    def row(events, skipped=None):
        return json.dumps(
            {
                "packet_timestamp": "2026-01-01T00:00:00Z",
                "skipped_message_reason": skipped,
                "events": events,
            }
        )

    with tempfile.TemporaryDirectory() as d:
        dd = Path(d)
        # zero coverage: only the old 7-analyzer-style header, no detectors
        (dd / "old.ndjson").write_text(json.dumps(hdr_old) + "\n" + row([None]) + "\n")
        r = analyze([dd / "old.ndjson"])
        check(
            "zero coverage -> all detectors uncovered",
            all(r["per"][k]["covered_msgs"] == 0 for k in DET_KEYS),
        )
        check("zero coverage -> render exits 2", render(r, False, out=null) == 2)

        # full coverage, one firing on auth_failure (index 2), one skipped row
        fire = [
            None,
            None,
            {
                "event": {
                    "event_type": "High",
                    "message": "Authentication Reject received",
                }
            },
            None,
        ]
        (dd / "cov.ndjson").write_text(
            json.dumps(hdr10)
            + "\n"
            + row([None, None, None, None])
            + "\n"
            + row(fire)
            + "\n"
            + row([None, None, None, None], skipped="UnsupportedGsmtapType")
            + "\n"
        )
        r = analyze([dd / "cov.ndjson"])
        check("skipped row excluded from denominator", r["analyzed_rows"] == 2)
        check(
            "auth_failure credited 1 firing", r["per"]["auth_failure"]["firings"] == 1
        )
        check(
            "auth_failure covered_msgs == analyzed (2)",
            r["per"]["auth_failure"]["covered_msgs"] == 2,
        )
        check("render with a firing exits 1", render(r, False, out=null) == 1)

        # partial enablement: header missing null_integrity would read as "no coverage"
        # for that one; here all 3 are present, so confirm clean exits 0
        (dd / "clean.ndjson").write_text(
            json.dumps(hdr10) + "\n" + row([None, None, None, None]) + "\n"
        )
        r = analyze([dd / "clean.ndjson"])
        check("clean covered -> render exits 0", render(r, False, out=null) == 0)

        # corrupt header -> unreadable, not counted as covered
        (dd / "bad.ndjson").write_text("{not json\n")
        r = analyze([dd / "bad.ndjson"])
        check(
            "corrupt header -> unreadable, no coverage",
            r["recs_unreadable"] == 1 and r["recs_covered"] == 0,
        )

        # malformed row counted separately, not in analyzed
        (dd / "malf.ndjson").write_text(
            json.dumps(hdr10) + "\n" + "{bad\n" + row([None, None, None, None]) + "\n"
        )
        r = analyze([dd / "malf.ndjson"])
        check(
            "malformed row counted, excluded from analyzed",
            r["malformed_rows"] == 1 and r["analyzed_rows"] == 1,
        )

        # non-object JSON header (null / [] / number) -> unreadable, no crash
        (dd / "nonobj.ndjson").write_text(
            "[1, 2, 3]\n" + row([None, None, None, None]) + "\n"
        )
        r = analyze([dd / "nonobj.ndjson"])
        check(
            "non-object header -> unreadable",
            r["recs_unreadable"] == 1 and r["recs_covered"] == 0,
        )

        # header dict without "analyzers" -> unreadable
        (dd / "noan.ndjson").write_text('{"rayhunter": {}}\n' + row([None]) + "\n")
        r = analyze([dd / "noan.ndjson"])
        check("header without analyzers -> unreadable", r["recs_unreadable"] == 1)

        # empty file -> unreadable, not a clean recording
        (dd / "empty.ndjson").write_text("")
        r = analyze([dd / "empty.ndjson"])
        check(
            "empty file -> unreadable",
            r["recs_unreadable"] == 1 and r["recs_covered"] == 0,
        )

        # non-object row and row lacking events -> malformed, not analyzed
        (dd / "badrows.ndjson").write_text(
            json.dumps(hdr10)
            + "\n"
            + "123\n"
            + '{"packet_timestamp": "x", "skipped_message_reason": null}\n'
        )
        r = analyze([dd / "badrows.ndjson"])
        check(
            "non-object / no-events rows -> malformed, 0 analyzed",
            r["malformed_rows"] == 2 and r["analyzed_rows"] == 0,
        )

        # invalid UTF-8 in the header line -> unreadable, no exception
        (dd / "utf8.ndjson").write_bytes(b"\xff\xfe not utf8\n")
        r = analyze([dd / "utf8.ndjson"])
        check("invalid UTF-8 header -> unreadable", r["recs_unreadable"] == 1)

    print("SELFTEST:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


def main() -> int:
    ap = argparse.ArgumentParser(description="T-OR-061 calibration FP snapshot")
    ap.add_argument("captures_dir", nargs="?", type=Path)
    ap.add_argument(
        "--all", action="store_true", help="also tabulate upstream analyzers"
    )
    ap.add_argument(
        "--selftest", action="store_true", help="run synthetic self-tests and exit"
    )
    args = ap.parse_args()

    if args.selftest:
        return selftest()
    if args.captures_dir is None:
        ap.error("captures_dir is required (or pass --selftest)")

    files = sorted(args.captures_dir.glob("*.ndjson"))
    if not files:
        print(f"no .ndjson files in {args.captures_dir}", file=sys.stderr)
        return 2
    return render(analyze(files, want_all=args.all), want_all=args.all)


if __name__ == "__main__":
    sys.exit(main())
