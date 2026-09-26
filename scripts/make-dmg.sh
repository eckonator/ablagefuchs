#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Packt „dist/Ablagefuchs.app“ in ein DMG mit Installationsfenster (App nach „Programme“ ziehen).
# Aufruf: scripts/make-dmg.sh [Ziel.dmg]   (vorher scripts/bundle-macos.sh)
# Braucht dmgbuild (pip install dmgbuild); DMG_PYTHON wählt den Python-Interpreter dafür.
set -euo pipefail

cd "$(dirname "$0")/.."
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
OUT="${1:-dist/Ablagefuchs-$VERSION-macOS.dmg}"
APP="dist/Ablagefuchs.app"
[ -d "$APP" ] || { echo "Fehlt: $APP (erst scripts/bundle-macos.sh)" >&2; exit 1; }

rm -f "$OUT"
"${DMG_PYTHON:-python3}" -m dmgbuild -s scripts/dmg-settings.py \
  -D app="$PWD/$APP" -D root="$PWD" "Ablagefuchs $VERSION" "$OUT"
echo "Fertig: $OUT"
