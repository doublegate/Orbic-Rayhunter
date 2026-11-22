#!/usr/bin/env bash
# Apply patches/series onto a clean copy of the pinned upstream tree.
#
# The submodule itself is never modified: the patched tree is staged in
# build/rayhunter (or $1). Each line of patches/series names a patch file;
# blank lines and lines starting with '#' are ignored.
#
# Staging is incremental so cached builds stay both warm and correct:
#   1. upstream (git archive of the committed submodule tree) + patches are
#      assembled in a scratch directory;
#   2. the result is synced into $dest by CONTENT (rsync --checksum), deleting
#      files that no longer exist, but never touching target/ or node_modules/.
# Only files whose content differs are rewritten, and rsync gives them the
# current time (no --times), so they are newer than Cargo's dep-info and get
# rebuilt. Files whose content is unchanged keep their previous mtime, which is
# correct because Cargo's fingerprint for them still describes this content.
# Setting every file to the archive's commit time instead would let a source
# that a patch change reverted look older than its dep-info, and Cargo would
# reuse a stale patched artifact.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dest="${1:-$root/build/rayhunter}"
series="$root/patches/series"

if [[ ! -e "$root/upstream/.git" ]]; then
    echo "error: upstream submodule missing; run: git submodule update --init" >&2
    exit 1
fi
command -v rsync >/dev/null || { echo "error: rsync is required" >&2; exit 1; }

mkdir -p "$(dirname "$dest")"
scratch="$(mktemp -d "$(dirname "$dest")/.stage-XXXXXX")"
trap 'rm -rf "$scratch"' EXIT

# Export the committed submodule tree (no untracked files, no .git).
git -C "$root/upstream" archive --format=tar HEAD | tar -x -C "$scratch"

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
    # $scratch is a plain directory (not a repo), so git apply acts as patch(1).
    (cd "$scratch" && git apply --check --whitespace=nowarn "$patch_file" \
        && git apply --whitespace=nowarn "$patch_file")
    count=$((count + 1))
done < "$series"

mkdir -p "$dest"
# -r recursive, -l symlinks as links, -p permissions (exec bits); deliberately no
# -t, so rewritten files get a fresh mtime. Excluded build outputs are neither
# compared nor deleted.
rsync -rlp --checksum --delete \
    --exclude=/target --exclude=/node_modules \
    "$scratch/" "$dest/"

echo "upstream $(git -C "$root/upstream" describe --tags --always) + $count patch(es) -> $dest"
