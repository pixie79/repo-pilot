#!/usr/bin/env bash
# Regenerate README.md's screenshot.png from the real dashboard.
#
# `tui-snapshot --html` renders one frame through the actual UI code into a
# ratatui test buffer and emits it as HTML with every cell's colour intact, so
# the picture cannot drift from what the dashboard draws. A headless browser
# turns that into the PNG. Nothing is scraped from a terminal, and the frame
# reflects whatever `roots` your config points at — so run it somewhere with a
# fleet worth showing.
set -euo pipefail

cd "$(dirname "$0")/.."

COLS=${COLS:-172}
ROWS=${ROWS:-32}
# Sized to fit the frame at the CSS in `render_html`: ~9.3px per column, and
# ~19.8px per row, plus the 20px padding on each side.
PX_W=${PX_W:-1600}
PX_H=${PX_H:-690}

CHROME=${CHROME:-}
if [[ -z $CHROME ]]; then
  for candidate in \
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
    "/Applications/Chromium.app/Contents/MacOS/Chromium" \
    "$(command -v chromium || true)" \
    "$(command -v google-chrome || true)"; do
    [[ -n $candidate && -x $candidate ]] && { CHROME=$candidate; break; }
  done
fi
if [[ -z $CHROME ]]; then
  echo "No Chrome or Chromium found. Set CHROME=/path/to/binary." >&2
  exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

cargo build --quiet
./target/debug/repo-pilot tui-snapshot \
  --width "$COLS" --height "$ROWS" --html > "$tmp/frame.html"

"$CHROME" --headless --disable-gpu --hide-scrollbars \
  --force-device-scale-factor=2 \
  --window-size="$PX_W,$PX_H" \
  --screenshot="$tmp/frame.png" \
  "file://$tmp/frame.html" 2>/dev/null

mv "$tmp/frame.png" screenshot.png
echo "Wrote screenshot.png"
