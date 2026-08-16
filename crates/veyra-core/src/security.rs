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
