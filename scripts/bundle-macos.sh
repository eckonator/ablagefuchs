#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Baut „Ablagefuchs.app“ für macOS nach dist/, mit pdfium in Contents/Frameworks.
# Aufruf: scripts/bundle-macos.sh   (AF_SKIP_BUILD=1 überspringt das Bauen, z. B. für das Universal-Binary der CI)
# AF_SIGN_IDENTITY / AF_SIGN_KEYCHAIN: Zertifikat zum Signieren (Standard: ad-hoc), siehe scripts/signing-keychain.sh
# Liegt ein Ordner licenses/ vor (CI), kommen die Lizenztexte nach Contents/Resources.
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

if [ -d licenses ]; then
  cp licenses/* "$APP/Contents/Resources/"
fi

# Mit dem eigenen Zertifikat des Projekts signiert, wenn es eins gibt (scripts/signing-keychain.sh):
# Dann nimmt das Update per Knopf nur Versionen mit demselben Zertifikat an. Sonst ad-hoc, damit
# Apple-Silicon-Macs die App überhaupt starten. Erst pdfium, dann die App.
SIGN=(--force --sign "${AF_SIGN_IDENTITY:--}")
if [ -n "${AF_SIGN_KEYCHAIN:-}" ]; then
  SIGN+=(--keychain "$AF_SIGN_KEYCHAIN")
fi
codesign "${SIGN[@]}" "$APP/Contents/Frameworks/libpdfium.dylib" >/dev/null
codesign "${SIGN[@]}" --identifier de.eckonator.ablagefuchs "$APP" >/dev/null
codesign --verify --deep --strict "$APP"
echo "Fertig: $APP"
