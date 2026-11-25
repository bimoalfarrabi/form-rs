#!/usr/bin/env bash
# Build Form.app and a DMG on macOS, signing the bundle ad-hoc.
#
# cargo-packager cannot do this: it always passes `codesign --timestamp`, which
# ad-hoc identities do not have. Everything here is stock macOS tooling.
#
#   packaging/macos-bundle.sh <version> <arch> [dist-dir]
#
# Expects `cargo build --profile dist -p form-app --bin form` to have run.

set -euo pipefail

VERSION="${1:?version, e.g. 0.1.5}"
ARCH="${2:?arch label, e.g. aarch64 or x64}"
DIST="${3:-target/dist}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

BIN="$DIST/form"
APP="$DIST/Form.app"
DMG="$DIST/Form_${VERSION}_${ARCH}.dmg"

[ -x "$BIN" ] || { echo "missing $BIN (build it first)"; exit 1; }

rm -rf "$APP" "$DMG"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cp "$BIN" "$APP/Contents/MacOS/form"
cp packaging/icons/icon.icns "$APP/Contents/Resources/icon.icns"
cp LICENSE-MIT LICENSE-APACHE NOTICE "$APP/Contents/Resources/"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Form</string>
  <key>CFBundleDisplayName</key><string>Form</string>
  <key>CFBundleIdentifier</key><string>dev.bimoalfarrabi.form</string>
  <key>CFBundleExecutable</key><string>form</string>
  <key>CFBundleIconFile</key><string>icon.icns</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.graphics-design</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict>
</plist>
PLIST

# Ad-hoc signature: no certificate, no timestamp, no hardened runtime.
codesign --force --deep --sign - "$APP"
codesign --verify --verbose=2 "$APP"

hdiutil create -volname "Form" -srcfolder "$APP" -ov -format UDZO -fs HFS+ "$DMG" >/dev/null
echo "wrote $DMG"
