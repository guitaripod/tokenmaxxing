#!/usr/bin/env bash
# Build tokenmaxxing and install the binary, hicolor icons, and .desktop entry
# into the user's ~/.local tree.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release
BIN="target/release/tokenmaxxing"

install -Dm755 "$BIN" "$HOME/.local/bin/tokenmaxxing"

for size in 16 22 24 32 48 64 128 256 512; do
    dir="$HOME/.local/share/icons/hicolor/${size}x${size}/apps"
    mkdir -p "$dir"
    "$BIN" --icon "$dir/tokenmaxxing.png" "$size" >/dev/null
done

install -Dm644 resources/tokenmaxxing.desktop "$HOME/.local/share/applications/tokenmaxxing.desktop"

gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
kbuildsycoca6 >/dev/null 2>&1 || true

echo "installed → ~/.local/bin/tokenmaxxing (ensure ~/.local/bin is on PATH)"
