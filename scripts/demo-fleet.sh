#!/usr/bin/env bash
# Build a throwaway fleet of git repos covering every state the dashboard can
# show: dirty and clean, ahead and behind, released, needing release and never
# released, detached, stashed, and one with no remote at all.
#
# It exists so `scripts/screenshot.sh` can produce a screenshot that shows the
# tool rather than whoever ran it. Nothing here is fabricated at the rendering
# layer — these are real repos with real commits, real tags and real
# remote-tracking refs, so the dashboard is doing exactly what it does against
# your own checkouts.
#
#   scripts/demo-fleet.sh /tmp/fleet
#
# Everything lives under the given directory. Bare upstreams go in a dotted
# subdirectory, which discovery skips along with every other hidden directory.
set -euo pipefail

FLEET=${1:?usage: demo-fleet.sh <directory>}
mkdir -p "$FLEET"
FLEET=$(cd "$FLEET" && pwd)
UPSTREAMS="$FLEET/.upstreams"
mkdir -p "$UPSTREAMS"

# Isolated from whoever runs this: no global config, no signing, no templates,
# and identities that don't belong to a real person.
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null
export GIT_AUTHOR_NAME="Dana Reyes" GIT_AUTHOR_EMAIL="dana@example.com"
export GIT_COMMITTER_NAME="Dana Reyes" GIT_COMMITTER_EMAIL="dana@example.com"

NOW=$(date +%s)
MIN=60 HOUR=3600 DAY=86400 WEEK=604800

# `touch -d @epoch` is GNU, `touch -t` is BSD. A dirty repo's activity is its
# newest changed file, so without this every dirty row would read "now".
set_mtime() { # path epoch
  touch -d "@$2" "$1" 2>/dev/null ||
    touch -t "$(date -r "$2" +%Y%m%d%H%M.%S)" "$1"
}

g() { git -C "$1" "${@:2}"; }

commit() { # repo message secs_ago
  local when=$((NOW - $3))
  GIT_AUTHOR_DATE="@$when +0000" GIT_COMMITTER_DATE="@$when +0000" \
    g "$1" commit -qm "$2"
}

write() { # repo file content
  mkdir -p "$(dirname "$1/$2")"
  printf '%s\n' "$3" > "$1/$2"
}

# Create a repo with `count` commits on `branch`, the newest `age` seconds old,
# cloned from a bare upstream so remote-tracking refs are real.
new_repo() { # slug branch count age
  local slug=$1 branch=$2 count=$3 age=$4
  local bare="$UPSTREAMS/${slug//\//__}.git"
  local repo="$FLEET/$slug"

  git init -q --bare -b "$branch" "$bare"
  git clone -q "$bare" "$repo" 2>/dev/null
  g "$repo" symbolic-ref HEAD "refs/heads/$branch"

  local i
  for ((i = count; i >= 1; i--)); do
    write "$repo" "src/mod_$i.rs" "pub fn thing_$i() {}"
    g "$repo" add -A
    commit "$repo" "$(subject "$i")" $((age + (i - 1) * DAY))
  done
  g "$repo" push -q -u origin "$branch"
  # A real fetch, so FETCH_HEAD exists and "behind" reads as checked rather
  # than never-looked-at.
  g "$repo" fetch -q origin
  echo "$repo"
}

subject() {
  local subjects=(
    "tighten the retry window"
    "drop the unused adapter"
    "cover the empty-batch case"
    "rename the config key"
    "split the parser out"
    "fix the off-by-one in paging"
    "document the failure modes"
    "cache the resolved schema"
  )
  echo "${subjects[$(($1 % ${#subjects[@]}))]}"
}

# Push `n` further commits to a repo's upstream, so the clone reads as behind.
push_upstream() { # repo n age
  local repo=$1 tmp
  tmp=$(mktemp -d)
  git clone -q "$(g "$repo" config --get remote.origin.url)" "$tmp/c" 2>/dev/null
  local i
  for ((i = 1; i <= $2; i++)); do
    write "$tmp/c" "upstream_$i.txt" "$i"
    g "$tmp/c" add -A
    commit "$tmp/c" "$(subject "$((i + 3))")" $(($3 + i * HOUR))
  done
  g "$tmp/c" push -q origin HEAD
  rm -rf "$tmp"
  g "$repo" fetch -q origin
}

dirty() { # repo modified untracked age
  local repo=$1 i
  for ((i = 1; i <= $2; i++)); do
    write "$repo" "src/mod_$i.rs" "pub fn thing_$i() { /* wip */ }"
    set_mtime "$repo/src/mod_$i.rs" $((NOW - $4))
  done
  for ((i = 1; i <= $3; i++)); do
    write "$repo" "notes_$i.md" "scratch"
    set_mtime "$repo/notes_$i.md" $((NOW - $4))
  done
}

