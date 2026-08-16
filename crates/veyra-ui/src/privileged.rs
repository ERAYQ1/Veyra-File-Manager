//! Faz 28: Privileged operation escalation via Polkit's `pkexec` (Rule #20).
//!
//! Isolated privilege escalation for filesystem operations that require
//! elevated permissions: chmod, remove, move, copy, and open-terminal-as-root.
//! Every operation is spawned as a fixed, known coreutils binary with argv
//! built entirely from typed/validated inputs (paths, validated octal
//! permissions, booleans for recursion flags), never a raw user-typed string
//! interpolated into anything (Rule #19). pkexec prompts the user via an
//! interactive GUI (the Polkit authentication agent) and returns once the
//! operation is complete or the user cancels.
//!
//! **CRITICAL**: These functions are blocking (they spawn child processes
//! synchronously via `Command::status()`/`Command::output()`). They must
//! ONLY be called from `fs_async::run_blocking`'s worker closure on a
//! background thread, never on the GTK main thread (Rule #14).

use std::path::Path;
use std::process::Command;

use veyra_filesystem::FilePermissions;

use crate::terminal;

/// Errors from privileged operations.
#[derive(Debug, thiserror::Error)]
pub enum PrivilegedError {
    #[error("pkexec not found on system")]
    PkexecNotFound,

    #[error("Polkit authorization required but agent not found")]
    NoAuthenticationAgent,

    #[error("authorization cancelled by user")]
    Cancelled,

    #[error("failed to launch privileged operation")]
    Failed,

    #[error("no terminal available")]
    NoTerminal,
}

/// Checks whether `pkexec` is available on `$PATH`.
pub(crate) fn is_available() -> bool {
    find_pkexec().is_some()
}

/// Finds `pkexec` on `$PATH`, using the same resolution logic as `terminal.rs`:
/// check if it contains a path separator and verify it as-is, otherwise search
/// every `$PATH` entry.
fn find_pkexec() -> Option<String> {
    let bin = "pkexec";
    let dirs = std::env::var_os("PATH").map(|var| std::env::split_paths(&var).collect::<Vec<_>>());
    find_in_dirs(bin, dirs.unwrap_or_default())
}

