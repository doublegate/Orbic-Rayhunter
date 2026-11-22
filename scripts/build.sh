#!/usr/bin/env bash
# Build the patched Rayhunter daemon (and rootshell) for the Orbic (armv7 musl).
#
# Usage: scripts/build.sh [devel|firmware]
#   devel    (default) upstream "firmware-devel" profile; needs only rustup + npm
#   firmware upstream "firmware" profile with pq-tls; needs an
#            arm-linux-musleabihf cross toolchain (see docs/BUILD.md)
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

case "$profile" in
    devel)
        cargo build-daemon-firmware-devel
        cargo build-rootshell-firmware-devel
        out="target/$target/firmware-devel"
        ;;
    firmware)
        cargo build-daemon-firmware
        cargo build-rootshell-firmware
        out="target/$target/firmware"
        ;;
    *)
        echo "error: unknown profile '$profile' (expected devel|firmware)" >&2
        exit 2
        ;;
esac

mkdir -p "$root/dist"
cp "$out/rayhunter-daemon" "$out/rootshell" "$root/dist/"
(cd "$root/dist" && sha256sum rayhunter-daemon rootshell > SHA256SUMS)
echo "artifacts in $root/dist:"
cat "$root/dist/SHA256SUMS"
