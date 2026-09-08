//! `repo-pilot roteiro` — scaffold or refresh the Roteiro graph in every repo.
//!
//! Roteiro is a codebase knowledge-graph CLI. `init` scaffolds it — config,
//! hooks, agent skill — and `sync` incrementally rebuilds the graph for the
//! current tree. Which one a repo needs is a question about the repo, and
//! asking it per repo is the entire operation.
//!
//! The decision leans towards `sync`. `init` writes into a repo — hooks, a
//! config file — so running it over a repo that is already set up is the
//! expensive mistake, while running `sync` over a repo that has a config but
//! no store just builds the store, which is what was wanted anyway.

use anyhow::{Context, Result};
use std::path::Path;
use std::time::Duration;

use crate::config::Config;
use crate::ops;

/// Which roteiro subcommand a repo needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Init,
    Sync,
}

impl Action {
    pub fn arg(self) -> &'static str {
        match self {
            Action::Init => "init",
            Action::Sync => "sync",
        }
    }
}

/// Choose between scaffolding and refreshing.
///
/// Pure, so the rule can be stated once and tested, rather than inferred from
/// a pile of `Path::exists` calls at the call site.
pub fn decide(has_config: bool, has_store: bool) -> Action {
    if has_config || has_store {
        Action::Sync
    } else {
        Action::Init
    }
}

/// Roteiro's project config, which `roteiro config` reports as the project
/// layer. Note that `init` does *not* write one — it is committed by hand when
/// a repo wants to override the user-level defaults — so this is the weaker of
/// the two signals and is here to catch a repo configured before it was built.
fn has_config(repo: &Path) -> bool {
    repo.join("roteiro.toml").is_file()
}

/// Roteiro's store, which is what `init` actually leaves behind: `graph.db`
/// under the git directory, with `.roteiro/` as the other layout in the wild.
/// Resolving the git dir rather than assuming `.git/` keeps this working
/// inside a linked worktree.
fn has_store(repo: &Path) -> bool {
    if repo.join(".roteiro").is_dir() {
        return true;
    }
    crate::git::resolve_git_dir(repo)
        .map(|dir| dir.join("roteiro").exists())
        .unwrap_or(false)
}

pub async fn run(root: &Path, cfg: &Config, timeout: Duration) -> Result<bool> {
    // One clear failure now beats the same failure repeated per repo.
    if !roteiro_on_path() {
        anyhow::bail!(
            "roteiro is not on PATH. Install it first: \
             https://github.com/OffeneDatenmodellierung/Roteiro"
        );
    }

    let repos = ops::fleet(root, cfg)?;
    if repos.is_empty() {
        eprintln!("No repos below {}.", ops::relative_path(root, root));
        return Ok(true);
    }

    let width = repos
        .iter()
        .map(|r| ops::relative_path(root, &r.root).chars().count())
        .max()
        .unwrap_or(0)
        .min(48);

    let mut tally = ops::Tally::default();
    for repo in &repos {
        let name = ops::relative_path(root, &repo.root);
        let action = decide(has_config(&repo.root), has_store(&repo.root));
        match run_one(&repo.root, action, timeout).await {
            Ok(()) => {
                println!("✓ {:width$}  {}", name, action.arg());
                tally.done += 1;
            }
            Err(err) => {
                println!("✗ {:width$}  {} failed: {err}", name, action.arg());
                tally.failed += 1;
            }
        }
    }
    Ok(tally.report("synced"))
}

async fn run_one(repo: &Path, action: Action, timeout: Duration) -> Result<()> {
    let mut cmd = tokio::process::Command::new("roteiro");
    cmd.arg(action.arg())
        .current_dir(repo)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);

    let output = tokio::time::timeout(timeout, cmd.output())
        .await
        .map_err(|_| anyhow::anyhow!("timed out after {}s", timeout.as_secs()))?
        .context("Running roteiro")?;

    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let why = stderr
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("no output");
    anyhow::bail!("{why}")
}

fn roteiro_on_path() -> bool {
    std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path).any(|dir| {
                let candidate = dir.join("roteiro");
                candidate.is_file() || candidate.with_extension("exe").is_file()
            })
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    // A repo nobody has scaffolded yet.
    #[test]
    fn an_untouched_repo_gets_init() {
        assert_eq!(decide(false, false), Action::Init);
    }

    #[test]
    fn a_scaffolded_repo_gets_sync() {
        assert_eq!(decide(true, true), Action::Sync);
    }

    // A fresh clone of a scaffolded repo has the committed config but no
    // store, because the store lives under the git directory and is not
    // cloned. `sync` builds it. Re-running `init` there would rewrite hooks
    // and config over a repo that already made those choices.
    #[test]
    fn a_fresh_clone_with_config_but_no_store_gets_sync() {
        assert_eq!(decide(true, false), Action::Sync);
    }

    // And the reverse: a store with no config still means somebody has run
    // roteiro here, so refreshing is right.
    #[test]
    fn a_store_without_config_gets_sync() {
        assert_eq!(decide(false, true), Action::Sync);
    }
}