fn find_in_dirs(bin: &str, dirs: impl IntoIterator<Item = std::path::PathBuf>) -> Option<String> {
    if bin.contains('/') {
        let candidate = Path::new(bin);
        return is_executable_file(candidate).then_some(bin.to_string());
    }
    dirs.into_iter().find_map(|dir| {
        let candidate = dir.join(bin);
        is_executable_file(&candidate).then_some(candidate.to_string_lossy().to_string())
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

/// Maps a process exit code to a `PrivilegedError`. Exit codes 126 and 127
/// follow pkexec's conventions: 126 = authorization denied, 127 = agent not found.
fn exit_code_to_error(code: Option<i32>) -> PrivilegedError {
    match code {
        Some(126) => PrivilegedError::Cancelled,
        Some(127) => PrivilegedError::NoAuthenticationAgent,
        _ => PrivilegedError::Failed,
    }
}

/// Faz 28: Changes permissions on a single file or directory via pkexec.
/// If `recursive` is `true`, uses `chmod -R`. The octal permissions are
/// validated before spawning pkexec (via `FilePermissions::octal_string()`),
/// so this never constructs a shell string from untrusted input.
///
/// **BLOCKING**: Must only be called from `fs_async::run_blocking`.
pub(crate) fn chmod(
    path: &Path,
    mode: &FilePermissions,
    recursive: bool,
) -> Result<(), PrivilegedError> {
    let pkexec = find_pkexec().ok_or(PrivilegedError::PkexecNotFound)?;
    let octal = mode.octal_string();

    let mut cmd = Command::new(&pkexec);
    cmd.arg("chmod");
    if recursive {
        cmd.arg("-R");
    }
    cmd.arg(octal);
    cmd.arg(path);

    let status = cmd.status().map_err(|_| PrivilegedError::Failed)?;

    if status.success() {
        Ok(())
    } else {
        Err(exit_code_to_error(status.code()))
    }
}

/// Faz 28: Permanently deletes a file or directory via pkexec.
/// If `recursive` is `true`, uses `rm -r` (note: not `rm -rf`, as that silently
/// ignores missing files — here we want explicit error feedback).
/// Trash is not meaningful as root — this backs "Delete Permanently" retried
/// as root, not "Move to Trash as root".
///
/// **BLOCKING**: Must only be called from `fs_async::run_blocking`.
pub(crate) fn remove(path: &Path, recursive: bool) -> Result<(), PrivilegedError> {
    let pkexec = find_pkexec().ok_or(PrivilegedError::PkexecNotFound)?;

    let mut cmd = Command::new(&pkexec);
    cmd.arg("rm");
    cmd.arg("-f");
    if recursive {
        cmd.arg("-r");
    }
    cmd.arg(path);

    let status = cmd.status().map_err(|_| PrivilegedError::Failed)?;

    if status.success() {
        Ok(())
    } else {
        Err(exit_code_to_error(status.code()))
    }
}

/// Faz 28: Moves (renames) a file or directory via pkexec.
/// Both `src` and `dst` are always distinct arguments (Rule #19).
///
/// **BLOCKING**: Must only be called from `fs_async::run_blocking`.
#[allow(dead_code)]
pub(crate) fn r#move(src: &Path, dst: &Path) -> Result<(), PrivilegedError> {
    let pkexec = find_pkexec().ok_or(PrivilegedError::PkexecNotFound)?;

    let status = Command::new(&pkexec)
        .arg("mv")
        .arg(src)
        .arg(dst)
        .status()
        .map_err(|_| PrivilegedError::Failed)?;

    if status.success() {
        Ok(())
    } else {
        Err(exit_code_to_error(status.code()))
    }
}

/// Faz 28: Copies a file or directory via pkexec.
/// If `recursive` is `true`, uses `cp -a` (attribute-preserving, matches
/// user expectations for file-manager copy operations).
///
/// **BLOCKING**: Must only be called from `fs_async::run_blocking`.
#[allow(dead_code)]
pub(crate) fn copy(src: &Path, dst: &Path, recursive: bool) -> Result<(), PrivilegedError> {
    let pkexec = find_pkexec().ok_or(PrivilegedError::PkexecNotFound)?;

    let mut cmd = Command::new(&pkexec);
    cmd.arg("cp");
    if recursive {
        cmd.arg("-a");
    }
    cmd.arg(src);
    cmd.arg(dst);

    let status = cmd.status().map_err(|_| PrivilegedError::Failed)?;

    if status.success() {
        Ok(())
    } else {
        Err(exit_code_to_error(status.code()))
    }
}

/// Faz 28: Opens a terminal in the given directory as root via pkexec.
/// The terminal is resolved first (so it's never a raw user-typed string),
/// then pkexec is used to spawn it with elevated privileges. The user's
/// Polkit authentication agent prompts them for their password.
///
/// **BLOCKING**: Must only be called from `fs_async::run_blocking`.
pub(crate) fn open_terminal_as_root(dir: &Path) -> Result<(), PrivilegedError> {
    let pkexec = find_pkexec().ok_or(PrivilegedError::PkexecNotFound)?;
    let resolved = terminal::resolve_terminal().ok_or(PrivilegedError::NoTerminal)?;

    let mut cmd = Command::new(&pkexec);
    cmd.arg(&resolved.program);
    cmd.args(&resolved.args);
    cmd.current_dir(dir);

    cmd.spawn().map_err(|_| PrivilegedError::Failed)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_126_is_cancelled() {
        assert!(matches!(
            exit_code_to_error(Some(126)),
            PrivilegedError::Cancelled
        ));
    }

    #[test]
    fn exit_code_127_is_no_agent() {
        assert!(matches!(
            exit_code_to_error(Some(127)),
            PrivilegedError::NoAuthenticationAgent
        ));
    }

    #[test]
    fn other_exit_codes_are_failed() {
        assert!(matches!(
            exit_code_to_error(Some(1)),
            PrivilegedError::Failed
        ));
        assert!(matches!(exit_code_to_error(None), PrivilegedError::Failed));
    }
}
