#!/bin/sh
# Builds a macOS app bundle with the Lorimer icon.
#
#   scripts/bundle-macos.sh                    build, then write target/release/Lorimer.app
#   scripts/bundle-macos.sh <binary> <app>     wrap an existing binary, as the release workflow does
set -eu

cd "$(dirname "$0")/.."

if [ "$#" -eq 2 ]; then
    binary=$1
    app=$2
elif [ "$#" -eq 0 ]; then
    cargo build --release -p lorimer
    binary=target/release/lorimer
    app=target/release/Lorimer.app
else
    echo "usage: $0 [<binary> <app>]" >&2
    exit 2
fi

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/lorimer"
cp assets/icons/lorimer.icns "$app/Contents/Resources/lorimer.icns"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>Lorimer</string>
    <key>CFBundleDisplayName</key>
    <string>Lorimer</string>
    <key>CFBundleIdentifier</key>
    <string>io.github.asweeney86.lorimer</string>
    <key>CFBundleExecutable</key>
    <string>lorimer</string>
    <key>CFBundleIconFile</key>
    <string>lorimer</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$version</string>
    <key>CFBundleVersion</key>
    <string>$version</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.utilities</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
PLIST

echo "Built $app"
