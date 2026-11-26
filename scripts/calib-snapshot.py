#!/usr/bin/env python3
"""T-OR-061 calibration snapshot: false-positive table for the v1.2.0 detectors.

Parses the analysis NDJSON that the daemon writes alongside each recording (as
pulled by `orx pull-logs`). Every detector firing over normal carrier traffic is
a false positive to understand before deciding an analyzer's default on/off.

This reads the daemon's OWN analysis output, so it reflects exactly what ran
on-device -- no re-analysis, no rebuild. A recording is only counted for a
detector if that detector appears in the recording's analyzer header (i.e. it
was enabled when the daemon analyzed it); recordings captured before the
detectors were enabled are reported as "not covered".

Each NDJSON file is: a header line {analyzers,rayhunter,report_version} followed
by one row per message {packet_timestamp, skipped_message_reason, events:[...]}.
`events` is indexed to the header's `analyzers` array; a non-null entry is a
firing by that analyzer.

Usage:
    scripts/calib-snapshot.py <captures-dir> [--all]

<captures-dir> is a directory of <name>.ndjson files (see docs/ANALYZERS.md,
"Field calibration"). --all also tabulates the upstream analyzers.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

# Detector header names -> short keys (the three v1.2.0 detectors under calibration).
DETECTORS = {
    "NAS Reject with identity-forcing cause": "nas_reject_identity",
    "Authentication failure or reject": "auth_failure",
    "Null integrity (EIA0) requested": "null_integrity",
}


def severity(ev: dict) -> str:
    inner = ev.get("event") if isinstance(ev.get("event"), dict) else ev
    return (inner or {}).get("event_type", "?")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("captures_dir", type=Path)
    ap.add_argument(
        "--all", action="store_true", help="also tabulate upstream analyzers"
    )
    args = ap.parse_args()

    files = sorted(args.captures_dir.glob("*.ndjson"))
    if not files:
        print(f"no .ndjson files in {args.captures_dir}", file=sys.stderr)
        return 2

    det_firings = {k: 0 for k in DETECTORS.values()}
    other_firings: dict[str, int] = {}
    covered_msgs = 0  # messages in recordings where the detectors were enabled
    total_msgs = 0
    covered_recs = 0
    uncovered_recs = 0
    firing_samples: list[str] = []

    for f in files:
        lines = [ln for ln in f.read_text().splitlines() if ln.strip()]
        if not lines:
            continue
        try:
            header = json.loads(lines[0])
        except json.JSONDecodeError:
            continue
        names = [a["name"] for a in header.get("analyzers", [])]
        det_idx = {i: DETECTORS[n] for i, n in enumerate(names) if n in DETECTORS}
        rows = lines[1:]
        total_msgs += len(rows)
        if not det_idx:
            uncovered_recs += 1
            continue
        covered_recs += 1
        covered_msgs += len(rows)
        for ln in rows:
            try:
                row = json.loads(ln)
            except json.JSONDecodeError:
                continue
            events = row.get("events") or []
            for i, ev in enumerate(events):
                if ev is None:
                    continue
                name = names[i] if i < len(names) else f"#{i}"
                if i in det_idx:
                    key = det_idx[i]
                    det_firings[key] += 1
                    msg = (
                        (ev.get("event") or ev).get("message", "")
                        if isinstance(ev, dict)
                        else str(ev)
                    )
                    firing_samples.append(f"{key} [{severity(ev)}]: {msg[:80]}")
                else:
                    other_firings[name] = other_firings.get(name, 0) + 1

    print("=" * 70)
    print("T-OR-061 CALIBRATION SNAPSHOT (daemon NDJSON analysis)")
    print("=" * 70)
    print(
        f"recordings            : {len(files)}  ({covered_recs} detector-enabled, "
        f"{uncovered_recs} pre-enablement/not covered)"
    )
    print(f"messages (all/covered): {total_msgs:,} / {covered_msgs:,}")
    print("-" * 70)
    print("v1.2.0 DETECTOR FALSE POSITIVES (firings over covered traffic):")
    for key in ("nas_reject_identity", "auth_failure", "null_integrity"):
        n = det_firings[key]
        rate = (n / covered_msgs * 100_000) if covered_msgs else 0
        print(f"  {key:22} : {n:4d} firings   ({rate:.3f} per 100k covered msgs)")
    if firing_samples:
        print("-" * 70)
        print("firing samples:")
        for s in firing_samples[:20]:
            print("   -", s)
    if args.all and other_firings:
        print("-" * 70)
        print("other (upstream) analyzer firings:")
        for name, n in sorted(other_firings.items(), key=lambda kv: -kv[1]):
            print(f"  {n:5d}  {name}")
    print("=" * 70)
    total = sum(det_firings.values())
    print(
        "verdict:",
        "CLEAN (0 FP)" if total == 0 else f"{total} detector firings -- investigate",
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
