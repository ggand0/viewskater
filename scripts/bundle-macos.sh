#!/usr/bin/env bash
# Build the macOS .app bundle and merge extra Info.plist keys that
# cargo-bundle 0.6.1 ignores (CFBundleDocumentTypes in particular).
#
# Usage:
#   scripts/bundle-macos.sh                 # opt-dev profile
#   scripts/bundle-macos.sh release         # release profile

set -euo pipefail

PROFILE="${1:-opt-dev}"

cd "$(dirname "$0")/.."

TARGET_DIR="$(cargo metadata --format-version 1 --no-deps \
    | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')"

if [[ "$PROFILE" == "release" ]]; then
    cargo bundle --release
    BUNDLE_DIR="$TARGET_DIR/release/bundle/osx/ViewSkater.app"
else
    cargo bundle --profile "$PROFILE"
    BUNDLE_DIR="$TARGET_DIR/$PROFILE/bundle/osx/ViewSkater.app"
fi

GENERATED_PLIST="$BUNDLE_DIR/Contents/Info.plist"
EXTENSIONS_PLIST="resources/macos/Info.plist"

if [[ ! -f "$GENERATED_PLIST" ]]; then
    echo "error: expected bundle at $GENERATED_PLIST" >&2
    exit 1
fi

# Merge each top-level key from the extensions plist into the generated plist.
# PlistBuddy's Merge command does exactly this for dicts.
/usr/libexec/PlistBuddy -c "Merge $EXTENSIONS_PLIST" "$GENERATED_PLIST"

echo "Merged $EXTENSIONS_PLIST into $GENERATED_PLIST"
echo "Bundle ready: $BUNDLE_DIR"
