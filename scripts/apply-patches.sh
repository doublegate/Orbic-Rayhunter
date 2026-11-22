#!/usr/bin/env bash
# Apply patches/series onto a clean copy of the pinned upstream tree.
#
# The submodule itself is never modified: the patched tree is staged in
# build/rayhunter (or $1). Each line of patches/series names a patch file;
# blank lines and lines starting with '#' are ignored.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dest="${1:-$root/build/rayhunter}"
series="$root/patches/series"

if [[ ! -e "$root/upstream/.git" ]]; then
    echo "error: upstream submodule missing; run: git submodule update --init" >&2
    exit 1
fi

# Build outputs survive re-staging so incremental and CI-cached builds stay
# warm: cargo's target/ and npm's node_modules/ are moved aside and restored.
# Everything else in $dest is replaced, so sources always match upstream +
# patches exactly. Cargo rebuilds whatever the new sources invalidate.
keep=(target node_modules)
stash="$(dirname "$dest")/.keep-$(basename "$dest")"
rm -rf "$stash"
if [[ -d "$dest" ]]; then
    mkdir -p "$stash"
    for d in "${keep[@]}"; do
        [[ -e "$dest/$d" ]] && mv "$dest/$d" "$stash/$d"
    done
fi
rm -rf "$dest"
mkdir -p "$dest"
# Export the committed submodule tree (no untracked files, no .git).
git -C "$root/upstream" archive --format=tar HEAD | tar -x -C "$dest"
if [[ -d "$stash" ]]; then
    for d in "${keep[@]}"; do
        [[ -e "$stash/$d" ]] && mv "$stash/$d" "$dest/$d"
    done
    rm -rf "$stash"
fi

count=0
while IFS= read -r line || [[ -n "$line" ]]; do
    line="${line%%#*}"
    line="$(echo "$line" | xargs)"
    [[ -z "$line" ]] && continue
    patch_file="$root/patches/$line"
    if [[ ! -f "$patch_file" ]]; then
        echo "error: series lists missing patch: $line" >&2
        exit 1
    fi
    echo "applying $line"
    # $dest is a plain directory (not a repo), so git apply acts as patch(1).
    (cd "$dest" && git apply --check --whitespace=nowarn "$patch_file" \
        && git apply --whitespace=nowarn "$patch_file")
    count=$((count + 1))
done < "$series"

echo "upstream $(git -C "$root/upstream" describe --tags --always) + $count patch(es) -> $dest"
