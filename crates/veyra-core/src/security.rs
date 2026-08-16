//! Security-hardening primitives shared across Veyra crates (Rules #19-24,
//! Faz 29). Two independent concerns live here:
//!
//! 1. [`validate_filename`] / [`has_bidi_override`] — validating untrusted
//!    filenames (archive entries, rename/paste input) against null-byte
//!    injection, overlong paths, and Unicode bidi-override spoofing.
//! 2. [`write_atomic_private`] — writing config/state files (bookmarks,
//!    shortcuts, history) so the temporary file is never briefly readable
//!    by other local users and never left behind on failure (Security
//!    Model 3.2).

use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Maximum path length Veyra accepts, matching Linux's `PATH_MAX` (4096
/// bytes, including the terminating null the kernel adds). Longer paths
/// are rejected before any syscall touches them.
pub const MAX_PATH_BYTES: usize = 4096;

/// Reasons a candidate filename fails validation.
#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone, Copy)]
pub enum FilenameError {
    #[error("filename contains a null byte")]
    NullByte,
    #[error("path exceeds {MAX_PATH_BYTES} bytes")]
    TooLong,
}

/// Validates a filename or path component supplied by untrusted input (an
/// archive entry, a rename dialog, a clipboard paste): rejects embedded
/// null bytes, which truncate C-string syscalls unpredictably and can
/// smuggle a shorter "real" path past a check that only looked at the
/// visible suffix, and paths exceeding [`MAX_PATH_BYTES`]. This never
/// panics on malformed input, including empty strings.
pub fn validate_filename(name: &str) -> Result<(), FilenameError> {
    if name.bytes().any(|b| b == 0) {
        return Err(FilenameError::NullByte);
    }
    if name.len() > MAX_PATH_BYTES {
        return Err(FilenameError::TooLong);
    }
    Ok(())
}

