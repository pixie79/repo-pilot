# repo-pilot — Build Plan

## What this tool is
`repo-pilot` is a Rust TUI + CLI that discovers git repositories and operates
on the whole fleet. The reporting commands scan the roots in your config; the
three fleet operations scan below the current working directory. It is a
fork of [drydock](https://github.com/yetidevworks/drydock) (MIT), which already
provides fleet discovery, a ratatui dashboard, and git status probing. We are
extending that base with three fleet operations.

**Attribution:** drydock is MIT-licensed; its `LICENSE` and attribution are
preserved (`crates/repo-pilot/LICENSE`, root `LICENSE`). Keep them.

## Current state (done)
- [x] Repository initialized (`~/GIT/repo-pilot`), pushed to
      `git@github.com:pixie79/repo-pilot.git` (private).
- [x] drydock vendored in as the base and renamed to `repo-pilot`
      (commit `5c0437d`). Crate lives at `crates/repo-pilot/`.
- [x] Existing drydock capabilities we will reuse:
  - `discover.rs` — fleet discovery of git repos below a path (prunes nested
    repos so a repo inside a repo is not double-counted).
  - `git.rs` — git operations / status probing.
  - `model.rs` — the per-repo data model.
  - `cli.rs` — subcommand plumbing.
  - `tui/` — the ratatui dashboard (must keep working).

## What was built

All three operations are in on `feat/repo-pilot-ops`. Two decisions were taken
during the build that differ from this plan as first written, both recorded
here because the plan is the thing a reviewer reads first.

**`export`, not `list`.** The plan gave operation 1 the name `list`, but `list`
already existed with twenty-odd filter flags and a `--json` shape carrying a
documented compatibility promise (`report.rs`, and a test pinning it).
Redefining it would have been a silent break, so the JSON manifest is its own
subcommand.

**Scope.** The plan said the tool discovers repos "below the current working
directory". It did not: discovery ran from `scan.roots` in config, defaulting
to `~/Projects`, and the README documented that as deliberate. Rather than
change discovery under the reporting commands, the three operations scope to
the current directory — which is what an operation should do — and everything
that reports is untouched. `--root PATH` overrides.

### Operation 1 — `export`
- [x] JSON array to stdout, one object per repo: `{ "gitUrl", "path" }`, path
      relative to where the sweep started.
- [x] Pure query, no side effects. `serde_json`, so it is valid by construction.
- [x] No model change needed after all: `remote_url` was already on `RefState`,
      and `git::quick_remote_url()` reads it from `.git/config` without a probe.

### Operation 2 — `update`
- [x] Stash when dirty, tracking whether a stash was *really* created by
      watching `refs/stash` move rather than by parsing prose.
- [x] Default branch from `origin/HEAD`, falling back to the first of `main`,
      `master`, `develop`, `trunk` that exists on the remote — clones set
      `origin/HEAD`, repos created locally and pushed afterwards do not.
- [x] Check out the default branch and fast-forward it. `--ff-only`: a diverged
      default branch is reported and skipped, never merged.
- [x] Check the original branch back out.
- [x] Pop the stash if one was created. If it will not pop, it is **left in the
      stash list** with the recovery command printed.
- [x] Y/N rebase prompt, as `--rebase ask|always|never`. `ask` is the default
      and becomes `never` when stdin is not a terminal, so a scripted sweep
      cannot hang on a question. Only offered when it could succeed.
- [x] Graceful: already on the default branch (pulled in place, no round trip),
      detached HEAD (skipped), no origin remote (skipped).

### Operation 3 — `roteiro`
- [x] `roteiro init` when the repo has never had it, `roteiro sync` when it has.
- [x] Detected from the store (`<gitdir>/roteiro`, or `.roteiro/`) and
      `roteiro.toml`. Note that `init` does not write `roteiro.toml`, so the
      store is the load-bearing signal.
- [x] Per-repo outcome reported; `roteiro` missing from PATH fails once, up
      front, rather than once per repo.

### Wiring
- [x] All three are CLI subcommands.
- [x] Not exposed as TUI actions. The dashboard has no confirm overlay and no
      suspend-to-shell, so a per-repo prompt cannot run under the alternate
      screen; half-building a modal to host one prompt was not worth it. The
      dashboard is otherwise untouched and still works.

### Quality bar
- [x] Per-repo errors are reported and the sweep continues. A sweep with any
      failure exits non-zero.
- [x] The decisions are pure functions with tests: the per-repo plan,
      default-branch detection, stash-really-created, init-vs-sync, and the
      export shape. The `update` round trip is additionally tested against real
      repos with a bare repo standing in for the remote — success, diverged,
      and both skip paths — with no network.
- [x] README updated: a "Fleet operations" section covering all three.

### Green gates
- [x] `cargo build --workspace`
- [x] `cargo test --workspace` — 138 pass. Five were failing before this branch
      on any machine whose global git config sets `tag.gpgSign` or
      `tag.forceSignAnnotated`; the fixtures now isolate the ambient config the
      way the rest of the test module already did.
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo fmt --all --check`

## Delivery
- Branch: `feat/repo-pilot-ops`. Open a PR into `main` on `pixie79/repo-pilot`.
- **Cross-vendor review still required:** this branch was implemented by
  Claude, so the review must not be.

## Follow-ups, now done
- `scan.roots` pointed at `~/Projects`, which does not exist on this machine.
  Config written with `roots = ["~/GIT"]`; the reporting commands now see 33
  repos across 6 groups. The shipped default `exclude` list was upstream's
  author's own fixture repos, which excluded nothing for anyone else — now
  empty.
- `screenshot.png` regenerated from the real dashboard against `~/GIT`, via a
  new `tui-snapshot --html` and `scripts/screenshot.sh`.
- Fixed while regenerating it: left-aligned table cells had no gutter, so a
  value that exactly filled its column ran into the next one
  (`Flutter-Globalebt/ebt-architecture`). Only GROUP ever showed it.
