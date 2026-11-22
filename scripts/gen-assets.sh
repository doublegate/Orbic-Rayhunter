#!/usr/bin/env bash
# Derive every icon, favicon, banner size and SVG->PNG render from the master
# artwork in assets/src/ (generated with agy; see assets/README.md).
# Deterministic and idempotent: re-run after replacing a master.
# Requires: ImageMagick 7 (magick), rsvg-convert.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
src="$root/assets/src"
a="$root/assets"

for tool in magick rsvg-convert; do
    command -v "$tool" >/dev/null || { echo "error: $tool not found" >&2; exit 1; }
done
for f in icon-master.png logo-full.png banner-hero.png social-preview.png; do
    [[ -f "$src/$f" ]] || { echo "error: missing master $src/$f" >&2; exit 1; }
done

mkdir -p "$a"/icons/{png,transparent,ico,svg} "$a"/web "$a"/branding "$a"/linux

# Square master, normalized to 1024x1024.
master="$(mktemp --suffix=.png)"
trans="$(mktemp --suffix=.png)"
trap 'rm -f "$master" "$trans"' EXIT
magick "$src/icon-master.png" -strip -resize 1024x1024^ -gravity center -extent 1024x1024 "$master"
# Transparent variant: flood-fill the background from the four corners.
magick "$master" -alpha set -fuzz 12% -fill none \
    -draw 'color 0,0 floodfill' -draw 'color 1023,0 floodfill' \
    -draw 'color 0,1023 floodfill' -draw 'color 1023,1023 floodfill' "$trans"

for s in 16 24 32 48 64 128 256 512 1024; do
    magick "$master" -strip -resize "${s}x${s}" "$a/icons/png/icon-$s.png"
done
for s in 16 32 64 128 256 512; do
    magick "$trans" -strip -resize "${s}x${s}" "$a/icons/transparent/orx-icon-transparent-$s.png"
done
magick "$trans" -strip -define icon:auto-resize=256,128,64,48,32,24,16 "$a/icons/ico/orbic-rayhunter.ico"
magick "$trans" -strip -define icon:auto-resize=32,24,16 "$a/icons/ico/orbic-rayhunter-small.ico"
cp "$a/icons/png/icon-512.png" "$a/linux/orbic-rayhunter.png"

# Web favicons.
for s in 16 32 96; do magick "$trans" -strip -resize "${s}x${s}" "$a/web/favicon-${s}x${s}.png"; done
magick "$master" -strip -resize 180x180 "$a/web/apple-touch-icon.png"
for s in 192 512; do magick "$master" -strip -resize "${s}x${s}" "$a/web/android-chrome-${s}x${s}.png"; done
cp "$a/icons/ico/orbic-rayhunter-small.ico" "$a/web/favicon.ico"

# Banners (cover-crop to exact aspect) and social card.
magick "$src/banner-hero.png" -strip "$a/branding/banner-full.png"
for wh in 1500x500 1200x400 900x300; do
    magick "$src/banner-hero.png" -strip -resize "${wh}^" -gravity center -extent "$wh" "$a/branding/banner-$wh.png"
done
magick "$src/social-preview.png" -strip -resize 1280x640^ -gravity center -extent 1280x640 "$a/branding/social-preview-1280x640.png"
magick "$src/logo-full.png" -strip -resize 'x600>' "$a/branding/logo-full.png"

# SVG diagrams -> PNG fallbacks (2x for crisp display).
shopt -s nullglob
for svg in "$root"/docs/images/*.svg; do
    rsvg-convert --zoom=2 "$svg" -o "${svg%.svg}.png"
done

echo "assets regenerated under $a and docs/images/"
