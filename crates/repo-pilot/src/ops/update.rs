//! `repo-pilot update` — bring every default branch to latest without
//! disturbing the branch you were working on.
//!
//! The whole design problem here is that uncommitted work must survive a
//! sequence that moves HEAD twice. So every step that can fail is asked about
//! before it is attempted, the restore path runs whether or not the middle
//! succeeded, and nothing is ever discarded: a stash that will not pop is left
//! in the stash list with its ref printed, because a conflict you have to
//! resolve by hand is a far better outcome than work that quietly went away.
//!
//! The pull is `--ff-only` on purpose. A default branch that has diverged from
//! its remote is a situation with no safe automatic answer, and a merge commit
//! invented by a fleet sweep across two hundred repos is the least safe answer
//! available. It is reported and skipped.

use anyhow::Result;
use std::io::IsTerminal;
use std::path::Path;
use std::time::Duration;

use crate::config::Config;
use crate::git;
use crate::ops;

/// Local git commands are local. Nothing here should take this long, and if it
/// does the repo has a problem this sweep isn't going to fix.
const LOCAL: Duration = Duration::from_secs(30);

/// Whether to rebase the working branch onto the freshly-updated default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rebase {
    /// Ask, per repo. The default, and what a human at a terminal wants.
    Ask,
    /// Rebase every repo that has somewhere to rebase onto.
    Always,
    /// Never offer it. What a script wants.
    Never,
}

impl std::str::FromStr for Rebase {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ask" => Ok(Rebase::Ask),
            "always" | "yes" => Ok(Rebase::Always),
            "never" | "no" => Ok(Rebase::Never),
            other => Err(format!("expected ask, always or never, got {other:?}")),
        }
    }
}

/// What HEAD is doing, which is most of what decides whether there is any work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Head {
    Branch(String),
    Detached,
}

/// The decided course of action for one repo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// Nothing to do, and the reason to print.
    Skip(String),
    /// Already on the default branch. Pull in place: no checkout, and nothing
    /// to rebase afterwards.
    InPlace { default: String },
    /// On a working branch. Go to the default, pull, come back.
    RoundTrip { working: String, default: String },
}

/// Decide what to do with a repo from what git said about it.
///
/// Pure, because this is where every "handle it gracefully" case lives and
/// none of them should need a real repo — let alone a network — to test.
pub fn plan(head: &Head, has_origin: bool, default: Option<&str>) -> Plan {
    if !has_origin {
        return Plan::Skip("no origin remote".into());
    }
    let Some(default) = default else {
        return Plan::Skip("could not tell which branch is the default".into());
    };
    match head {
        // Nothing to check back out to. Moving HEAD here would strand whatever
        // commit the user is sitting on, which is exactly the kind of thing
        // people detach HEAD in order to look at.
        Head::Detached => Plan::Skip("detached HEAD".into()),
        Head::Branch(b) if b == default => Plan::InPlace {
            default: default.to_string(),
        },
        Head::Branch(b) => Plan::RoundTrip {
            working: b.clone(),
            default: default.to_string(),
        },
    }
}

/// Turn whatever `origin/HEAD` resolved to into a branch name.
///
/// Not `rsplit('/')`: `origin/release/1.0` is one branch called `release/1.0`,
/// and a fleet tool that decided the default branch was `1.0` would check out
/// something that doesn't exist on most of the fleet and nothing at all on the
/// rest.
pub fn strip_origin(head: &str) -> Option<&str> {
    let name = head
        .strip_prefix("refs/remotes/origin/")
        .or_else(|| head.strip_prefix("origin/"))?;
    (!name.is_empty() && name != "HEAD").then_some(name)
}

/// Pick the default branch: what `origin/HEAD` says if git knows, and
/// otherwise the first conventional name that actually exists on the remote.
///
/// `origin/HEAD` is only set by a clone, so repos created locally and pushed
/// afterwards routinely don't have it. That is common enough that falling back
/// matters more than it sounds.
pub fn pick_default_branch(
    origin_head: Option<&str>,
    remote_branches: &[String],
) -> Option<String> {
    if let Some(name) = origin_head.and_then(strip_origin) {
        return Some(name.to_string());
    }
    ["main", "master", "develop", "trunk"]
        .into_iter()
        .find(|c| remote_branches.iter().any(|b| b == c))
        .map(str::to_string)
}

