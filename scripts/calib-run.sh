#!/usr/bin/env bash
# T-OR-061 one-command calibration false-positive snapshot.
#
# Pulls recordings from the device (QMDL + the daemon's analysis NDJSON) and
# prints a per-detector FP table from the daemon's own analysis -- so it
# reflects exactly what ran on-device, with no rebuild. The three v1.2.0
# detectors must be enabled on the device (see docs/ANALYZERS.md, "Field
# calibration"); recordings captured before they were enabled contribute no
# coverage and are reported as such.
#
# Captures hold real traffic and must never be committed. This script refuses a
# <captures-dir> that resolves inside the repo unless git ignores it.
#
# Usage: scripts/calib-run.sh [<captures-dir>] [--ip <addr>] [--all]
#   <captures-dir> default: ~/orbic-captures
#   --all  also tabulate upstream analyzers in the snapshot
set -euo pipefail
# Resolve physically (pwd -P) so the repo-containment check below is not fooled
# by a symlinked repo path.
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"

usage() { sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }

cap="${HOME}/orbic-captures"
ip_args=()
snap_args=()
while [ $# -gt 0 ]; do
    case "$1" in
        -h|--help) usage 0 ;;
        --ip)      ip_args+=(--ip "$2"); shift 2 ;;
        --all)     snap_args+=(--all); shift ;;
        -*)        echo "unknown flag: $1" >&2; usage 2 ;;
        *)         cap="$1"; shift ;;
    esac
done
# Canonicalise WITHOUT creating anything (realpath -m works on a nonexistent
# path), so the repo-containment check runs BEFORE any mkdir -- a rejected path
# must not leave a directory behind inside the repo.
cap="$(realpath -m "$cap")"

# Fail closed: never let real captures land in a tracked path inside the repo.
case "$cap/" in
    "$root"/*)
        if ! git -C "$root" check-ignore -q "$cap"; then
            echo "refusing: captures dir '$cap' is inside the repo and not gitignored." >&2
            echo "Use a path outside the repo (e.g. ~/orbic-captures) -- captures are never committed." >&2
            exit 2
        fi
        ;;
esac
mkdir -p "$cap"

echo "[1/2] pull-logs -> $cap"
(cd "$root" && cargo run -q -p orx-cli -- pull-logs --to "$cap" ${ip_args[@]+"${ip_args[@]}"}) 2>&1 | tail -1

echo "[2/2] snapshot"
python3 "$root/scripts/calib-snapshot.py" "$cap" ${snap_args[@]+"${snap_args[@]}"}
