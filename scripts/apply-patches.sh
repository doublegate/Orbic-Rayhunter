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
# Both paths are made absolute and symlink-free up front: git ignores RELATIVE
# GIT_CEILING_DIRECTORIES entries (and compares against a canonical getcwd), and
# patch paths must still resolve after the `cd "$scratch"` below.
dest_arg="${1:-$root/build/rayhunter}"
mkdir -p "$(dirname "$dest_arg")"
dest="$(cd "$(dirname "$dest_arg")" && pwd -P)/$(basename "$dest_arg")"
patches_arg="${PATCHES_DIR:-$root/patches}"   # override only for tests/apply-patches.sh
patches_dir="$(cd "$patches_arg" 2>/dev/null && pwd -P)" \
    || { echo "error: patch directory not found: $patches_arg" >&2; exit 1; }
series="$patches_dir/series"

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
    patch_file="$patches_dir/$line"
    if [[ ! -f "$patch_file" ]]; then
        echo "error: series lists missing patch: $line" >&2
        exit 1
    fi
    echo "applying $line"
    # git apply must act as plain patch(1) on $scratch. Without a ceiling, git
    # discovers an enclosing repository (this one, when $scratch is under build/)
    # and then IGNORES every patched path outside the current subdirectory while
    # still exiting 0, so the whole series would be skipped silently.
    # GIT_CEILING_DIRECTORIES stops discovery above $scratch.
    (cd "$scratch" \
        && GIT_CEILING_DIRECTORIES="$(dirname "$scratch")" git apply --check --whitespace=nowarn "$patch_file" \
        && GIT_CEILING_DIRECTORIES="$(dirname "$scratch")" git apply --whitespace=nowarn "$patch_file")
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
