//! Faz 23: "Open Terminal Here" launch engine.
//!
//! Resolves the user's preferred terminal emulator through a strict
//! priority hierarchy (Rule #25 — never hardcode a single terminal) and
//! launches it with the target directory as the child process's working
//! directory via `Command::current_dir` (Rule #19 — never build a shell
//! string from a path). Priority order:
//!
//! 1. `xdg-terminal-exec` (freedesktop.org's terminal launch standard).
//! 2. `$TERMINAL` (user's shell-level preference).
//! 3. The GIO-registered desktop application advertising the
//!    `TerminalEmulator` category (the XDG default terminal).
//! 4. A fixed list of common terminal binaries found on `$PATH`.
//!
//! Every resolvable candidate (not just the first) is kept, so a stale
//! `$TERMINAL` pointing at an uninstalled binary or an `xdg-terminal-exec`
//! that exists but fails to actually launch a terminal doesn't strand the
//! user — [`open_terminal`] falls through to the next candidate instead.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use gio::prelude::*;

use veyra_filesystem::VeyraPath;

/// Common terminal emulator binaries checked on `$PATH` as the last-resort
/// fallback tier, in the order they're tried.
const KNOWN_TERMINALS: &[&str] = &[
    "ptyxis",
    "gnome-terminal",
    "konsole",
    "kitty",
    "alacritty",
    "wezterm",
    "foot",
    "ghostty",
    "xfce4-terminal",
    "mate-terminal",
    "terminator",
    "tilix",
    "urxvt",
    "xterm",
];

/// A resolved, launchable terminal: an executable found to actually exist
/// (never a raw, unvalidated string), plus any argument tokens that came
/// with it (e.g. extra words in `$TERMINAL`).
struct Candidate {
    program: OsString,
    args: Vec<OsString>,
}

/// Faz 28: A resolved terminal's program and arguments, used by privileged
/// operations (e.g. pkexec) to invoke the same terminal as root.
pub(crate) struct ResolvedTerminal {
    pub program: OsString,
    pub args: Vec<OsString>,
}

#[derive(Debug, thiserror::Error)]
pub enum TerminalError {
    #[error("a terminal can't be opened at a non-local location")]
    NotLocal,
    #[error("no terminal emulator was found on this system")]
    NotFound,
    #[error("failed to launch terminal: {0}")]
    Spawn(#[source] std::io::Error),
}

/// Opens the user's preferred terminal at `path` (its parent directory if
/// `path` names a file) and returns once the process has been spawned.
/// Never runs a shell string — every candidate is an executable resolved to
/// exist on disk beforehand, launched with the target directory passed only
/// as `Command::current_dir`.
pub fn open_terminal(path: &VeyraPath) -> Result<(), TerminalError> {
    let dir = target_dir(path)?;
    let candidates = candidates();
    let mut last_err = None;
    for candidate in candidates {
        match Command::new(&candidate.program)
            .args(&candidate.args)
            .current_dir(&dir)
            .spawn()
        {
            Ok(_) => return Ok(()),
            Err(err) => last_err = Some(err),
        }
    }
    Err(match last_err {
        Some(err) => TerminalError::Spawn(err),
        None => TerminalError::NotFound,
    })
}

/// Faz 28: Resolves the user's preferred terminal to a concrete program and
/// arguments, without launching it. Returns `None` if no terminal is available.
/// Used by privileged operations (e.g. pkexec) to invoke the terminal as root.
pub(crate) fn resolve_terminal() -> Option<ResolvedTerminal> {
    let candidates = candidates();
    candidates.into_iter().next().map(|c| ResolvedTerminal {
        program: c.program,
        args: c.args,
    })
}

/// The directory a terminal should open in: `path` itself if it's a
/// directory, otherwise its parent. Remote/virtual locations (no local path)
/// are rejected outright — a terminal can't `cd` into an `sftp://` URI.
fn target_dir(path: &VeyraPath) -> Result<PathBuf, TerminalError> {
    let local = path.as_local_path().ok_or(TerminalError::NotLocal)?;
    if local.is_dir() {
        Ok(local.to_path_buf())
    } else {
        Ok(local
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| local.to_path_buf()))
    }
}

/// Gathers every resolvable terminal candidate, in priority order.
fn candidates() -> Vec<Candidate> {
    let mut found = Vec::new();
    found.extend(resolve_xdg_terminal_exec());
    found.extend(resolve_env_terminal());
    found.extend(resolve_gio_default_terminal());
    found.extend(resolve_known_terminals());
    found
}

/// Priority 1: `xdg-terminal-exec`, the freedesktop.org terminal launch
/// standard — when present, it already knows the user's real preference, so
/// every other tier exists only as a fallback for systems without it.
fn resolve_xdg_terminal_exec() -> Option<Candidate> {
    find_in_path("xdg-terminal-exec").map(|program| Candidate {
        program: program.into(),
        args: Vec::new(),
    })
}

