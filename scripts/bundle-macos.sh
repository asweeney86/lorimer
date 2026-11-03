#!/bin/sh
# Builds a macOS app bundle with the Lorimer icon.
#
#   scripts/bundle-macos.sh                          build, then write target/release/Lorimer.app
#   scripts/bundle-macos.sh <binary> <app> [<cli>]   wrap existing binaries, as the release workflow does
#
# The command-line tool goes in Contents/MacOS next to the app binary. The Homebrew cask
# links it onto PATH from there.
set -eu

cd "$(dirname "$0")/.."

cli=
if [ "$#" -eq 2 ] || [ "$#" -eq 3 ]; then
    binary=$1
    app=$2
    cli=${3:-}
elif [ "$#" -eq 0 ]; then
    cargo build --release -p lorimer -p lorimer-cli
    binary=target/release/lorimer
    cli=target/release/lorimer-cli
    app=target/release/Lorimer.app
else
    echo "usage: $0 [<binary> <app> [<cli>]]" >&2
    exit 2
fi

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$app/Contents/MacOS/lorimer"
if [ -n "$cli" ]; then
    cp "$cli" "$app/Contents/MacOS/lorimer-cli"
fi
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
