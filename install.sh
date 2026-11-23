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
  rm -f "$BIN/cosmic-doro" "$BIN/cosmic-doro-applet" \
        "$APPS/$ID.desktop" "$APPS/$ID.Applet.desktop" \
        "$ICONS/scalable/apps/$ID.svg" "$ICONS/symbolic/apps/$ID-symbolic.svg"
  echo "Uninstalled. History remains in ~/.local/share/cosmic-doro"
  exit 0
fi

cargo build --release
install -Dm755 target/release/cosmic-doro        "$BIN/cosmic-doro"
install -Dm755 target/release/cosmic-doro-applet "$BIN/cosmic-doro-applet"
install -Dm644 res/icons/$ID.svg                 "$ICONS/scalable/apps/$ID.svg"
install -Dm644 res/icons/$ID-symbolic.svg        "$ICONS/symbolic/apps/$ID-symbolic.svg"
# Absolute Exec paths: cosmic-panel's PATH may not include ~/.local/bin.
mkdir -p "$APPS"
sed "s|^Exec=cosmic-doro$|Exec=$BIN/cosmic-doro|" res/desktop/$ID.desktop > "$APPS/$ID.desktop"
sed "s|^Exec=cosmic-doro-applet$|Exec=$BIN/cosmic-doro-applet|" res/desktop/$ID.Applet.desktop > "$APPS/$ID.Applet.desktop"
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -qtf "$ICONS" || true
command -v update-desktop-database >/dev/null && update-desktop-database -q "$APPS" || true

echo
echo "Installed Cosmic Doro."
echo "  • Launch it from the app library, or run: cosmic-doro"
echo "  • Panel applet: Settings → Desktop → Panel (or Dock) → Configure applets → Add 'Cosmic Doro'"
