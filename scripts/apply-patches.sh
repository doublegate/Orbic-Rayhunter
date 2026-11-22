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

rm -rf "$dest"
mkdir -p "$(dirname "$dest")"
# Export the committed submodule tree (no untracked files, no .git).
git -C "$root/upstream" archive --format=tar HEAD | { mkdir -p "$dest" && tar -x -C "$dest"; }

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
