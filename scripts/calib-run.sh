#!/usr/bin/env bash
# T-OR-061 one-command calibration false-positive snapshot.
#
# Pulls recordings from the device, re-analyzes every message offline with
# rayhunter-check (the v1.2.0 detectors enabled via --config, T-OR-113), and
# prints a per-detector FP table with a real per-message denominator. The
# offline path is used rather than the daemon's live NDJSON because the daemon
# only logs skipped/firing messages, so its NDJSON has no denominator.
#
# Captures hold real traffic and must never be committed. This script refuses a
# <captures-dir> that resolves inside the repo unless git ignores it.
#
# Usage: scripts/calib-run.sh [<captures-dir>] [--ip <addr>]
#   <captures-dir> default: ~/orbic-captures
set -euo pipefail
# Resolve physically (pwd -P) so the repo-containment check below is not fooled
# by a symlinked repo path.
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"

usage() { sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }

cap="${HOME}/orbic-captures"
ip_args=()
while [ $# -gt 0 ]; do
    case "$1" in
        -h|--help) usage 0 ;;
        --ip)      ip_args+=(--ip "$2"); shift 2 ;;
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

report="$cap/calib-report.json"

echo "[1/3] pull-logs -> $cap"
(cd "$root" && cargo run -q -p orx-cli -- pull-logs --to "$cap" ${ip_args[@]+"${ip_args[@]}"}) 2>&1 | tail -1

echo "[2/3] rayhunter-check (detectors enabled via --config)"
(cd "$root/build/rayhunter" && cargo run -q -p rayhunter-check -- \
    -p "$cap" --config "$root/scripts/calib-analyzers.toml" -j "$report" -q) 2>&1 | tail -1

echo "[3/3] snapshot"
python3 "$root/scripts/calib-snapshot.py" "$report"