/// Priority 2: `$TERMINAL`, the conventional shell-level terminal
/// preference. The first whitespace-separated token is resolved as the
/// binary; any remaining tokens are passed through as its own arguments
/// (never anything path-derived).
fn resolve_env_terminal() -> Option<Candidate> {
    let value = std::env::var("TERMINAL").ok()?;
    let mut tokens = value.split_whitespace();
    let bin = tokens.next()?;
    let program = find_in_path(bin)?;
    let args = tokens.map(OsString::from).collect();
    Some(Candidate {
        program: program.into(),
        args,
    })
}

/// Priority 3: the GIO/XDG-registered default terminal application — the
/// installed `.desktop` entry advertising the `TerminalEmulator` category.
/// Uses `AppInfo::executable`, which GIO resolves to the plain binary with
/// desktop-file placeholders (`%f`, `%u`, ...) already stripped, so nothing
/// here is built from an unsanitized string.
fn resolve_gio_default_terminal() -> Option<Candidate> {
    gio::AppInfo::all().into_iter().find_map(|app| {
        let desktop_app = app.downcast::<gio::DesktopAppInfo>().ok()?;
        let categories = desktop_app.categories()?;
        categories
            .split(';')
            .any(|category| category == "TerminalEmulator")
            .then(|| Candidate {
                program: desktop_app.executable().into_os_string(),
                args: Vec::new(),
            })
    })
}

/// Priority 4: the fixed [`KNOWN_TERMINALS`] fallback list, in order,
/// filtered down to the ones actually present on `$PATH`.
fn resolve_known_terminals() -> Vec<Candidate> {
    KNOWN_TERMINALS
        .iter()
        .filter_map(|name| {
            find_in_path(name).map(|program| Candidate {
                program: program.into(),
                args: Vec::new(),
            })
        })
        .collect()
}

/// Resolves `bin` to an existing, executable file: if it contains a path
/// separator it's checked as-is, otherwise every `$PATH` entry is searched
/// in order. Returns `None` rather than a guessed/unvalidated path so no
/// caller ever hands `Command::new` something that doesn't exist.
fn find_in_path(bin: &str) -> Option<PathBuf> {
    let dirs = std::env::var_os("PATH").map(|var| std::env::split_paths(&var).collect::<Vec<_>>());
    find_in_dirs(bin, dirs.unwrap_or_default())
}

/// Testable core of [`find_in_path`]: takes the search directories
/// explicitly instead of reading `$PATH`, so tests never need to mutate
/// real process-global environment state to exercise search order.
fn find_in_dirs(bin: &str, dirs: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    if bin.contains('/') {
        let candidate = PathBuf::from(bin);
        return is_executable_file(&candidate).then_some(candidate);
    }
    dirs.into_iter().find_map(|dir| {
        let candidate = dir.join(bin);
        is_executable_file(&candidate).then_some(candidate)
    })
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(_path: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_executable(path: &Path) {
        std::fs::write(path, b"#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    fn target_dir_of_a_directory_is_itself() {
        let tmp = tempfile::tempdir().unwrap();
        let path = VeyraPath::from_local(tmp.path());
        assert_eq!(target_dir(&path).unwrap(), tmp.path());
    }

    #[test]
    fn target_dir_of_a_file_is_its_parent() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("notes.txt");
        std::fs::write(&file, b"hi").unwrap();
        let path = VeyraPath::from_local(&file);
        assert_eq!(target_dir(&path).unwrap(), tmp.path());
    }

    #[test]
    fn target_dir_rejects_remote_locations() {
        let path = VeyraPath::from_uri("sftp://example.com/home/user");
        assert!(matches!(target_dir(&path), Err(TerminalError::NotLocal)));
    }

    #[test]
    fn find_in_path_locates_an_executable_by_absolute_path() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("myterm");
        make_executable(&bin);
        let found = find_in_path(bin.to_str().unwrap());
        assert_eq!(found.as_deref(), Some(bin.as_path()));
    }

    #[test]
    fn find_in_path_rejects_a_nonexistent_absolute_path() {
        let missing = std::env::temp_dir().join("veyra-terminal-test-does-not-exist");
        assert!(find_in_path(missing.to_str().unwrap()).is_none());
    }

    #[test]
    fn find_in_dirs_searches_entries_in_order() {
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();
        make_executable(&dir_b.path().join("myterm"));

        let found = find_in_dirs(
            "myterm",
            [dir_a.path().to_path_buf(), dir_b.path().to_path_buf()],
        );

        assert_eq!(
            found.as_deref(),
            Some(dir_b.path().join("myterm").as_path())
        );
    }

    #[test]
    fn find_in_dirs_returns_none_when_absent_from_every_entry() {
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();

        let found = find_in_dirs(
            "myterm",
            [dir_a.path().to_path_buf(), dir_b.path().to_path_buf()],
        );

        assert!(found.is_none());
    }

    #[test]
    fn known_terminals_list_has_no_duplicates() {
        let mut sorted = KNOWN_TERMINALS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), KNOWN_TERMINALS.len());
    }

    #[test]
    fn open_terminal_rejects_remote_locations_before_spawning() {
        let path = VeyraPath::from_uri("smb://server/share");
        assert!(matches!(open_terminal(&path), Err(TerminalError::NotLocal)));
    }
}
