#!/usr/bin/env bash
# Build the patched Rayhunter daemon (and rootshell) for the Orbic (armv7 musl).
#
# Usage: scripts/build.sh [devel|firmware]
#   devel    (default) "firmware-devel" profile, rustcrypto TLS backend. Pure Rust,
#            needs only rustup + npm. Fast to iterate; larger, unstripped binary.
#   firmware production "firmware" profile with post-quantum TLS, matching the
#            upstream release build. Built inside the messense/rust-musl-cross
#            container (the same one upstream uses) for a reproducible result;
#            needs docker (or podman) + npm. See docs/BUILD.md.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
profile="${1:-devel}"
tree="$root/build/rayhunter"
target=armv7-unknown-linux-musleabihf

"$root/scripts/apply-patches.sh" "$tree"

cd "$tree"
rustup toolchain install   # honours the upstream rust-toolchain.toml
rustup target add "$target"

npm install
npm run build -w daemon/web

mkdir -p "$root/dist"

case "$profile" in
    devel)
        cargo build-daemon-firmware-devel
        cargo build-rootshell-firmware-devel
        out="target/$target/firmware-devel"
        cp "$out/rayhunter-daemon" "$out/rootshell" "$root/dist/"
        ;;
    firmware)
        # Reproducible production build in the same cross container upstream uses
        # (post-quantum TLS needs an arm-linux-musleabihf C toolchain). The web UI
        # is already built on the host above; the container only runs cargo. The
        # target dir is a scratch path OUTSIDE the tree so the container's
        # root-owned artifacts never land in build/rayhunter.
        engine="$(command -v docker || command -v podman || true)"
        if [ -z "$engine" ]; then
            echo "error: firmware profile needs docker or podman (see docs/BUILD.md)" >&2
            exit 3
        fi
        # Pin the build image by digest so the production build is reproducible and
        # not silently re-tagged upstream (override with ORX_MUSL_CROSS_IMAGE). Update
        # the digest deliberately and record it; see re/notes/reproducible-build.md.
        image="${ORX_MUSL_CROSS_IMAGE:-messense/rust-musl-cross:armv7-musleabihf@sha256:dd494898259cf6981f9832bfd4437ddfc8393605164b3c242e4efe8a6bc30dfb}"
        tgt="$(mktemp -d "${TMPDIR:-/tmp}/orx-firmware.XXXXXX")"
        "$engine" run --rm \
            -v "$tree":/work -v "$tgt":/target -e CARGO_TARGET_DIR=/target -w /work \
            "$image" \
            bash -lc 'rustup target add armv7-unknown-linux-musleabihf >/dev/null 2>&1; cargo build-daemon-firmware && cargo build-rootshell-firmware'
        out="$tgt/$target/firmware"
        cp "$out/rayhunter-daemon" "$out/rootshell" "$root/dist/"
        ;;
    *)
        echo "error: unknown profile '$profile' (expected devel|firmware)" >&2
        exit 2
        ;;
esac

(cd "$root/dist" && sha256sum rayhunter-daemon rootshell > SHA256SUMS)
echo "artifacts in $root/dist:"
cat "$root/dist/SHA256SUMS"
