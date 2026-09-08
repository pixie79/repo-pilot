//! The fleet operations: things that act on every repo below you, rather than
//! reporting on every repo you own.
//!
//! Two things separate these from `list`, `scan` and the dashboard.
//!
//! They scope themselves to the **current directory** rather than to the
//! configured `roots`. The reporting side of this tool answers "what is the
//! state of everything I own", and it should give the same answer wherever it
//! is run from. An operation is the opposite: `cd`-ing somewhere and acting on
//! what is there is the whole point, and a command that reached outside that
//! directory because a config file said so would be a nasty surprise. `--root`
//! overrides when you want to stand somewhere else.
//!
//! And they run **one repo at a time**. The probe sweep is heavily concurrent
//! because reads don't collide; these take index locks, move HEAD around, and
//! in one case stop to ask a question. Interleaving that across a few hundred
//! repos would produce output nobody could follow and failures nobody could
//! attribute.

pub mod export;
pub mod roteiro;
pub mod update;

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::discover::{self, Discovered};

/// Resolve the directory an operation works below: `--root` if given, and
/// otherwise wherever the user is standing.
pub fn scope(root: Option<&str>) -> Result<PathBuf> {
    let dir = match root {
        Some(raw) => crate::paths::expand(raw),
        None => std::env::current_dir().context("Could not read the current directory")?,
    };
    if !dir.is_dir() {
        anyhow::bail!("Not a directory: {}", dir.display());
    }
    dir.canonicalize()
        .with_context(|| format!("Could not resolve {}", dir.display()))
}

/// Every repo at or below `root`, in path order.
///
/// Depth is still bounded by `max_depth`, and `exclude` / `prune` still apply:
/// an operation should look exactly where the dashboard would look, just from
/// somewhere else.
pub fn fleet(root: &Path, cfg: &Config) -> Result<Vec<Discovered>> {
    let (found, stats) = discover::discover_under(root, cfg)?;
    tracing::debug!(
        repos = stats.repos_found,
        dirs = stats.dirs_visited,
        pruned = stats.pruned,
        "discovery"
    );
    Ok(found)
}

/// A repo's path as the user would refer to it: relative to where the sweep
/// started, or the repo's own directory name when it *is* where the sweep
/// started.
///
/// Kept free of the filesystem so the shape can be tested directly.
pub fn relative_path(root: &Path, repo: &Path) -> String {
    match repo.strip_prefix(root) {
        Ok(rest) if rest.as_os_str().is_empty() => repo
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| repo.display().to_string()),
        Ok(rest) => rest.to_string_lossy().to_string(),
        // Outside the root entirely, which discovery shouldn't produce. An
        // absolute path is still a true answer, so say that rather than guess.
        Err(_) => repo.display().to_string(),
    }
}

/// How a sweep ended, for the one-line summary and the exit code.
#[derive(Debug, Default)]
pub struct Tally {
    pub done: usize,
    pub skipped: usize,
    pub failed: usize,
}

impl Tally {
    pub fn total(&self) -> usize {
        self.done + self.skipped + self.failed
    }

    /// Print the closing line, and say whether anything went wrong.
    pub fn report(&self, verb: &str) -> bool {
        let repos = if self.total() == 1 { "repo" } else { "repos" };
        let mut parts = vec![format!("{} {repos}, {} {verb}", self.total(), self.done)];
        if self.skipped > 0 {
            parts.push(format!("{} skipped", self.skipped));
        }
        if self.failed > 0 {
            parts.push(format!("{} failed", self.failed));
        }
        eprintln!("\n{}.", parts.join(", "));
        self.failed == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repo_below_the_root_is_named_relatively() {
        assert_eq!(
            relative_path(Path::new("/g"), Path::new("/g/acme/api")),
            "acme/api"
        );
        assert_eq!(
            relative_path(Path::new("/g"), Path::new("/g/loose")),
            "loose"
        );
    }

    // Standing inside a checkout, the one repo found is the root itself. Its
    // relative path is empty, and printing an empty string as a repo name is
    // worse than useless.
    #[test]
    fn the_root_repo_is_named_by_its_directory() {
        assert_eq!(
            relative_path(Path::new("/g/acme/api"), Path::new("/g/acme/api")),
            "api"
        );
    }

    #[test]
    fn a_path_outside_the_root_is_reported_in_full() {
        assert_eq!(
            relative_path(Path::new("/g"), Path::new("/elsewhere/api")),
            "/elsewhere/api"
        );
    }
}
