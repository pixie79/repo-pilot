//! `repo-pilot export` — the fleet as a JSON array, for whatever comes next.
//!
//! Deliberately not a shape of `list --json`. That one is the dashboard's view
//! of a repo: thirty-odd fields about branches, tags, counts and states, and
//! it costs a full probe of every repo to produce. This is a manifest — where
//! the repos are and where they came from — and it costs one small file read
//! each, because a remote URL is sitting in `.git/config` and never needed a
//! `git` process to fetch it. Exporting five hundred repos is milliseconds.

use anyhow::Result;
use serde::Serialize;
use std::path::Path;

use crate::config::Config;
use crate::git;
use crate::ops;

/// One repo, in the shape the export promises.
#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// Where the repo came from. `origin` when there is one, otherwise
    /// whichever remote is configured, and null when there is no remote at
    /// all — a repo that only ever existed on this disk is a real thing to
    /// find, and the null is the useful part of finding it.
    pub git_url: Option<String>,
    /// The repo's path relative to where the sweep started.
    pub path: String,
}

/// Build the export from discovered repos. Separated from printing so the
/// shape can be tested without a terminal, and from discovery so it can be
/// tested without a fleet.
pub fn entries<'a>(
    root: &Path,
    repos: impl IntoIterator<Item = &'a Path>,
    remote_url: impl Fn(&Path) -> Option<String>,
) -> Vec<Entry> {
    repos
        .into_iter()
        .map(|repo| Entry {
            git_url: remote_url(repo),
            path: ops::relative_path(root, repo),
        })
        .collect()
}

pub fn run(root: &Path, cfg: &Config) -> Result<()> {
    let repos = ops::fleet(root, cfg)?;
    let paths: Vec<&Path> = repos.iter().map(|r| r.root.as_path()).collect();
    let entries = entries(root, paths, git::quick_remote_url);
    println!("{}", serde_json::to_string_pretty(&entries)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn urls(pairs: &[(&'static str, Option<&'static str>)]) -> impl Fn(&Path) -> Option<String> {
        let map: Vec<(PathBuf, Option<String>)> = pairs
            .iter()
            .map(|(p, u)| (PathBuf::from(*p), u.map(str::to_string)))
            .collect();
        move |repo| {
            map.iter()
                .find(|(p, _)| p == repo)
                .and_then(|(_, u)| u.clone())
        }
    }

    #[test]
    fn the_export_is_a_json_array_of_url_and_relative_path() {
        let root = Path::new("/g");
        let entries = entries(
            root,
            [Path::new("/g/acme/api"), Path::new("/g/solo")],
            urls(&[
                ("/g/acme/api", Some("git@github.com:acme/api.git")),
                ("/g/solo", None),
            ]),
        );
        let json = serde_json::to_string(&entries).unwrap();
        assert_eq!(
            json,
            r#"[{"gitUrl":"git@github.com:acme/api.git","path":"acme/api"},{"gitUrl":null,"path":"solo"}]"#
        );
    }

    // A repo with no remote is exported with a null rather than dropped. The
    // whole reason to ask for the fleet as JSON is to find the odd one out.
    #[test]
    fn a_repo_with_no_remote_still_appears() {
        let entries = entries(Path::new("/g"), [Path::new("/g/solo")], |_| None);
        assert_eq!(
            entries,
            vec![Entry {
                git_url: None,
                path: "solo".into()
            }]
        );
    }

    #[test]
    fn an_empty_fleet_is_an_empty_array_not_null() {
        let entries = entries(Path::new("/g"), [], |_| None);
        assert_eq!(serde_json::to_string(&entries).unwrap(), "[]");
    }

    // Standing inside a single checkout is the common case for this command,
    // and it must not export a repo whose path is the empty string.
    #[test]
    fn the_root_repo_exports_under_its_own_name() {
        let entries = entries(Path::new("/g/acme/api"), [Path::new("/g/acme/api")], |_| {
            Some("git@github.com:acme/api.git".into())
        });
        assert_eq!(entries[0].path, "api");
    }
}
