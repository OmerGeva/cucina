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

    let here = dir.canonicalize().ok();
    let mut found: Vec<Worktree> = Vec::new();
    let mut path: Option<PathBuf> = None;
    let mut branch = String::new();

    // Records are separated by a blank line; a trailing empty push flushes the
    // last one.
    for line in String::from_utf8_lossy(&output.stdout)
        .lines()
        .chain(std::iter::once(""))
    {
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
                let is_current = here
                    .as_ref()
                    .zip(p.canonicalize().ok())
                    .map(|(a, b)| *a == b)
                    .unwrap_or(false);
                found.push(Worktree {
                    is_main: found.is_empty(), // git always lists the main one first
                    branch: std::mem::take(&mut branch),
                    is_current,
                    path: p,
                });
            }
        }
    }
    found
}
