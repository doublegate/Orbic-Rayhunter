#!/usr/bin/env bash
# Report (and optionally trial) a move of the upstream submodule to a newer tag.
#
# Usage: scripts/sync-upstream.sh            # list pinned vs latest tag
#        scripts/sync-upstream.sh <tag>      # check out <tag> in upstream/ and
#                                            # test that patches/series applies
# Never commits. Review `git diff --submodule` and commit deliberately.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
up="$root/upstream"

git -C "$up" fetch --tags --quiet origin
pinned="$(git -C "$up" describe --tags --exact-match 2>/dev/null || git -C "$up" rev-parse --short HEAD)"
latest="$(git -C "$up" tag --list 'v*' --sort=-v:refname | head -n1)"
echo "pinned: $pinned"
echo "latest: $latest"

if [[ $# -eq 0 ]]; then
    [[ "$pinned" == "$latest" ]] && echo "up to date" || echo "update available: scripts/sync-upstream.sh $latest"
    exit 0
fi

tag="$1"
if [[ -n "$(git -C "$up" status --porcelain)" ]]; then
    echo "error: upstream/ has local changes; refusing to switch tags" >&2
    exit 1
fi
git -C "$up" checkout --quiet "$tag"
if "$root/scripts/apply-patches.sh" "$root/build/rayhunter-trial"; then
    echo "OK: patches apply cleanly on $tag"
else
    echo "FAIL: patches do not apply on $tag; rebase them (docs/PATCH-POLICY.md)" >&2
    exit 1
fi