tag() { # repo name age
  local when=$((NOW - $3))
  GIT_AUTHOR_DATE="@$when +0000" GIT_COMMITTER_DATE="@$when +0000" \
    g "$1" tag "$2"
}

# Commits made locally and deliberately not pushed, which is what the AHEAD
# column counts.
ahead() { # repo n age
  local repo=$1 i
  for ((i = 1; i <= $2; i++)); do
    write "$repo" "src/local_$i.rs" "pub fn local_$i() {}"
    g "$repo" add -A
    commit "$repo" "$(subject "$((i + 5))")" $(($3 + (i - 1) * HOUR))
  done
}

# Commits after the tag, which is what "needs release" means.
past_tag() { # repo n age
  local repo=$1 i
  for ((i = 1; i <= $2; i++)); do
    write "$repo" "src/later_$i.rs" "pub fn later_$i() {}"
    g "$repo" add -A
    commit "$repo" "$(subject "$((i + 1))")" $(($3 + (i - 1) * HOUR))
  done
  g "$repo" push -q origin HEAD
}

echo "Building a demo fleet in $FLEET" >&2

# --- acme ------------------------------------------------------------------
r=$(new_repo acme/api main 4 $((3 * DAY)))
tag "$r" v2.4.0 $((3 * DAY))
past_tag "$r" 7 $((6 * HOUR))
dirty "$r" 2 1 $((12 * MIN))

r=$(new_repo acme/web main 5 $((5 * DAY)))
tag "$r" v1.9.2 $((5 * DAY))
past_tag "$r" 4 $((2 * DAY))
push_upstream "$r" 3 $((2 * HOUR))

r=$(new_repo acme/billing feat/ledger-rewrite 3 $((5 * HOUR)))
dirty "$r" 0 4 $((5 * HOUR))

r=$(new_repo acme/docs main 2 $DAY)
tag "$r" v0.8.0 $DAY

r=$(new_repo acme/mobile main 3 $((2 * DAY)))
g "$r" checkout -q --detach HEAD

r=$(new_repo acme/auth-gateway develop 4 $((9 * DAY)))
tag "$r" v3.0.0 $((9 * DAY))
past_tag "$r" 12 $((4 * DAY))

# --- northwind -------------------------------------------------------------
r=$(new_repo northwind/etl main 4 $((4 * HOUR)))
tag "$r" etl-v3.1.0 $((3 * DAY))
past_tag "$r" 2 $((4 * HOUR))
dirty "$r" 3 0 $((3 * HOUR))

r=$(new_repo northwind/warehouse main 6 $((4 * DAY)))
tag "$r" v5.0.0 $((4 * DAY))

r=$(new_repo northwind/dashboards feat/usage-panel 3 $((8 * DAY)))
ahead "$r" 2 $WEEK

r=$(new_repo northwind/ingest main 5 $((8 * DAY)))
push_upstream "$r" 5 $((6 * HOUR))

r=$(new_repo northwind/schema-registry main 3 $((10 * DAY)))
tag "$r" v1.4.1 $((10 * DAY))
past_tag "$r" 1 $((9 * DAY))

r=$(new_repo northwind/replay-tool main 2 $((12 * DAY)))

# --- oss -------------------------------------------------------------------
r=$(new_repo oss/parser main 5 $((2 * WEEK)))
tag "$r" v1.2.0 $((5 * WEEK))
past_tag "$r" 31 $((2 * WEEK))

r=$(new_repo oss/cli-kit main 3 $((3 * WEEK)))
dirty "$r" 1 2 $((3 * WEEK))
g "$r" stash -q -u
dirty "$r" 1 2 $((3 * WEEK))

r=$(new_repo oss/wasm-loader main 4 $((5 * WEEK)))
tag "$r" v0.3.4 $((5 * WEEK))

r=$(new_repo oss/spec-tests main 2 $((6 * WEEK)))
ahead "$r" 3 $((6 * WEEK))
dirty "$r" 2 0 $((6 * WEEK))

r=$(new_repo oss/bench-harness fix/allocator-churn 3 $((7 * WEEK)))

# --- loose repos, directly in the root -------------------------------------
r=$(new_repo sandbox main 2 $((6 * HOUR)))
dirty "$r" 0 6 $((6 * HOUR))
# No remote at all: the row that says "this only ever existed on your disk".
g "$r" remote remove origin

r=$(new_repo prototype main 2 $((11 * DAY)))

echo "Built $(find "$FLEET" -maxdepth 3 -name .git | wc -l | tr -d ' ') repos" >&2
