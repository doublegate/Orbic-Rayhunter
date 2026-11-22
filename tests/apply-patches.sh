#!/usr/bin/env bash
# Regression tests for scripts/apply-patches.sh.
#
# Uses a throwaway patch directory (PATCHES_DIR) and destination, so the real
# patches/series and build/rayhunter are never touched. Requires the upstream
# submodule to be checked out. Exit 0 = all tests passed.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
script="$root/scripts/apply-patches.sh"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
fail=0

ok()   { echo "ok    $1"; }
bad()  { echo "FAIL  $1"; fail=1; }

# A patch against a file that exists in every upstream revision (README.md),
# generated with diff -u from the pinned upstream content so it always applies.
mkdir -p "$work/patches" "$work/orig/a" "$work/orig/b"
git -C "$root/upstream" show HEAD:README.md > "$work/orig/a/README.md"
{ head -n1 "$work/orig/a/README.md"; echo "ORX-APPLY-PATCHES-TEST-MARKER"; tail -n +2 "$work/orig/a/README.md"; } \
    > "$work/orig/b/README.md"
{
    printf 'From: test <test@example.invalid>\nSubject: [LOCAL] test marker\n\n---\n'
    printf 'diff --git a/README.md b/README.md\n'
    (cd "$work/orig" && diff -u a/README.md b/README.md || true)
} > "$work/patches/0001-test-marker.patch"
printf '0001-test-marker.patch\n' > "$work/patches/series"

# 1. Destination INSIDE this repository's working tree (the default layout):
#    git apply must not discover the superproject's .git and skip the patch.
dest_in="$root/build/.apply-patches-test-$$"
trap 'rm -rf "$work" "$dest_in"' EXIT
PATCHES_DIR="$work/patches" "$script" "$dest_in" >/dev/null
if grep -q 'ORX-APPLY-PATCHES-TEST-MARKER' "$dest_in/README.md"; then
    ok "patch applied when the destination is inside the repository"
else
    bad "patch silently skipped when the destination is inside the repository"
fi

# 2. Destination outside the repository.
dest_out="$work/out"
PATCHES_DIR="$work/patches" "$script" "$dest_out" >/dev/null
if grep -q 'ORX-APPLY-PATCHES-TEST-MARKER' "$dest_out/README.md"; then
    ok "patch applied when the destination is outside the repository"
else
    bad "patch not applied when the destination is outside the repository"
fi

# 3. Removing the patch restores upstream content with a fresh mtime, and keeps
#    retained build outputs.
mkdir -p "$dest_out/target/x" && echo keep > "$dest_out/target/x/m"
touch -d '2001-01-01' "$dest_out/README.md"
: > "$work/patches/series"
PATCHES_DIR="$work/patches" "$script" "$dest_out" >/dev/null
if git -C "$root/upstream" show HEAD:README.md | cmp -s - "$dest_out/README.md"; then
    ok "removing a patch restores upstream content"
else
    bad "removing a patch did not restore upstream content"
fi
if [ "$(stat -c %Y "$dest_out/README.md")" -gt "$(date -d '2002-01-01' +%s)" ]; then
    ok "restored file gets a fresh mtime (Cargo sees it as changed)"
else
    bad "restored file kept a stale mtime"
fi
if [ -f "$dest_out/target/x/m" ]; then ok "target/ survives restaging"; else bad "target/ was removed"; fi

# 4. A series entry naming a missing patch fails loudly.
printf 'missing.patch\n' > "$work/patches/series"
if PATCHES_DIR="$work/patches" "$script" "$work/out2" >/dev/null 2>&1; then
    bad "missing patch did not fail"
else
    ok "missing patch fails"
fi

# 5. A patch that does not apply fails loudly (never a silent skip).
sed 's/^ \(.*\)$/ NOT-THE-REAL-CONTEXT/' "$work/patches/0001-test-marker.patch" > "$work/patches/0002-bad.patch"
printf '0002-bad.patch\n' > "$work/patches/series"
if PATCHES_DIR="$work/patches" "$script" "$dest_in" >/dev/null 2>&1; then
    bad "non-applying patch did not fail"
else
    ok "non-applying patch fails"
fi

[ "$fail" -eq 0 ] && echo "all apply-patches tests passed"
exit "$fail"
