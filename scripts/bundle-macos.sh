#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Baut „Ablagefuchs.app“ für macOS nach dist/, mit pdfium in Contents/Frameworks.
# Aufruf: scripts/bundle-macos.sh   (AF_SKIP_BUILD=1 überspringt das Bauen, z. B. für das Universal-Binary der CI)
# pdfium vorher holen: cargo xtask fetch-pdfium mac-arm64 mac-x64 (beide → Universal-Bibliothek)
set -euo pipefail

cd "$(dirname "$0")/.."
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
APP="dist/Ablagefuchs.app"

if [ -z "${AF_SKIP_BUILD:-}" ]; then
  cargo build --release -p ablagefuchs
fi

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$APP/Contents/Frameworks"
cp target/release/ablagefuchs "$APP/Contents/MacOS/Ablagefuchs"
cp assets/Ablagefuchs.icns "$APP/Contents/Resources/Ablagefuchs.icns"

# pdfium: aus beiden Architekturen eine Universal-Bibliothek, sonst die vorhandene
ARM=vendor/pdfium/mac-arm64/lib/libpdfium.dylib
X64=vendor/pdfium/mac-x64/lib/libpdfium.dylib
if [ -f "$ARM" ] && [ -f "$X64" ]; then
  lipo -create -output "$APP/Contents/Frameworks/libpdfium.dylib" "$ARM" "$X64"
elif [ -f "$ARM" ]; then
  cp "$ARM" "$APP/Contents/Frameworks/"
elif [ -f "$X64" ]; then
  cp "$X64" "$APP/Contents/Frameworks/"
else
  echo "pdfium fehlt: cargo xtask fetch-pdfium mac-arm64 mac-x64" >&2
  exit 1
fi

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Ablagefuchs</string>
  <key>CFBundleDisplayName</key><string>Ablagefuchs</string>
  <key>CFBundleIdentifier</key><string>de.eckonator.ablagefuchs</string>
  <key>CFBundleExecutable</key><string>Ablagefuchs</string>
  <key>CFBundleIconFile</key><string>Ablagefuchs</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>NSHumanReadableCopyright</key><string>© 2026 Markus Eckert · MIT OR Apache-2.0</string>
  <key>CFBundleDevelopmentRegion</key><string>de</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

# Ad-hoc-Signatur (inklusive pdfium), damit Apple-Silicon-Macs die App ohne Entwicklerkonto starten
codesign --force --deep --sign - "$APP" >/dev/null
echo "Fertig: $APP"