/// Whether `git stash push` actually stashed anything.
///
/// Not read from the command's output, which is prose and localised. The stash
/// ref moving is the fact; everything else is a description of it. Getting this
/// wrong in the "no" direction pops somebody else's stash onto their tree.
pub fn stash_was_created(before: Option<&str>, after: Option<&str>) -> bool {
    match (before, after) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(b), Some(a)) => b != a,
    }
}

/// How one repo turned out.
struct Outcome {
    mark: char,
    note: String,
    kind: Kind,
}

enum Kind {
    Done,
    Skipped,
    Failed,
}

impl Outcome {
    fn done(note: impl Into<String>) -> Self {
        Self {
            mark: '✓',
            note: note.into(),
            kind: Kind::Done,
        }
    }
    fn skipped(note: impl Into<String>) -> Self {
        Self {
            mark: '·',
            note: format!("skipped: {}", note.into()),
            kind: Kind::Skipped,
        }
    }
    fn failed(note: impl Into<String>) -> Self {
        Self {
            mark: '✗',
            note: note.into(),
            kind: Kind::Failed,
        }
    }
}

pub async fn run(
    root: &Path,
    cfg: &Config,
    rebase: Rebase,
    pull_timeout: Duration,
) -> Result<bool> {
    let repos = ops::fleet(root, cfg)?;
    if repos.is_empty() {
        eprintln!("No repos below {}.", ops::relative_path(root, root));
        return Ok(true);
    }

    // A prompt nobody can answer is a hang, not a question.
    let interactive = std::io::stdin().is_terminal();
    let rebase = match (rebase, interactive) {
        (Rebase::Ask, false) => {
            eprintln!("repo-pilot: stdin is not a terminal, so --rebase=ask means never.");
            Rebase::Never
        }
        (r, _) => r,
    };

    let width = repos
        .iter()
        .map(|r| ops::relative_path(root, &r.root).chars().count())
        .max()
        .unwrap_or(0)
        .min(48);

    let mut tally = ops::Tally::default();
    for repo in &repos {
        let name = ops::relative_path(root, &repo.root);
        let outcome = update_one(&repo.root, rebase, pull_timeout)
            .await
            .unwrap_or_else(|err| Outcome::failed(err.to_string()));
        println!("{} {:width$}  {}", outcome.mark, name, outcome.note);
        match outcome.kind {
            Kind::Done => tally.done += 1,
            Kind::Skipped => tally.skipped += 1,
            Kind::Failed => tally.failed += 1,
        }
    }
    Ok(tally.report("updated"))
}

