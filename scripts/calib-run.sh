#!/usr/bin/env bash
# T-OR-061 one-command calibration false-positive snapshot.
#
# Pulls recordings from the device (QMDL + the daemon's analysis NDJSON) and
# prints a per-detector FP table from the daemon's own analysis -- so it
# reflects exactly what ran on-device, with no rebuild. The three v1.2.0
# detectors must be enabled on the device (see docs/ANALYZERS.md, "Field
# calibration"); recordings captured before they were enabled are reported as
# "not covered".
#
# Captures hold real traffic and must never be committed -- keep <captures-dir>
# outside the repo (e.g. ~/orbic-captures) or in a gitignored path.
#
# Usage: scripts/calib-run.sh [<captures-dir>] [--ip <addr>]
#   <captures-dir> default: ~/orbic-captures
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cap="${HOME}/orbic-captures"
ip_args=()
while [ $# -gt 0 ]; do
    case "$1" in
        --ip) ip_args+=(--ip "$2"); shift 2 ;;
        -*)   echo "unknown flag: $1" >&2; exit 2 ;;
        *)    cap="$1"; shift ;;
    esac
done
mkdir -p "$cap"

echo "[1/2] pull-logs -> $cap"
(cd "$root" && cargo run -q -p orx-cli -- pull-logs --to "$cap" "${ip_args[@]}") 2>&1 | tail -1

echo "[2/2] snapshot"
python3 "$root/scripts/calib-snapshot.py" "$cap"
