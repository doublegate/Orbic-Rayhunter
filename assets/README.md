# assets/

Project artwork. Original work for this project; it contains no EFF, Orbic, Verizon or Qualcomm
marks and makes no performance claims.

## Provenance

| Source | Produced by |
|---|---|
| `src/icon-master.png`, `src/logo-full.png`, `src/banner-hero.png`, `src/social-preview.png` | generated with Antigravity (`agy`) image generation, 2026-11-22, reviewed by hand |
| `../docs/images/*.svg` | hand-written SVG authored by `agy` from a fixed fact sheet, reviewed by hand |
| everything else under `assets/`, and `../docs/images/*.png` | derived by `scripts/gen-assets.sh` |

Do not edit derived files: replace a master in `src/` (or an SVG) and re-run
`scripts/gen-assets.sh` (needs ImageMagick 7 and `rsvg-convert`).

## Layout

| Path | Contents |
|---|---|
| `icons/png/` | opaque square icon, 16 to 1024 px |
| `icons/transparent/` | icon with the background removed, 16 to 512 px |
| `icons/ico/` | multi-resolution `.ico` (full and small) |
| `web/` | favicons, apple-touch-icon, android-chrome icons |
| `branding/` | hero banners (1500x500, 1200x400, 900x300, full), logo, GitHub social preview (1280x640) |
| `linux/` | 512 px desktop icon |

The GitHub social preview (`branding/social-preview-1280x640.png`) has to be uploaded by hand in
the repository settings; the API does not accept it.

Known inconsistency: the icon drawn inside `src/logo-full.png` and `src/social-preview.png` is a
variant of, not identical to, `src/icon-master.png`.
