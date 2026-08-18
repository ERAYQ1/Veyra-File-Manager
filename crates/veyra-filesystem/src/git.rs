//! Faz 39: Git repository detection and status, for the Developer Mode
//! headerbar badge.
//!
//! `find_git_root` is pure Rust — it just walks `path`'s ancestors looking
//! for a `.git` entry (a directory for a normal repo, a file for a worktree
//! or submodule), never touching a subprocess.
//!
//! `git_status` does shell out to the user's own `git`, but exactly once,
//! with a fixed argv (`Command::new("git").arg("-C").arg(root).args([...])`)
//! — never a shell string built from user input (Rule #19). Re-implementing
//! `git status` from the on-disk object/pack format would mean hand-rolling
//! pack-file delta decompression and ref resolution just to duplicate what
//! `git` itself already does correctly; shelling out to the same `git`
//! binary the user's repo already trusts is both safer and far more
//! correct. Both functions are blocking (a subprocess spawn + local disk
//! I/O) — callers run them off the GTK main thread via `fs_async::
//! run_blocking` (Rule #11/#14), same as every other `veyra-filesystem`
//! query.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Walks `path` (and its ancestors, if `path` doesn't itself exist or isn't
/// a directory) looking for a `.git` entry, returning the first directory
/// that has one. Returns `None` once the filesystem root is reached without
/// finding one — `path` isn't inside a Git repository (or working copy is
/// gone, Rule #17).
pub fn find_git_root(path: &Path) -> Option<PathBuf> {
    let mut dir = if path.is_dir() {
        Some(path)
    } else {
        path.parent()
    };
    while let Some(d) = dir {
        if d.join(".git").exists() {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}

/// A repository's status as of the last `git_status` call — never live
/// beyond that (no filesystem watch), so callers re-query on every
/// navigation, same as the rest of `veyra-filesystem`'s stat-on-demand
/// model.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitRepoStatus {
    /// The active branch name, or `"HEAD (detached)"` when not on a branch.
    pub branch: String,
    /// Local commits not yet pushed to the upstream branch.
    pub ahead: u32,
    /// Upstream commits not yet merged locally.
    pub behind: u32,
    /// Entries with a staged (index) change.
    pub staged: usize,
    /// Entries with an unstaged (worktree) change, including unresolved
    /// merge conflicts.
    pub modified: usize,
    /// Untracked entries.
    pub untracked: usize,
}

impl GitRepoStatus {
    /// Whether the working tree has any staged, modified, or untracked
    /// entries — the `*` in the headerbar badge.
    pub fn is_dirty(&self) -> bool {
        self.staged + self.modified + self.untracked > 0
    }

    /// Renders the compact headerbar badge, e.g. `main ↑2 ↓1 *`.
    pub fn badge_label(&self) -> String {
        let mut out = self.branch.clone();
        if self.ahead > 0 {
            out.push_str(&format!(" ↑{}", self.ahead));
        }
        if self.behind > 0 {
            out.push_str(&format!(" ↓{}", self.behind));
        }
        if self.is_dirty() {
            out.push_str(" *");
        }
        out
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GitStatusError {
    #[error("git is not installed or not on PATH")]
    NotFound,
    #[error("failed to run git: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("git exited with an error status")]
    CommandFailed,
}

/// Runs `git status --porcelain=2 --branch` at `repo_root` (a directory
/// `find_git_root` has already confirmed contains a `.git`) and parses the
/// result. Never touches the network (no fetch) — `ahead`/`behind` reflect
/// whatever the local upstream-tracking ref already knows, exactly like
/// plain `git status` itself.
pub fn git_status(repo_root: &Path) -> Result<GitRepoStatus, GitStatusError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["status", "--porcelain=2", "--branch"])
        .output()
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                GitStatusError::NotFound
            } else {
                GitStatusError::Spawn(err)
            }
        })?;

    if !output.status.success() {
        return Err(GitStatusError::CommandFailed);
    }

    Ok(parse_porcelain_v2(&String::from_utf8_lossy(&output.stdout)))
}

/// Parses `git status --porcelain=2 --branch` output. Format reference
/// (each on its own line):
/// - `# branch.head <name>` — `name` is `(detached)` when not on a branch.
/// - `# branch.ab +<ahead> -<behind>` — only present when an upstream is
///   configured.
/// - `1 <XY> ...` / `2 <XY> ...` — ordinary / renamed-or-copied changed
///   entries; `X` is the index (staged) status, `Y` the worktree status,
///   `.` meaning "no change" for that half.
/// - `u <XY> ...` — an unresolved merge conflict.
/// - `? <path>` — untracked.
fn parse_porcelain_v2(text: &str) -> GitRepoStatus {
    let mut status = GitRepoStatus {
        branch: "HEAD (detached)".to_string(),
        ..GitRepoStatus::default()
    };

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("# branch.head ") {
            status.branch = if rest == "(detached)" {
                "HEAD (detached)".to_string()
            } else {
                rest.to_string()
            };
        } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
            for token in rest.split_whitespace() {
                if let Some(n) = token.strip_prefix('+') {
                    status.ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = token.strip_prefix('-') {
                    status.behind = n.parse().unwrap_or(0);
                }
            }
        } else if let Some(rest) = line.strip_prefix("1 ").or_else(|| line.strip_prefix("2 ")) {
            count_xy(rest, &mut status);
        } else if line.starts_with("u ") {
            // An unresolved conflict is always worktree-visible.
            status.modified += 1;
        } else if line.starts_with("? ") {
            status.untracked += 1;
        }
    }

    status
}