async fn update_one(repo: &Path, rebase: Rebase, pull_timeout: Duration) -> Result<Outcome> {
    let head = head_state(repo).await?;
    let has_origin = git::query(repo, &["config", "--get", "remote.origin.url"])
        .await?
        .ok;
    let default = detect_default_branch(repo).await?;

    let (working, default) = match plan(&head, has_origin, default.as_deref()) {
        Plan::Skip(why) => return Ok(Outcome::skipped(why)),
        Plan::InPlace { default } => (None, default),
        Plan::RoundTrip { working, default } => (Some(working), default),
    };

    let mut notes: Vec<String> = Vec::new();

    // --- stash, if there is anything to stash -------------------------------
    let stash_before = stash_ref(repo).await?;
    let dirty = !git::query(repo, &["status", "--porcelain"])
        .await?
        .stdout
        .is_empty();
    let mut stashed = false;
    if dirty {
        let label = working.as_deref().unwrap_or(&default);
        let push = git::run_write(
            repo,
            &[
                "stash",
                "push",
                "--include-untracked",
                "--message",
                &format!("repo-pilot: update on {label}"),
            ],
            LOCAL,
        )
        .await?;
        if !push.ok {
            return Ok(Outcome::failed(format!(
                "could not stash local changes, nothing touched: {}",
                push.why()
            )));
        }
        stashed = stash_was_created(stash_before.as_deref(), stash_ref(repo).await?.as_deref());
        if stashed {
            notes.push("stashed".into());
        }
    }

    // --- go to the default branch, pull, come back --------------------------
    // From here on the restore path runs whatever happens, so failures are
    // recorded rather than returned.
    let mut moved = false;
    let mut failure: Option<String> = None;

    if working.is_some() {
        let checkout = git::run_write(repo, &["checkout", "--quiet", &default], LOCAL).await?;
        if checkout.ok {
            moved = true;
        } else {
            failure = Some(format!("could not check out {default}: {}", checkout.why()));
        }
    }

    if failure.is_none() {
        let before = rev(repo, "HEAD").await?;
        let pull = git::run_write(
            repo,
            &["pull", "--ff-only", "--quiet", "origin", &default],
            pull_timeout,
        )
        .await?;
        if pull.ok {
            let gained = count_between(repo, before.as_deref(), "HEAD").await?;
            notes.push(match gained {
                0 => format!("{default} already up to date"),
                n => format!("{default} +{n}"),
            });
        } else {
            failure = Some(format!("could not fast-forward {default}: {}", pull.why()));
        }
    }

    // --- restore: branch first, then the stash ------------------------------
    if let (Some(working), true) = (working.as_deref(), moved) {
        let back = git::run_write(repo, &["checkout", "--quiet", working], LOCAL).await?;
        if !back.ok {
            // Leaving someone on the wrong branch with their work in a stash is
            // the worst state this can produce, so it is loud and the stash is
            // deliberately left alone rather than popped onto the wrong tree.
            return Ok(Outcome::failed(format!(
                "LEFT ON {default}: could not return to {working} ({}){}",
                back.why(),
                if stashed {
                    "; your changes are in the stash, unpopped"
                } else {
                    ""
                }
            )));
        }
    }

    if stashed {
        let pop = git::run_write(repo, &["stash", "pop"], LOCAL).await?;
        if pop.ok {
            notes.push("stash restored".into());
        } else {
            return Ok(Outcome::failed(format!(
                "changes are safe in the stash but would not pop: {}. Resolve with `git -C {} stash pop`",
                pop.why(),
                repo.display()
            )));
        }
    }

    if let Some(failure) = failure {
        return Ok(Outcome::failed(failure));
    }

    // --- and only now, the rebase -------------------------------------------
    if let Some(working) = working.as_deref() {
        if let Some(note) = rebase_note(repo, working, &default, rebase).await? {
            notes.push(note);
        }
    }

    Ok(Outcome::done(notes.join(", ")))
}

/// Offer, and possibly perform, the rebase. Returns the note to print.
async fn rebase_note(
    repo: &Path,
    working: &str,
    default: &str,
    rebase: Rebase,
) -> Result<Option<String>> {
    if rebase == Rebase::Never {
        return Ok(None);
    }
    // A rebase refuses to start on a dirty tree, and we may have just restored
    // one. Saying so beats offering a choice that could only fail.
    if !git::query(repo, &["status", "--porcelain"])
        .await?
        .stdout
        .is_empty()
    {
        return Ok(Some("rebase not offered: working tree has changes".into()));
    }
    if count_between(repo, Some(working), default).await? == 0 {
        return Ok(None);
    }
    if rebase == Rebase::Ask && !confirm(&format!("  rebase {working} onto {default}?")).await? {
        return Ok(Some("rebase declined".into()));
    }
    let out = git::run_write(repo, &["rebase", "--quiet", default], LOCAL).await?;
    if out.ok {
        return Ok(Some(format!("rebased onto {default}")));
    }
    // Never leave a half-finished rebase behind for the next repo's sake.
    let _ = git::run_write(repo, &["rebase", "--abort"], LOCAL).await;
    Ok(Some(format!(
        "rebase failed and was aborted: {}",
        out.why()
    )))
}

/// A y/N question on stdin. Blocking, on a blocking thread, because the whole
/// sweep is waiting for the answer anyway.
async fn confirm(question: &str) -> Result<bool> {
    let question = question.to_string();
    Ok(tokio::task::spawn_blocking(move || {
        use std::io::{BufRead, Write};
        print!("{question} [y/N] ");
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        if std::io::stdin().lock().read_line(&mut line).unwrap_or(0) == 0 {
            return false;
        }
        matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
    })
    .await?)
}

async fn head_state(repo: &Path) -> Result<Head> {
    let out = git::query(repo, &["symbolic-ref", "--quiet", "--short", "HEAD"]).await?;
    Ok(if out.ok && !out.stdout.is_empty() {
        Head::Branch(out.stdout)
    } else {
        Head::Detached
    })
}

