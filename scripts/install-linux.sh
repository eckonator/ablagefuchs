#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Installiert Ablagefuchs aus dem entpackten Linux-Archiv für den aktuellen Benutzer:
# Programme samt libpdfium.so nach ~/.local/lib/ablagefuchs, Verknüpfungen in ~/.local/bin,
# Startmenü-Eintrag und Icon nach ~/.local/share. Danach steht Ablagefuchs mit Icon im Anwendungsmenü.
#   ./install.sh              installieren (erneut ausführen = aktualisieren)
#   ./install.sh --uninstall  wieder entfernen (Einstellungen bleiben erhalten)
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
LIB="$HOME/.local/lib/ablagefuchs"
BIN="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
APPS="$DATA/applications"
ICONS="$DATA/icons/hicolor"

refresh() {
  command -v update-desktop-database >/dev/null && update-desktop-database -q "$APPS" || true
  command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$ICONS" || true
}

if [ "${1:-}" = "--uninstall" ]; then
  rm -rf "$LIB"
  rm -f "$BIN/ablagefuchs" "$BIN/ablagefuchs-cli" "$APPS/ablagefuchs.desktop" "$ICONS/256x256/apps/ablagefuchs.png"
  refresh
  echo "Ablagefuchs entfernt."
  exit 0
fi

mkdir -p "$LIB" "$BIN" "$APPS" "$ICONS/256x256/apps"
# pdfium wird neben dem Programm gesucht (current_exe folgt den Verknüpfungen in ~/.local/bin)
install -m755 "$HERE/ablagefuchs" "$HERE/ablagefuchs-cli" "$LIB/"
install -m644 "$HERE/libpdfium.so" "$LIB/"
ln -sf "$LIB/ablagefuchs" "$BIN/ablagefuchs"
ln -sf "$LIB/ablagefuchs-cli" "$BIN/ablagefuchs-cli"
install -m644 "$HERE/ablagefuchs.png" "$ICONS/256x256/apps/ablagefuchs.png"
# Exec mit vollem Pfad: ~/.local/bin steht nicht bei jeder Desktop-Umgebung im Suchpfad
sed "s|^Exec=ablagefuchs|Exec=\"$LIB/ablagefuchs\"|" "$HERE/ablagefuchs.desktop" > "$APPS/ablagefuchs.desktop"
chmod 644 "$APPS/ablagefuchs.desktop"
refresh

echo "Ablagefuchs installiert: $LIB"
echo "Start über das Anwendungsmenü oder mit: ablagefuchs"