/// Returns `true` if `name` contains a Unicode bidirectional
/// override/embedding character, which can be used to spoof a file's
/// apparent extension — e.g. `U+202E` (RIGHT-TO-LEFT OVERRIDE) reverses
/// the rendering of everything after it, so `"exe.txt\u{202E}cod"` is
/// really named with a `.doc` look-alike suffix while the bytes on disk
/// end in something else entirely. This is advisory rather than a hard
/// rejection in [`validate_filename`], since legitimate RTL-script
/// filenames use nearby, non-override codepoints — callers should surface
/// a UI warning (e.g. show the raw codepoints) rather than block the name.
pub fn has_bidi_override(name: &str) -> bool {
    name.chars()
        .any(|c| matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'))
}

/// Writes `contents` to `tmp_path`, restricts it to owner-only `0600`
/// permissions, then renames it onto `final_path`. If any step fails,
/// `tmp_path` is removed rather than left behind as readable debris.
///
/// Callers choose `tmp_path` themselves (typically `final_path` with a
/// `.tmp` suffix) so this doesn't dictate a naming convention.
pub fn write_atomic_private(tmp_path: &Path, final_path: &Path, contents: &[u8]) -> io::Result<()> {
    let result = write_atomic_private_inner(tmp_path, final_path, contents);
    if result.is_err() {
        let _ = std::fs::remove_file(tmp_path);
    }
    result
}

fn write_atomic_private_inner(
    tmp_path: &Path,
    final_path: &Path,
    contents: &[u8],
) -> io::Result<()> {
    std::fs::write(tmp_path, contents)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(tmp_path, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(tmp_path, final_path)
}

/// Process-wide flag backing the Preferences dialog's "Sanitize file paths
/// in logs" toggle (Faz 34). A plain `AtomicBool` rather than a
/// `Rc<RefCell<VeyraSettings>>` field because some of the log call sites
/// this gates run on background worker threads (e.g. the thumbnail decode
/// pool), which can't hold a `Rc`. Defaults to `false` (full paths logged,
/// today's behavior) until `set_sanitize_log_paths` is called with the
/// loaded setting at startup.
static SANITIZE_LOG_PATHS: AtomicBool = AtomicBool::new(false);

/// Whether path-sanitizing is currently enabled for logging.
pub fn sanitize_log_paths_enabled() -> bool {
    SANITIZE_LOG_PATHS.load(Ordering::Relaxed)
}

/// Enables or disables path-sanitizing for logging, effective immediately
/// for every subsequent [`log_path`] call on any thread.
pub fn set_sanitize_log_paths(enabled: bool) {
    SANITIZE_LOG_PATHS.store(enabled, Ordering::Relaxed);
}

/// Renders `path` for a `tracing` log line: the full path when sanitizing
/// is disabled (today's behavior), or just the final path component when
/// enabled, so a shared log file doesn't reveal a user's directory
/// structure or other filenames (Kural #23) while still naming which file
/// an error was about. Takes anything `Display`-able (a `std::path::Path`'s
/// `.display()`, a `VeyraPath`, a plain string) rather than `&Path`
/// specifically, so call sites logging a `veyra_filesystem::VeyraPath`
/// (which can be a remote URI, not a local filesystem path) don't need an
/// extra conversion.
pub fn log_path(path: impl std::fmt::Display) -> String {
    let rendered = path.to_string();
    if sanitize_log_paths_enabled() {
        rendered
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("<redacted>")
            .to_string()
    } else {
        rendered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_null_byte() {
        assert_eq!(
            validate_filename("evil\0name"),
            Err(FilenameError::NullByte)
        );
    }

    #[test]
    fn rejects_overlong_path() {
        let long = "a".repeat(MAX_PATH_BYTES + 1);
        assert_eq!(validate_filename(&long), Err(FilenameError::TooLong));
    }

    #[test]
    fn accepts_normal_names() {
        assert_eq!(validate_filename("report.pdf"), Ok(()));
        assert_eq!(validate_filename(""), Ok(()));
        assert_eq!(validate_filename(&"a".repeat(MAX_PATH_BYTES)), Ok(()));
    }

    #[test]
    fn detects_rtl_override() {
        assert!(has_bidi_override("invoice\u{202E}gpj.exe"));
        assert!(!has_bidi_override("normal-file.txt"));
    }

    #[test]
    fn write_atomic_private_sets_owner_only_permissions() {
        let dir = std::env::temp_dir().join(format!("veyra-security-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let final_path = dir.join("out.txt");
        let tmp_path = dir.join("out.txt.tmp");

        write_atomic_private(&tmp_path, &final_path, b"hello").unwrap();

        assert!(!tmp_path.exists());
        assert_eq!(std::fs::read(&final_path).unwrap(), b"hello");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&final_path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn log_path_toggles_between_full_and_basename_only() {
        // Single test owns every read/write of the shared static so it
        // can't interleave with another test's toggle.
        let path = Path::new("/home/alice/Documents/secret-plan.txt");

        set_sanitize_log_paths(false);
        assert!(!sanitize_log_paths_enabled());
        assert_eq!(
            log_path(path.display()),
            "/home/alice/Documents/secret-plan.txt"
        );

        set_sanitize_log_paths(true);
        assert!(sanitize_log_paths_enabled());
        assert_eq!(log_path(path.display()), "secret-plan.txt");

        set_sanitize_log_paths(false);
    }

    #[test]
    fn write_atomic_private_cleans_up_tmp_on_failure() {
        let dir =
            std::env::temp_dir().join(format!("veyra-security-test-fail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let tmp_path = dir.join("out.txt.tmp");
        // Final path's parent doesn't exist, so the rename fails.
        let final_path = dir.join("missing-dir").join("out.txt");

        let result = write_atomic_private(&tmp_path, &final_path, b"hello");

        assert!(result.is_err());
        assert!(!tmp_path.exists());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