async fn detect_default_branch(repo: &Path) -> Result<Option<String>> {
    let origin_head = git::query(
        repo,
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    )
    .await?;
    let origin_head = origin_head.ok.then_some(origin_head.stdout);

    let listed = git::query(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname:lstrip=3)",
            "refs/remotes/origin/",
        ],
    )
    .await?;
    let remote_branches: Vec<String> = listed
        .stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();

    Ok(pick_default_branch(
        origin_head.as_deref(),
        &remote_branches,
    ))
}

async fn stash_ref(repo: &Path) -> Result<Option<String>> {
    let out = git::query(repo, &["rev-parse", "--verify", "--quiet", "refs/stash"]).await?;
    Ok((out.ok && !out.stdout.is_empty()).then_some(out.stdout))
}

async fn rev(repo: &Path, spec: &str) -> Result<Option<String>> {
    let out = git::query(repo, &["rev-parse", "--verify", "--quiet", spec]).await?;
    Ok((out.ok && !out.stdout.is_empty()).then_some(out.stdout))
}

/// How many commits `to` has that `from` does not.
async fn count_between(repo: &Path, from: Option<&str>, to: &str) -> Result<u32> {
    let Some(from) = from else { return Ok(0) };
    let out = git::query(repo, &["rev-list", "--count", &format!("{from}..{to}")]).await?;
    Ok(out.stdout.trim().parse().unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn branch(name: &str) -> Head {
        Head::Branch(name.into())
    }

    #[test]
    fn a_working_branch_gets_the_round_trip() {
        assert_eq!(
            plan(&branch("feature/x"), true, Some("main")),
            Plan::RoundTrip {
                working: "feature/x".into(),
                default: "main".into()
            }
        );
    }

    // Already on the default: pulling in place is both correct and cheaper,
    // and there is nothing to offer a rebase onto.
    #[test]
    fn being_on_the_default_branch_skips_the_round_trip() {
        assert_eq!(
            plan(&branch("main"), true, Some("main")),
            Plan::InPlace {
                default: "main".into()
            }
        );
    }

    #[test]
    fn the_ungraceful_cases_are_skips_and_not_failures() {
        assert!(matches!(
            plan(&branch("main"), false, Some("main")),
            Plan::Skip(_)
        ));
        assert!(matches!(
            plan(&Head::Detached, true, Some("main")),
            Plan::Skip(_)
        ));
        assert!(matches!(plan(&branch("x"), true, None), Plan::Skip(_)));
    }

    // A repo with no remote is skipped before the default branch is even
    // considered: there is nothing to pull from, whatever the branches say.
    #[test]
    fn no_remote_beats_every_other_consideration() {
        assert_eq!(
            plan(&Head::Detached, false, None),
            Plan::Skip("no origin remote".into())
        );
    }

    #[test]
    fn origin_head_names_the_default_branch() {
        assert_eq!(
            pick_default_branch(Some("origin/main"), &[]).as_deref(),
            Some("main")
        );
        assert_eq!(
            pick_default_branch(Some("refs/remotes/origin/master"), &[]).as_deref(),
            Some("master")
        );
    }

    // A default branch with a slash in it is one branch, not two.
    #[test]
    fn a_slashed_default_branch_survives_intact() {
        assert_eq!(strip_origin("origin/release/1.0"), Some("release/1.0"));
        assert_eq!(
            pick_default_branch(Some("origin/release/1.0"), &[]).as_deref(),
            Some("release/1.0")
        );
    }

    #[test]
    fn a_useless_origin_head_is_not_mistaken_for_a_branch() {
        assert_eq!(strip_origin("origin/HEAD"), None);
        assert_eq!(strip_origin("origin/"), None);
        assert_eq!(strip_origin("main"), None);
    }

    // Repos created locally and pushed afterwards have no `origin/HEAD` at
    // all, which is common enough that guessing has to work.
    #[test]
    fn without_origin_head_the_conventional_names_are_tried_in_order() {
        let remotes = |names: &[&str]| names.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            pick_default_branch(None, &remotes(&["master", "topic"])).as_deref(),
            Some("master")
        );
        // main wins over master when a repo somehow has both.
        assert_eq!(
            pick_default_branch(None, &remotes(&["master", "main"])).as_deref(),
            Some("main")
        );
        assert_eq!(pick_default_branch(None, &remotes(&["topic"])), None);
        assert_eq!(pick_default_branch(None, &[]), None);
    }

    // The direction that matters: a `stash push` that stashed nothing must
    // never be followed by a `stash pop`, which would drop an unrelated stash
    // onto the working tree.
    #[test]
    fn a_stash_that_did_nothing_is_not_reported_as_created() {
        assert!(!stash_was_created(Some("abc"), Some("abc")));
        assert!(!stash_was_created(None, None));
        assert!(stash_was_created(None, Some("abc")));
        assert!(stash_was_created(Some("abc"), Some("def")));
    }

    #[test]
    fn rebase_mode_parses_the_words_people_type() {
        use std::str::FromStr;
        assert_eq!(Rebase::from_str("ask").unwrap(), Rebase::Ask);
        assert_eq!(Rebase::from_str("always").unwrap(), Rebase::Always);
        assert_eq!(Rebase::from_str("never").unwrap(), Rebase::Never);
        assert!(Rebase::from_str("maybe").is_err());
    }

    // -----------------------------------------------------------------------
    // The round trip itself, against real repos. A bare repo on disk stands in
    // for the remote, so these exercise clone, fetch and pull without touching
    // a network.
    // -----------------------------------------------------------------------

    fn git(cwd: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .current_dir(cwd)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@e")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@e")
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
    }

    fn out(cwd: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .current_dir(cwd)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// An upstream with two commits on `main`, and a clone parked one commit
    /// behind it. Returns the tempdir and the clone's path.
    fn behind_clone() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        git(
            base,
            &["init", "-q", "--bare", "-b", "main", "upstream.git"],
        );

        git(base, &["clone", "-q", "upstream.git", "seed"]);
        let seed = base.join("seed");
        std::fs::write(seed.join("a.txt"), "one").unwrap();
        git(&seed, &["add", "-A"]);
        git(&seed, &["commit", "-qm", "one"]);
        git(&seed, &["push", "-q", "-u", "origin", "main"]);
        std::fs::write(seed.join("b.txt"), "two").unwrap();
        git(&seed, &["add", "-A"]);
        git(&seed, &["commit", "-qm", "two"]);
        git(&seed, &["push", "-q", "origin", "main"]);

        git(base, &["clone", "-q", "upstream.git", "work"]);
        let work = base.join("work");
        git(&work, &["reset", "-q", "--hard", "HEAD~1"]);
        (dir, work)
    }

    /// The promise the whole command rests on: uncommitted work is exactly
    /// where it was, on the branch it was on, with nothing left in the stash.
    #[tokio::test]
    async fn the_round_trip_returns_the_branch_and_the_uncommitted_work() {
        let (_dir, repo) = behind_clone();
        git(&repo, &["checkout", "-q", "-b", "feature/x"]);
        std::fs::write(repo.join("a.txt"), "edited").unwrap();
        std::fs::write(repo.join("untracked.txt"), "scratch").unwrap();

        let outcome = update_one(&repo, Rebase::Never, LOCAL).await.unwrap();
        assert!(matches!(outcome.kind, Kind::Done), "{}", outcome.note);

        assert_eq!(
            out(&repo, &["symbolic-ref", "--short", "HEAD"]),
            "feature/x"
        );
        assert_eq!(
            std::fs::read_to_string(repo.join("a.txt")).unwrap(),
            "edited"
        );
        assert!(
            repo.join("untracked.txt").exists(),
            "untracked file restored"
        );
        assert_eq!(out(&repo, &["stash", "list"]), "", "nothing left stashed");
        // And the point of the exercise: main actually moved.
        assert_eq!(
            out(&repo, &["rev-parse", "main"]),
            out(&repo, &["rev-parse", "origin/main"])
        );
    }

    #[tokio::test]
    async fn a_clean_repo_on_the_default_branch_is_pulled_in_place() {
        let (_dir, repo) = behind_clone();
        let outcome = update_one(&repo, Rebase::Never, LOCAL).await.unwrap();
        assert!(matches!(outcome.kind, Kind::Done), "{}", outcome.note);
        assert_eq!(out(&repo, &["symbolic-ref", "--short", "HEAD"]), "main");
        assert_eq!(
            out(&repo, &["rev-parse", "main"]),
            out(&repo, &["rev-parse", "origin/main"])
        );
    }

    // A default branch that has diverged has no safe automatic answer. What
    // matters is that refusing it still puts everything back.
    #[tokio::test]
    async fn a_diverged_default_branch_fails_without_losing_anything() {
        let (_dir, repo) = behind_clone();
        std::fs::write(repo.join("local.txt"), "local").unwrap();
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-qm", "a local commit on main"]);
        let diverged_main = out(&repo, &["rev-parse", "main"]);

        git(&repo, &["checkout", "-q", "-b", "feature/z"]);
        std::fs::write(repo.join("wip.txt"), "wip").unwrap();

        let outcome = update_one(&repo, Rebase::Never, LOCAL).await.unwrap();
        assert!(matches!(outcome.kind, Kind::Failed), "{}", outcome.note);

        assert_eq!(
            out(&repo, &["symbolic-ref", "--short", "HEAD"]),
            "feature/z"
        );
        assert!(
            repo.join("wip.txt").exists(),
            "the untracked file came back"
        );
        assert_eq!(out(&repo, &["stash", "list"]), "", "nothing left stashed");
        // No merge commit invented on the user's behalf.
        assert_eq!(out(&repo, &["rev-parse", "main"]), diverged_main);
    }

    #[tokio::test]
    async fn a_detached_head_and_a_remoteless_repo_are_skipped_not_touched() {
        let (_dir, repo) = behind_clone();
        let before = out(&repo, &["rev-parse", "HEAD"]);
        git(&repo, &["checkout", "-q", "--detach", "HEAD"]);
        let outcome = update_one(&repo, Rebase::Never, LOCAL).await.unwrap();
        assert!(matches!(outcome.kind, Kind::Skipped), "{}", outcome.note);
        assert_eq!(out(&repo, &["rev-parse", "HEAD"]), before);

        let solo = tempfile::tempdir().unwrap();
        git(solo.path(), &["init", "-q", "-b", "main", "."]);
        std::fs::write(solo.path().join("x"), "x").unwrap();
        git(solo.path(), &["add", "-A"]);
        git(solo.path(), &["commit", "-qm", "x"]);
        let outcome = update_one(solo.path(), Rebase::Never, LOCAL).await.unwrap();
        assert!(matches!(outcome.kind, Kind::Skipped), "{}", outcome.note);
    }

    #[tokio::test]
    async fn rebase_always_replays_the_working_branch_onto_the_new_default() {
        let (_dir, repo) = behind_clone();
        git(&repo, &["checkout", "-q", "-b", "feature/y"]);
        std::fs::write(repo.join("mine.txt"), "mine").unwrap();
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-qm", "my work"]);

        let outcome = update_one(&repo, Rebase::Always, LOCAL).await.unwrap();
        assert!(matches!(outcome.kind, Kind::Done), "{}", outcome.note);
        assert_eq!(
            out(&repo, &["symbolic-ref", "--short", "HEAD"]),
            "feature/y"
        );
        // The work now sits directly on top of the updated default branch.
        assert_eq!(
            out(&repo, &["rev-parse", "HEAD~1"]),
            out(&repo, &["rev-parse", "main"])
        );
        assert_eq!(out(&repo, &["log", "-1", "--format=%s"]), "my work");
    }

    // The rebase is offered only when it could succeed. A tree still holding
    // restored changes cannot be rebased, and saying so beats a guaranteed
    // failure dressed up as a question.
    #[tokio::test]
    async fn a_dirty_tree_is_told_why_the_rebase_is_not_on_offer() {
        let (_dir, repo) = behind_clone();
        git(&repo, &["checkout", "-q", "-b", "feature/x"]);
        std::fs::write(repo.join("a.txt"), "edited").unwrap();

        let outcome = update_one(&repo, Rebase::Always, LOCAL).await.unwrap();
        assert!(matches!(outcome.kind, Kind::Done), "{}", outcome.note);
        assert!(
            outcome.note.contains("rebase not offered"),
            "expected a reason, got {:?}",
            outcome.note
        );
    }
}
