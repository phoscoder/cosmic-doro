#!/usr/bin/env bash
# Builds and installs Cosmic Doro (app + panel applet) for the current user.
# Usage: ./install.sh            install to ~/.local
#        ./install.sh uninstall  remove it again (your history is kept)
set -euo pipefail
cd "$(dirname "$0")"

PREFIX="${PREFIX:-$HOME/.local}"
BIN="$PREFIX/bin"
APPS="$PREFIX/share/applications"
ICONS="$PREFIX/share/icons/hicolor"
ID=io.github.phoscoder.CosmicDoro

if [[ "${1:-}" == "uninstall" ]]; then
  pkill -f "$BIN/cosmic-doro-tray" 2>/dev/null || true
  rm -f "$BIN/cosmic-doro" "$BIN/cosmic-doro-applet" "$BIN/cosmic-doro-tray" \
        "${XDG_CONFIG_HOME:-$HOME/.config}/autostart/$ID.Tray.desktop" \
        "$APPS/$ID.desktop" "$APPS/$ID.Applet.desktop" \
        "$ICONS/scalable/apps/$ID.svg" "$ICONS/symbolic/apps/$ID-symbolic.svg"
  echo "Uninstalled. History remains in ~/.local/share/cosmic-doro"
  exit 0
fi

cargo build --release
install -Dm755 target/release/cosmic-doro        "$BIN/cosmic-doro"
install -Dm755 target/release/cosmic-doro-applet "$BIN/cosmic-doro-applet"
install -Dm755 target/release/cosmic-doro-tray   "$BIN/cosmic-doro-tray"
install -Dm644 res/icons/$ID.svg                 "$ICONS/scalable/apps/$ID.svg"
install -Dm644 res/icons/$ID-symbolic.svg        "$ICONS/symbolic/apps/$ID-symbolic.svg"
# Absolute Exec paths: cosmic-panel's PATH may not include ~/.local/bin.
mkdir -p "$APPS"
sed "s|^Exec=cosmic-doro$|Exec=$BIN/cosmic-doro|" res/desktop/$ID.desktop > "$APPS/$ID.desktop"
sed "s|^Exec=cosmic-doro-applet$|Exec=$BIN/cosmic-doro-applet|" res/desktop/$ID.Applet.desktop > "$APPS/$ID.Applet.desktop"
# Restart the tray so the new binary takes over (the app relaunches it).
pkill -f "$BIN/cosmic-doro-tray" 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -qtf "$ICONS" || true
command -v update-desktop-database >/dev/null && update-desktop-database -q "$APPS" || true

echo
echo "Installed Cosmic Doro."
echo "  • Launch it from the app library, or run: cosmic-doro"
echo "  • Panel applet: Settings → Desktop → Panel (or Dock) → Configure applets → Add 'Cosmic Doro'"
