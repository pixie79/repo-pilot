#!/usr/bin/env bash
# Regenerate README.md's screenshot.png.
#
# The frame is rendered against a synthetic fleet built by `demo-fleet.sh`, not
# against whoever happens to run this. A screenshot of somebody's real
# checkouts publishes their branch names and project names, and it makes the
# image unreproducible by anyone else — two problems with one fix.
#
# The synthetic part is the *fleet*, not the rendering. Those are real git
# repos with real commits, tags and remote-tracking refs, and the dashboard
# probes them exactly as it probes yours. `tui-snapshot --html` then renders one
# frame through the real UI code into a ratatui test buffer and emits it with
# every cell's colour intact, so the picture cannot drift from what the
# dashboard draws. Nothing is scraped from a terminal.
#
#   scripts/screenshot.sh
#
# Set KEEP=1 to leave the demo fleet on disk to poke at, and it will say where.
set -euo pipefail

cd "$(dirname "$0")/.."

COLS=${COLS:-140}
# The table shows ROWS-7 repos: two header lines, two footer lines, the box
# border and the column header. demo-fleet.sh builds 19, so 26 fits them with
# no empty band under the last row.
ROWS=${ROWS:-26}
# Sized to the frame at the CSS in `render_html`: ~9.02px per column and
# ~19.8px per row, plus 20px padding on each side.
PX_W=${PX_W:-1305}
PX_H=${PX_H:-560}

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
if [[ -n ${KEEP:-} ]]; then
  trap 'echo "Demo fleet left in $tmp/home/Projects" >&2' EXIT
else
  trap 'rm -rf "$tmp"' EXIT
fi

cargo build --quiet

# Built under a stand-in HOME called Projects, purely so the dashboard's
# header and footer read `~/Projects/acme/api` rather than a mktemp path with
# a random suffix in it. `paths::contract` collapses $HOME back to `~`.
mkdir -p "$tmp/home"
# `pwd -P` matters: on macOS /var is a symlink to /private/var, so discovery
# canonicalizes the repo paths and an uncanonicalized $HOME would fail to
# prefix-match them — leaving the raw mktemp path in the footer.
HOME="$(cd "$tmp/home" && pwd -P)"
export HOME
./scripts/demo-fleet.sh "$HOME/Projects"

# A config of our own, so this neither reads nor disturbs the real one. An
# absolute XDG_CONFIG_HOME wins outright on every platform (see `paths.rs`),
# and pointing XDG_CACHE_HOME here too keeps the demo out of the real cache.
mkdir -p "$tmp/config/repo-pilot"
cat > "$tmp/config/repo-pilot/config.toml" <<EOF
roots = ["~/Projects"]
max_depth = 4
EOF

XDG_CONFIG_HOME="$tmp/config" XDG_CACHE_HOME="$tmp/cache" \
  ./target/debug/repo-pilot tui-snapshot \
  --width "$COLS" --height "$ROWS" --html > "$tmp/frame.html"

"$CHROME" --headless --disable-gpu --hide-scrollbars \
  --force-device-scale-factor=2 \
  --window-size="$PX_W,$PX_H" \
  --screenshot="$tmp/frame.png" \
  "file://$tmp/frame.html" 2>/dev/null

cp "$tmp/frame.png" screenshot.png
echo "Wrote screenshot.png"
