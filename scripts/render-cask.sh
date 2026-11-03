#!/bin/sh
# Prints the Homebrew cask for a release, filled in from packaging/homebrew/lorimer.rb.in.
#
#   scripts/render-cask.sh <version> <zip>     version without the leading v, e.g. 0.1.1
set -eu

if [ "$#" -ne 2 ]; then
    echo "usage: $0 <version> <zip>" >&2
    exit 2
fi

version=${1#v}
zip=$2
template="$(dirname "$0")/../packaging/homebrew/lorimer.rb.in"

case $version in
    "" | *[!0-9A-Za-z.-]*)
        echo "invalid version: $version" >&2
        exit 2
        ;;
esac

if command -v shasum >/dev/null 2>&1; then
    sha=$(shasum -a 256 "$zip" | cut -d ' ' -f 1)
else
    sha=$(sha256sum "$zip" | cut -d ' ' -f 1)
fi

sed -e "s/@VERSION@/$version/" -e "s/@SHA256@/$sha/" "$template"
