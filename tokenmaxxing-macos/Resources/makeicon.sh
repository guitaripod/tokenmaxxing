#!/usr/bin/env bash
# Render the app icon PNG via the built binary, then assemble AppIcon.icns.
# Usage: makeicon.sh <binary> <output.icns>
set -euo pipefail
BIN="$1"
OUT="$2"

PNG="$(mktemp -t tmicon).png"
"$BIN" --icon "$PNG" >/dev/null

SET="$(mktemp -d)/AppIcon.iconset"
mkdir -p "$SET"
gen() { sips -z "$1" "$1" "$PNG" --out "$SET/icon_$2.png" >/dev/null; }
gen 16   16x16
gen 32   16x16@2x
gen 32   32x32
gen 64   32x32@2x
gen 128  128x128
gen 256  128x128@2x
gen 256  256x256
gen 512  256x256@2x
gen 512  512x512
gen 1024 512x512@2x

iconutil -c icns "$SET" -o "$OUT"