/// Tallies one `1 <XY> ...` / `2 <XY> ...` entry's index/worktree halves
/// into `status`.
fn count_xy(rest: &str, status: &mut GitRepoStatus) {
    let mut chars = rest.chars();
    let x = chars.next().unwrap_or('.');
    let y = chars.next().unwrap_or('.');
    if x != '.' {
        status.staged += 1;
    }
    if y != '.' {
        status.modified += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .status()
            .expect("git must be on PATH to run these tests");
        assert!(status.success(), "git {args:?} failed");
    }

    fn init_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q", "-b", "main"]);
        git(dir.path(), &["config", "user.email", "test@veyra.local"]);
        git(dir.path(), &["config", "user.name", "Veyra Test"]);
        dir
    }

    fn commit_all(dir: &Path, message: &str) {
        git(dir, &["add", "-A"]);
        git(dir, &["commit", "-q", "-m", message]);
    }

    #[test]
    fn find_git_root_locates_the_top_level_dir() {
        let repo = init_repo();
        let nested = repo.path().join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        assert_eq!(find_git_root(&nested), Some(repo.path().to_path_buf()));
    }

    #[test]
    fn find_git_root_from_a_file_checks_its_parent() {
        let repo = init_repo();
        let file = repo.path().join("notes.txt");
        std::fs::write(&file, b"hi").unwrap();
        assert_eq!(find_git_root(&file), Some(repo.path().to_path_buf()));
    }

    #[test]
    fn find_git_root_returns_none_outside_any_repo() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(find_git_root(dir.path()), None);
    }

    #[test]
    fn status_on_a_clean_repo_reports_the_branch_and_no_dirt() {
        let repo = init_repo();
        std::fs::write(repo.path().join("readme.md"), b"hi").unwrap();
        commit_all(repo.path(), "initial");

        let status = git_status(repo.path()).unwrap();
        assert_eq!(status.branch, "main");
        assert!(!status.is_dirty());
        assert_eq!(status.ahead, 0);
        assert_eq!(status.behind, 0);
    }

    #[test]
    fn status_counts_staged_modified_and_untracked() {
        let repo = init_repo();
        std::fs::write(repo.path().join("a.txt"), b"a").unwrap();
        std::fs::write(repo.path().join("b.txt"), b"b").unwrap();
        commit_all(repo.path(), "initial");

        // Modify a tracked file (unstaged).
        std::fs::write(repo.path().join("a.txt"), b"a changed").unwrap();
        // Stage a new file.
        std::fs::write(repo.path().join("c.txt"), b"c").unwrap();
        git(repo.path(), &["add", "c.txt"]);
        // Leave an untracked file.
        std::fs::write(repo.path().join("d.txt"), b"d").unwrap();

        let status = git_status(repo.path()).unwrap();
        assert!(status.is_dirty());
        assert_eq!(status.staged, 1);
        assert_eq!(status.modified, 1);
        assert_eq!(status.untracked, 1);
    }

    #[test]
    fn status_on_detached_head_reports_it() {
        let repo = init_repo();
        std::fs::write(repo.path().join("a.txt"), b"a").unwrap();
        commit_all(repo.path(), "initial");
        git(repo.path(), &["checkout", "-q", "--detach", "HEAD"]);

        let status = git_status(repo.path()).unwrap();
        assert_eq!(status.branch, "HEAD (detached)");
    }

    #[test]
    fn status_reports_ahead_and_behind_against_upstream() {
        let origin = tempfile::tempdir().unwrap();
        git(origin.path(), &["init", "-q", "--bare", "-b", "main"]);

        let work = init_repo();
        std::fs::write(work.path().join("a.txt"), b"a").unwrap();
        commit_all(work.path(), "initial");
        git(
            work.path(),
            &["remote", "add", "origin", origin.path().to_str().unwrap()],
        );
        git(work.path(), &["push", "-q", "-u", "origin", "main"]);

        // One local commit not yet pushed: ahead 1.
        std::fs::write(work.path().join("b.txt"), b"b").unwrap();
        commit_all(work.path(), "second");

        let status = git_status(work.path()).unwrap();
        assert_eq!(status.ahead, 1);
        assert_eq!(status.behind, 0);

        // Push, then bring in a commit made from a second clone: behind 1.
        git(work.path(), &["push", "-q"]);
        let other = tempfile::tempdir().unwrap();
        git(
            other.path().parent().unwrap(),
            &[
                "clone",
                "-q",
                origin.path().to_str().unwrap(),
                other.path().to_str().unwrap(),
            ],
        );
        git(other.path(), &["config", "user.email", "test@veyra.local"]);
        git(other.path(), &["config", "user.name", "Veyra Test"]);
        std::fs::write(other.path().join("c.txt"), b"c").unwrap();
        commit_all(other.path(), "third");
        git(other.path(), &["push", "-q"]);
        git(work.path(), &["fetch", "-q"]);

        let status = git_status(work.path()).unwrap();
        assert_eq!(status.ahead, 0);
        assert_eq!(status.behind, 1);
    }

    #[test]
    fn badge_label_formats_clean_repo() {
        let status = GitRepoStatus {
            branch: "main".to_string(),
            ..GitRepoStatus::default()
        };
        assert_eq!(status.badge_label(), "main");
    }

    #[test]
    fn badge_label_formats_ahead_behind_and_dirty() {
        let status = GitRepoStatus {
            branch: "feature/auth".to_string(),
            ahead: 2,
            behind: 1,
            modified: 1,
            ..GitRepoStatus::default()
        };
        assert_eq!(status.badge_label(), "feature/auth ↑2 ↓1 *");
    }

    #[test]
    fn parse_empty_output_defaults_to_detached() {
        let status = parse_porcelain_v2("");
        assert_eq!(status.branch, "HEAD (detached)");
        assert!(!status.is_dirty());
    }
}
