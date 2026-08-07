//! Just enough git to know where else a server could run from.
//!
//! `git worktree list` reports every worktree of a repository regardless of
//! which one you ask from, so a server pointed at any worktree already knows
//! about all its siblings — including the main one. Nothing here is cached or
//! polled; it runs when the user opens the picker.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub path: PathBuf,
    /// Branch name, or a short commit for a detached HEAD.
    pub branch: String,
    /// The repository's main worktree — "back to base".
    pub is_main: bool,
    /// Where the server currently points.
    pub is_current: bool,
}

/// Every worktree of the repository containing `dir`. Empty when `dir` isn't
/// a git repository, which is the signal to hide the picker entirely.
pub fn worktrees(dir: &Path) -> Vec<Worktree> {
    let Ok(output) = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(dir)
        .args(["worktree", "list", "--porcelain"])
        .stderr(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    parse_worktrees(
        &String::from_utf8_lossy(&output.stdout),
        dir.canonicalize().ok(),
    )
}

/// Read `git worktree list --porcelain`. Records are separated by a blank
/// line; a trailing empty line flushes the last one.
fn parse_worktrees(stdout: &str, here: Option<PathBuf>) -> Vec<Worktree> {
    let mut found: Vec<Worktree> = Vec::new();
    let mut path: Option<PathBuf> = None;
    let mut branch = String::new();

    for line in stdout.lines().chain(std::iter::once("")) {
        if let Some(rest) = line.strip_prefix("worktree ") {
            path = Some(PathBuf::from(rest));
            branch.clear();
        } else if let Some(rest) = line.strip_prefix("branch refs/heads/") {
            branch = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("HEAD ") {
            // Only used if the worktree turns out to be detached.
            if branch.is_empty() {
                branch = rest.chars().take(7).collect();
            }
        } else if line.is_empty() {
            if let Some(p) = path.take() {
                // Compare canonical paths where we can; fall back to the raw
                // path so a worktree git knows about but we cannot stat is
                // still matched.
                let resolved = p.canonicalize().unwrap_or_else(|_| p.clone());
                let is_current = here.as_ref().is_some_and(|h| *h == resolved);
                found.push(Worktree {
                    is_main: found.is_empty(), // git always lists the main one first
                    branch: std::mem::take(&mut branch),
                    is_current,
                    path: p,
                });
            }
        }
    }

    // Git lists in its own order, which is unscannable once a repo has a dozen
    // worktrees. Sort the rest the way a person reads them, and keep the main
    // worktree pinned at the top as the anchor to return to.
    let main = if found.is_empty() {
        None
    } else {
        Some(found.remove(0))
    };
    found.sort_by(|a, b| natural_cmp(&a.branch, &b.branch));
    if let Some(main) = main {
        found.insert(0, main);
    }
    found
}

/// Compare the way a person reads: runs of digits compare numerically, so
/// feat-64 sorts before feat-522 rather than after it.
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0, 0);

    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let start_a = i;
            let start_b = j;
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            // Compare by value, falling back to text for numbers too long to fit.
            let na = std::str::from_utf8(&a[start_a..i])
                .unwrap_or("")
                .parse::<u128>();
            let nb = std::str::from_utf8(&b[start_b..j])
                .unwrap_or("")
                .parse::<u128>();
            match (na, nb) {
                (Ok(x), Ok(y)) if x != y => return x.cmp(&y),
                (Ok(_), Ok(_)) => {}
                _ => return a[start_a..i].cmp(&b[start_b..j]),
            }
        } else {
            let (x, y) = (a[i].to_ascii_lowercase(), b[j].to_ascii_lowercase());
            if x != y {
                return x.cmp(&y);
            }
            i += 1;
            j += 1;
        }
    }
    match (a.len() - i, b.len() - j) {
        (0, 0) => Ordering::Equal,
        (0, _) => Ordering::Less,
        _ => Ordering::Greater,
    }
}

#[cfg(test)]
mod tests {
    use super::natural_cmp;
    use std::cmp::Ordering;

    #[test]
    fn digit_runs_compare_by_value() {
        assert_eq!(natural_cmp("feat-64-a", "feat-522-a"), Ordering::Less);
        assert_eq!(natural_cmp("feat-617-x", "feat-585-y"), Ordering::Greater);
    }

    #[test]
    fn text_compares_case_insensitively() {
        assert_eq!(natural_cmp("feat-agent", "feat-464"), Ordering::Greater);
        assert_eq!(natural_cmp("main", "MAIN"), Ordering::Equal);
    }

    const PORCELAIN: &str = "\
worktree /repo
HEAD abc123def4567890
branch refs/heads/main

worktree /repo/.wt/feat-64
HEAD 1111111111111111
branch refs/heads/feat-64

worktree /repo/.wt/feat-522
HEAD 2222222222222222
branch refs/heads/feat-522

worktree /repo/.wt/detached
HEAD 9876543210abcdef

";

    #[test]
    fn keeps_the_main_worktree_first_and_sorts_the_rest() {
        let found = super::parse_worktrees(PORCELAIN, None);
        let branches: Vec<&str> = found.iter().map(|w| w.branch.as_str()).collect();
        // main is pinned; 64 sorts before 522 by value, not by text.
        assert_eq!(branches, vec!["main", "9876543", "feat-64", "feat-522"]);
        assert!(found[0].is_main);
        assert!(!found[1].is_main);
    }

    /// A detached worktree has no branch line, so it is named by a short commit.
    #[test]
    fn names_a_detached_head_by_its_commit() {
        let found = super::parse_worktrees(PORCELAIN, None);
        let detached = found.iter().find(|w| w.path.ends_with("detached")).unwrap();
        assert_eq!(detached.branch, "9876543");
    }

    #[test]
    fn marks_the_worktree_we_asked_from_as_current() {
        let here = Some(std::path::PathBuf::from("/repo/.wt/feat-64"));
        let found = super::parse_worktrees(PORCELAIN, here);
        let current: Vec<&str> = found
            .iter()
            .filter(|w| w.is_current)
            .map(|w| w.branch.as_str())
            .collect();
        assert_eq!(current, vec!["feat-64"]);
    }

    /// Not a git repository at all — the caller uses this to hide the picker.
    #[test]
    fn yields_nothing_for_empty_output() {
        assert!(super::parse_worktrees("", None).is_empty());
    }
}
