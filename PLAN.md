# repo-pilot — Build Plan

## What this tool is
`repo-pilot` is a Rust TUI + CLI that discovers every git repository nested
below the current working directory and operates on the whole fleet. It is a
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

## What is left to build

### Operation 1 — `list` (JSON export)
- [ ] Print a JSON **array** to stdout, one object per discovered repo:
      `{ "gitUrl": <origin remote URL, or null if none>, "path": <path RELATIVE to cwd> }`.
- [ ] Pure query, no side effects. Valid, parseable JSON via `serde_json`.
- [ ] Model change: extend the repo model so the origin remote URL (`gitUrl`)
      is captured for export.

### Operation 2 — `update` (refresh default branch, keep working branch)
Per repo, in order, robust so uncommitted work is never lost:
- [ ] If the working tree is dirty, `git stash` (push); track whether a stash
      was actually created.
- [ ] Detect the default branch (auto-detect `main` vs `master` via
      `origin/HEAD`, with a fallback).
- [ ] Check out the default branch and pull it to latest (fetch +
      fast-forward / pull).
- [ ] Check back out to the **original** working branch.
- [ ] If a stash was created, `git stash pop`.
- [ ] Then present a **Y/N** prompt offering to rebase the working branch onto
      the freshly-updated default branch; only rebase on `Y`.
- [ ] Handle gracefully (skip with a clear message, never crash / never
      corrupt state): repo already on the default branch, detached HEAD, and
      repos with no origin remote.

### Operation 3 — `roteiro` (init or sync)
- [ ] Per repo: if roteiro is **not** yet initialized, run `roteiro init`;
      otherwise run `roteiro sync`. (`roteiro` is a codebase knowledge-graph
      CLI on PATH; `init` scaffolds it, `sync` incrementally updates.)
- [ ] Detect initialization by the presence of roteiro's scaffold/config dir
      in the repo.
- [ ] Report per-repo outcome.

### Wiring
- [ ] Expose all three as **CLI subcommands** (`cli.rs`).
- [ ] Where it fits naturally, expose them as **TUI actions / keybindings**.
- [ ] Keep the existing drydock dashboard fully working.

### Quality bar
- [ ] Idiomatic Rust; reuse existing modules; no `unwrap`-on-external-IO that
      can panic mid-fleet — handle per-repo errors and continue the sweep.
- [ ] Factor pure logic (JSON export shape, default-branch detection,
      roteiro init-vs-sync decision, stash/restore decision) so it is unit
      testable without a live network.
- [ ] Update `README.md` with usage for the three operations.

### Green gates (all must pass before the branch is considered done)
Run from the repo root:
- [ ] `cargo build --workspace`
- [ ] `cargo test --workspace`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo fmt --all --check`

## Delivery
- Branch: `feat/repo-pilot-ops` (this branch). Commit with clear messages.
- Open a PR into `main` on `pixie79/repo-pilot`.
- **Cross-vendor review required:** the implementer's diff must be reviewed by
  a different vendor than the one that wrote it before merge.

## Build-environment note (2026-09-08)
Implementation is currently **blocked on delegation infrastructure**, not on
the task itself. Every coding worker failed to run:
- `claude_code` — native harness dies at terminal start; `apiKeyHelper`
  auth is failing. Fix: re-authenticate (run `/status` in a Claude Code
  session).
- `codex` — no model provider configured for the harness in this deployment.
  Fix: `omnigent setup` / sign in.
- `pi` — gateway "verified" but every call fails: OpenAI-family models return
  `429` / `500` (sustained rate-limit / backend errors), Qwen returns `400`,
  and Claude-family models hit a `thinking.type.enabled` request-shaping bug.

Recommended unblock: re-auth `claude_code` (restores the strongest implementer
**and** a genuine cross-vendor reviewer), or wait for the OpenAI rate window to
reset and retry `pi` on a gpt-5-family model.
