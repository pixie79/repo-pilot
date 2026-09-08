# repo-pilot

```
┬─┐┌─┐┌─┐┌─┐ ┌─┐┬┬  ┌─┐┌┬┐
├┬┘├┤ ├─┘│ │─├─┘││  │ │ │
┴└─└─┘┴  └─┘ ┴  ┴┴─┘└─┘ ┴   what's still in for work.
```

**What's uncommitted, unpushed, and unreleased across every repo you own —
and the fleet operations to do something about it.**

If you keep dozens or hundreds of checkouts on disk and lose track of which
ones still have work sitting in them, this tells you, in one screen, live.
Then `update` brings every default branch to latest without touching your
working branch, and `export` hands the whole fleet to whatever comes next.

Written in Rust. Works on macOS and Linux.

A fork of [drydock](https://github.com/yetidevworks/drydock) by Andy Miller
(MIT), which provides the dashboard, the discovery walk and the git probing.
See [Licence](#licence).

![repo-pilot](screenshot.png)

Colour carries the state, so the rows worth acting on stand out without reading
a word: yellow for uncommitted changes, cyan for commits you haven't pushed,
bold light red for commits waiting on the remote, magenta for commits past the
last tag, red for conflicts and half-finished merges, and everything clean
dimmed out of the way.

## What it answers

- **What did I touch recently?** Filter to the last hour, day, week or month.
  A clean repo's activity is its newest commit; a dirty repo's is when you last
  saved a changed file, which is usually much more recent.
- **What have I not committed?** Staged, unstaged, untracked and conflicted
  counts per repo, plus stashes and any half-finished merge or rebase.
- **What have I not pushed?** Across *every* local branch, not just the one
  that happens to be checked out. Side branches are exactly where work goes
  missing.
- **What's worth releasing?** Every repo sits in one of three release states,
  in their own column:

  | | |
  |---|---|
  | `· unreleased` | no tags at all, never been released |
  | `✓ released` | tagged, with nothing since |
  | `◆ needs release` | tagged, but with commits or uncommitted changes past the tag |

  Plus the commits since the last tag with their actual subjects, and whether
  `CHANGELOG.md` has run ahead of the newest tag.

  Release state is a separate axis from working state: a repo can be dirty and
  released, or spotless and still needing a release.

## Install

repo-pilot is not on crates.io or in a Homebrew tap. Install it from source:

```sh
cargo install --git https://github.com/pixie79/repo-pilot repo-pilot
```

Or from a checkout:

```sh
git clone https://github.com/pixie79/repo-pilot
cd repo-pilot
make install          # into ~/.local/bin
```

## Use

Run it with no arguments for the live dashboard:

```sh
repo-pilot
```

It scans the directories in your config (`~/Projects` by default), *not* the
current directory, so it behaves the same wherever you invoke it.

### Dashboard keys

| | |
|---|---|
| `j` `k` `↑` `↓` | move · `ctrl-d` / `ctrl-u` half a page · `home` / `end` ends |
| mouse | click a row to select it · wheel moves the selection |
| `⏎` | detail view: branches, commits since the last tag, changed files |
| mouse | wheel scrolls the help and detail panes, and moves the column picker |
| `d` `u` `b` | filter to dirty, unpushed, behind |
| `r` `N` | filter to needs-release, never-released |
| `c` `i` `x` `e` | conflicts, operation in progress, detached HEAD, probe errors |
| `n` | only repos with nothing outstanding |
| `&` | switch between matching **any** active filter and **all** of them |
| `a` | clear every filter |
| `/` | fuzzy search on name, group and branch |
| `[` `]` | step through groups |
| `1` `2` `3` `4` | touched in the last hour, day, week, month · `0` any age |
| `s` `S` | cycle the sort key · reverse it |
| `o` `O` | show the folder in Finder · open in your editor |
| `t` | open in your git client |
| `T` `ctrl-o` | open a terminal there |
| `w` `y` | open the remote in a browser · copy the path |
| `f` `F` | fetch the selected repo · everything on screen |
| `ctrl-f` | fetch **every** repo in the fleet, whatever is filtered or scrolled off |
| `C` | choose which columns to show |
| `R` `ctrl-r` | rescan now — the local twin of `ctrl-f` |
| `?` `q` | help, with a legend for every marker on screen · quit |

Hold shift or ctrl and the hints along the bottom change to what *those* keys
would do — `o finder` becomes `O editor`, ctrl shows `^o terminal` and the rest
of its row. That needs a terminal which reports a modifier being held on its
own (the kitty keyboard protocol: kitty, Ghostty, WezTerm, iTerm2 3.5+, foot).
Anywhere else the footer is the static list it always was, and `?` still lists
everything.

### Commands

```sh
repo-pilot list --dirty --since 1d              # a table, then exit
repo-pilot list --unpushed --group acme --json  # machine-readable
repo-pilot list --cached                        # last known state, no probing (~5ms)
repo-pilot list --behind --fetch                # check every remote, then show what's behind
repo-pilot list --behind --fetch -g acme        # ...just that group's remotes
repo-pilot releasable --min-commits 3           # what's worth a release pass
repo-pilot status .                             # everything about one repo
repo-pilot scan                                 # refresh the cache
repo-pilot scan --fetch                         # ...checking the remotes as it goes
repo-pilot groups                               # per-group tallies
repo-pilot config init                          # write a config file
```

Plus the three that act on the fleet rather than reporting on it — `export`,
`update` and `roteiro`. They have their own section below.

Every command takes `--json`, so `repo-pilot list --cached --json` is cheap enough
to drive a status line.

### Filters

`--dirty` `--unpushed` `--behind` `--conflicted` `--in-progress` `--detached`
`--no-remote` `--no-upstream` `--stashed` `--clean` `--errored`

`--behind` reads what your last fetch left behind; add `--fetch` to check the
remotes first.

Release state: `--unreleased` (never tagged), `--needs-release`, `--released`.

Several filters **widen** the result by default: `--dirty --unpushed` means
"either". Pass `--match all` (or press `&`) to require all of them instead.

## Fleet operations

Everything above *reports*. These three *act*, and they differ from the rest of
the tool in two ways worth knowing before you run them.

They work on the repos below **where you are standing**, not on the configured
`roots`. Reporting should give the same answer wherever it is run from; acting
on whatever happens to be under your current directory is the entire point of
`cd`. Pass `--root PATH` to work somewhere else.

And they run one repo at a time. The probe sweep is heavily concurrent because
reads don't collide; these take index locks and move `HEAD` around, so they go
in order and say what happened to each repo as they go. A sweep with any
failure in it exits non-zero.

```sh
repo-pilot export                    # the fleet as JSON, for piping onwards
repo-pilot update                    # refresh every default branch
repo-pilot update --rebase never     # ...without being asked about rebasing
repo-pilot roteiro                   # init or sync the Roteiro graph in each
```

### `export`

Prints a JSON array of every repo below here — where it is, and where it came
from:

```json
[
  { "gitUrl": "git@github.com:acme/api.git", "path": "acme/api" },
  { "gitUrl": null, "path": "scratch/notes" }
]
```

`path` is relative to where the sweep started. `gitUrl` is `origin` when there
is one, otherwise whatever remote is configured, and `null` for a repo that
only ever existed on this disk — which is usually the row you were looking for.

This is not a shape of `list --json`. That one is the dashboard's view of a
*probed* fleet, thirty-odd fields deep, and it costs a full probe to produce.
This is a manifest, and it costs one small config-file read per repo, so it
stays quick across hundreds of them.

### `update`

Brings each repo's default branch up to date and puts you back where you were.
Per repo, in order:

1. Stash anything uncommitted, including untracked files.
2. Work out the default branch — `origin/HEAD` if the clone set it, otherwise
   the first of `main`, `master`, `develop`, `trunk` that exists on the remote.
3. Check it out and fast-forward it.
4. Check the original branch back out.
5. Restore the stash.
6. Offer to rebase the working branch onto the freshly-updated default.

Uncommitted work surviving that sequence is the whole design problem, so
nothing is ever discarded. A stash that will not pop is **left in the stash
list** with the command to recover it printed, because a conflict you resolve
by hand beats work that quietly went away.

The pull is `--ff-only`. A default branch that has diverged from its remote has
no safe automatic answer, and a merge commit invented by a sweep across two
hundred repos is the least safe one available — so it is reported and skipped,
with the branch and the stash still restored.

Repos on a **detached HEAD** and repos with **no origin remote** are skipped
with a reason rather than guessed at. A repo already on its default branch is
pulled in place, with no checkout and no rebase offered.

| | |
|---|---|
| `--rebase ask` | prompt per repo. The default. Means `never` when stdin is not a terminal, so a scripted run can't hang on a question |
| `--rebase always` | rebase every working branch that has somewhere to go |
| `--rebase never` | don't offer it |
| `--timeout N` | seconds allowed per `git pull`, default 120 |

The rebase is only offered when it could succeed: a tree still holding restored
changes says so instead. A rebase that fails is aborted, never left half-done.

### `roteiro`

[Roteiro](https://github.com/OffeneDatenmodellierung/Roteiro) is a codebase
knowledge-graph CLI. This runs `roteiro init` in repos that have never had it
and `roteiro sync` in repos that have, deciding per repo from whether a store
is already there.

The decision leans towards `sync`, because `init` writes into a repo — hooks, a
config file, an agent skill — and running it over a repo that already made
those choices is the expensive mistake. `roteiro` missing from `PATH` fails
once, up front, rather than once per repo. `--timeout` defaults to 600 seconds,
since a first `init` on a large repo builds the whole graph.

## How it works, and why it's quick

Measured on a real tree of ~550 repos, on an Apple silicon Mac:

| | |
|---|---|
| Walk the tree | ~0.9s |
| Refs, tags, tracking (tier 1) | ~1.4s |
| Full sweep, working trees from cache | **~2.2s** |
| Full sweep, cold | ~6.2s |
| `list --cached` | ~5ms |
| Watcher startup | ~19ms |

The split is the whole design. Reading refs and tags is cheap. Scanning working
trees is not: `git status` across that many repos costs around 40 seconds of
syscall time, roughly 85% of the total. So:

- **Tier 1** — refs, tags, tracking counts, stash count, in-progress operations
  — runs on every sweep.
- **Tier 2** — the working-tree scan — is cached against HEAD and the index
  mtime, and only reruns where something actually moved.
- A **filesystem watcher** then re-probes individual repos as they change, which
  costs milliseconds. That is what makes leaving the dashboard open all day
  reasonable rather than a background CPU tax.

Some implementation notes worth knowing:

- It **shells out to `git`** rather than linking a git library, so the numbers
  match exactly what you see on the command line, including whatever per-repo
  config is in play. Process startup is noise next to the scan it wraps.
- Every invocation passes **`--no-optional-locks`**. Without it, polling hundreds
  of repos would take `index.lock` and rewrite indexes constantly, fighting your
  editor and any GUI client you have open.
- The index mtime is deliberately **not** treated as activity. Any tool that runs
  `git status` refreshes it, so an editor sitting open on a repo would otherwise
  make it read as recently active when nothing had happened.
- **Git-flow back-merges are not counted** as work to release. After a release,
  `develop` carries a "Merge tag 'x.y.z' into develop" commit the tag cannot
  reach; counting it reported one commit to release when there was none. Merge
  commits are excluded, unless a merge is the only thing there and it genuinely
  changes the tree.
- Discovery **stops descending the moment it finds a repo**, which keeps
  submodules and vendored checkouts out of the list without enumerating them.

### Ahead, behind, and the network

Ahead and behind counts come from remote-tracking refs you have **already
fetched**, so no network access is involved and they are safe to recompute
constantly. That also means **"behind" is only as fresh as your last fetch**.

Both columns report **the branch you have checked out** — the one BRANCH names
in the same row — not every local branch summed. A topic branch you abandoned
a month ago being 128 commits behind is not your `main` being 128 behind, and
the row said so with a number next to the wrong branch's name. When another
local branch is ahead or behind on its own, the cell picks up a `*`: `·*` is
"this branch is in sync, some other one isn't". Select the repo to see which,
in the per-branch table. `--unpushed`, `--behind` and both sort keys still work
from the repo-wide totals, so a repo with only a stale side branch still turns
up in those.

`--json` carries all four numbers. `ahead` and `behind` are the repo-wide sums
they have always been, because 1.0 promised those fields wouldn't change
incompatibly and quietly redefining a number is the one break a script can't
notice. The per-branch pair the columns show is `branch_ahead` and
`branch_behind`.

Which is why a repo nothing has ever fetched shows `?` in BEHIND rather than
`·`. Zero there would say "in sync", and for a repo that has never fetched
that's a claim nobody checked — the count is zero because there was nothing to
compare against. The header counts them: `57 behind · 59 never fetched`.

Fetching is off by default, because it is real traffic against every remote you
own and a remote that wants credentials can hang. To do it:

| | |
|---|---|
| `f` `F` `ctrl-f` | in the dashboard: the selected repo, everything on screen, the whole fleet |
| `--fetch` | on `repo-pilot list` and `repo-pilot scan`, before anything is probed |
| `remote.fetch = true` | on a timer, every `remote.interval` |

Every one of those is bounded by `remote.concurrency` (4 by default — raise it
if you're fetching hundreds and can wait less), capped per repo by
`remote.timeout`, and refuses credential prompts rather than letting one hang.
A remote that can't be reached leaves that repo's counts exactly as stale as
they were, and says so rather than passing them off as checked.

`repo-pilot list --fetch` narrows to `--group` when you pass one, so
`--group acme --behind --fetch` is thirty fetches rather than five hundred.

Tags come along with a fetch, by git's ordinary auto-follow. They have to: the
release half of the table is built on them, so a tag pushed from another
machine that never arrived meant a repo went on reporting `needs release` for
work that had already been released. Local tags are never pruned, though — a
tag you've cut but not pushed is exactly what `needs release` exists to find.

The FETCHED column, off by default, shows how long since each repo last heard
from its remote (`never` if it never has). The detail view (`⏎`) says the same
for whichever repo you're on. Both read git's own `FETCH_HEAD`, so a `git pull`
you ran yourself in a terminal counts.

### Worktrees

A bare repo with its worktrees beside it — `.git/` next to `trunk/` and
`branch/` — is listed as you'd expect: each worktree is a repo in its own
right, grouped under the bare repo's name, and the bare repo itself gets a
`bare` state rather than being scanned for a working tree it doesn't have.

This works with `follow_nested_repos` left off. That setting exists to keep
submodules and vendored checkouts from being listed separately from the repo
containing them, and a bare repo has no working tree for anything to be
contained *in* — so it's descended into regardless. A checkout genuinely
nested inside a working tree is still pruned.

### Visibility

The VISIBILITY column shows whether each repo is public, private, or (on
GitHub Enterprise) internal. This isn't a `git` concept — nothing under
`.git` records it — so it's the one column that asks the *hosting provider*
directly, rather than reading anything local. Right now the only provider
supported is GitHub, via [`gh`](https://cli.github.com). (GitLab is the
natural next provider to add, since `glab` mirrors `gh` closely; Bitbucket
has no equivalent first-party CLI, so supporting it would mean handling API
tokens directly rather than riding on a CLI you've already authenticated.)

It's off by default (`visibility.enabled = false`), for the same reason
fetching is: it's real API traffic, and it depends on `gh` being installed
and already authenticated (`gh auth status`). Set `visibility.enabled = true`
to turn it on — the column appears with it, since a VISIBILITY column with
checking off would just read "checking off" on every row while still costing
the width. A checked value is cached and trusted for `visibility.interval`
(a day, by default — visibility changes rarely) so repeat runs stay cheap.
Filter with `--public` or `--private` on `list`.

Every non-value in this column says specifically why, rather than a bare `-`:

| Cell | Meaning |
|---|---|
| `no remote configured` | Nothing to ever check. Free to know, shown even with checking off. |
| `unsupported` | The remote is on a host nothing recognises. Also free. |
| `checking disabled` | The remote *is* checkable, but `visibility.enabled` is off. Shown in the table as `not checked`. |
| `unknown` | The repo itself couldn't be read, so whether it even has a remote isn't established. |
| `check failed` | A check was attempted and failed (rate limited, not authenticated, timed out), with nothing cached to fall back to. The actual reason is never shown in a table — one long message would widen the column for every row — but it's always available via `status <repo>` or `--json` (`visibility_error`).|

A GitHub wiki's clone URL (`<repo>.wiki.git`) isn't a repository the API can
look up on its own, so it's resolved to its parent repo instead: a wiki's
VISIBILITY cell shows the parent repo's actual visibility, not
`unsupported`.

Markers, so the column scans without reading the words: `●` public, `⊘`
private, `◐` internal, `!` a check that was attempted and failed, `·` a cell
with no answer for one of the reasons above. Public is green and private is
blue — private isn't a warning state, it's the one most repos should be in —
and grey is kept for the cells that hold no answer at all.

If you know the markers the words are redundant, so there's a **`VIS`**
column that's just the marker. It's four characters instead of fifteen, which
at 120 columns is the difference between `repo-pilot` and `dryd…` in the repo
name. Pick it in the `C` picker, or use `"visibility_short"` in `[ui]
columns`. The two forms are the same value, so asking for both gives you
whichever you listed first.

Remote URLs are parsed rather than prefix-matched, so the awkward-but-real
shapes are recognised too: `ssh.github.com` on port 443 (what you end up with
on a network that blocks 22), an explicit port, `git://`, credentials in an
https URL, a trailing slash. A web URL deeper than `owner/repo` is rejected
rather than guessed at.

### Columns

Press `C` in the dashboard to choose which columns to show. Space toggles the
one under the cursor, `J` and `K` move it left and right, `a` goes back to the
defaults, and `esc` saves. The table behind the panel redraws as you go, so
you can see what each change costs before committing to it. Only REPO can't be
turned off.

Closing the panel writes the list to `[ui] columns` in your config, which the
plain `repo-pilot list` table reads too:

```toml
[ui]
columns = ["group", "repo", "branch", "state", "changes", "age"]
```

Leave it unset and you get the defaults, which are every column except
VISIBILITY, STASH and FETCHED, and VISIBILITY as well when `visibility.enabled`
is on. Set it and you get exactly what you list, in the order you list it.

STASH is off by default rather than absent: the count is already read as part of
every sweep, so it costs nothing to know, but most repos have no stashes and a
column of `·` is six characters an entire row wide taken off the repo name. Turn
it on if you park work across a tree, or leave it off and press `s` to `stashes`
when you want to find it — the sort key works whether or not the column is
showing.

Turning VISIBILITY on in the picker turns `visibility.enabled` on with it and
kicks off a sweep — otherwise the column could only ever say "checking off".
Turning it back off leaves checking on, since `--public`, `--private` and
`--json` still read it; set `visibility.enabled = false` yourself to stop the
API traffic.

## Configuration

`repo-pilot config init` writes the defaults to
`~/Library/Application Support/repo-pilot/config.toml` on macOS, or
`~/.config/repo-pilot/config.toml` on Linux. The cache lives under
`~/Library/Caches/repo-pilot` or `~/.cache/repo-pilot`. `repo-pilot config path`
prints both, whatever they resolved to.

If you keep every tool's config in `~/.config` and sync it between machines,
you can have that on macOS too. Either set `XDG_CONFIG_HOME` (and
`XDG_CACHE_HOME`) explicitly, or just create `~/.config/repo-pilot` — repo-pilot
uses it if it's already there. Neither moves an existing config, so doing
nothing keeps the platform default.

```toml
roots = ["~/Projects"]       # each immediate subdirectory becomes a "group"
max_depth = 4
follow_nested_repos = false  # keeps submodules and vendored checkouts out
                             # (bare repos always descend — see "Worktrees")
follow_symlinks = false      # so a symlinked tree can't be counted twice
exclude = ["fixtures/**"]    # globs, relative to a root
prune = []                   # extra directory names to skip

[refresh]
interval = "5m"              # backstop sweep; the watcher handles the rest
watch = true
debounce = "1s"

[status]
untracked = "normal"         # normal | all | no
concurrency = 12             # omit to size from your core count
max_files = 200

[release]
# Requires a digit, so marker tags like `latest` aren't mistaken for releases.
tag_pattern = "*[0-9]*"
read_changelog = true

[remote]
fetch = false                # see "Ahead, behind, and the network"
interval = "1h"
concurrency = 4              # how many fetches run at once
timeout = "20s"              # per repo, then it's given up on

[visibility]
enabled = false               # see "Visibility"; requires `gh`
interval = "24h"

[ui]
# columns = [...]            # unset = the defaults; the `C` key writes this
default_filters = []         # e.g. ["dirty", "unpushed"]
default_sort = "activity"
default_since = ""           # e.g. "1w"
editor_command = ["zed", "{path}"]              # O
file_manager_command = ["open", "{path}"]      # o — `open -R` reveals instead
git_client_command = ["open", "-a", "Tower", "{path}"]      # t
terminal_command = ["open", "-a", "Terminal", "{path}"]     # T and ctrl-o
```

Every one of those is a command template with `{path}` replaced by the repo
root, so point them at whatever you actually use — `["open", "-a", "iTerm",
"{path}"]` for iTerm2, `["open", "-a", "Ghostty", "{path}"]` for Ghostty.

`repo-pilot config show` prints the effective config; `repo-pilot config path` says
where things live.

### Tags, and a note on git-flow

With git-flow, tags land on `master` while work carries on on `develop`, so the
newest tag by date often isn't an ancestor of `HEAD`. repo-pilot reports the nearest
**reachable** tag as the primary number and flags the discrepancy rather than
quietly picking one.

A tag shown in parentheses, like `(1.9.4)`, means the newest tag isn't reachable
from the current branch. The `+TAG` column always counts commits since the
reachable tag.

## Development

```sh
make check        # fmt, clippy with -D warnings, and tests
cargo test
cargo test -- --ignored    # plus the slow watcher-startup regression test
```

`repo-pilot tui-snapshot --width 150 --height 40 --view help` renders one dashboard
frame to plain text, which is how the layout gets reviewed without a terminal.
Views: `none`, `filtered`, `detail`, `help`, `search`, `scanning`.

Add `--html` and the same frame comes out as a standalone page with every
cell's colour intact. That is what `scripts/screenshot.sh` feeds to a headless
browser to regenerate `screenshot.png`, so the image at the top of this README
is drawn by the real UI code rather than scraped from someone's terminal, and
cannot drift from what the dashboard actually renders.

It renders against a throwaway fleet built by `scripts/demo-fleet.sh` rather
than against whatever you happen to have checked out — a screenshot of real
work publishes real branch names, and makes the image unreproducible by anyone
else. The synthetic part is the fleet, not the rendering: those are real git
repos with real commits, tags and remote-tracking refs, covering dirty, ahead,
behind, released, needs-release, never-released, detached and no-remote, so the
dashboard is doing exactly what it does against yours. `KEEP=1
scripts/demo-fleet.sh /tmp/fleet` leaves one on disk to poke at.

## Licence

MIT.

repo-pilot is a fork of [drydock](https://github.com/yetidevworks/drydock),
Copyright (c) 2026 Andy Miller, used under the MIT licence. The upstream
licence is preserved verbatim in `LICENSE` and `crates/repo-pilot/LICENSE`,
and `CHANGELOG.md` is drydock's history up to the point of the fork.
