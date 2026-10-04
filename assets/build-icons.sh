#!/bin/sh
# Regenerates every raster icon from assets/logo/icon.svg.
# Requires rsvg-convert (librsvg) and magick (ImageMagick); iconutil (macOS) is used when present.
set -eu

cd "$(dirname "$0")"
source_svg=logo/icon.svg
png_dir=icons/png
mkdir -p "$png_dir"

for size in 16 24 32 48 64 128 256 512 1024; do
    rsvg-convert -w "$size" -h "$size" "$source_svg" -o "$png_dir/icon-$size.png"
done

# Raw RGBA pixels that the desktop app embeds as its window icon.
magick "$png_dir/icon-128.png" -depth 8 rgba:icons/window-icon-128.rgba

# Windows: one .ico holding the common sizes.
magick "$png_dir/icon-16.png" "$png_dir/icon-24.png" "$png_dir/icon-32.png" "$png_dir/icon-48.png" \
    "$png_dir/icon-64.png" "$png_dir/icon-128.png" "$png_dir/icon-256.png" icons/lorimer.ico

# macOS: .icns built from an iconset with 1x and 2x variants.
if command -v iconutil >/dev/null 2>&1; then
    iconset=$(mktemp -d)/lorimer.iconset
    mkdir -p "$iconset"
    for size in 16 32 128 256 512; do
        rsvg-convert -w "$size" -h "$size" "$source_svg" -o "$iconset/icon_${size}x${size}.png"
        double=$((size * 2))
        rsvg-convert -w "$double" -h "$double" "$source_svg" -o "$iconset/icon_${size}x${size}@2x.png"
    done
    iconutil -c icns "$iconset" -o icons/lorimer.icns
    rm -rf "$(dirname "$iconset")"
else
    echo "iconutil not found; skipped icons/lorimer.icns" >&2
fi

echo "Icons written to $(pwd)/icons"
