//! Path-traversal defense for archive extraction (Rule #21/#22).
//!
//! Every entry name coming out of an archive is untrusted input. Two
//! independent protections apply to all six formats:
//!
//! 1. [`sanitize_entry_path`] strips any `..`, empty, or `.` component and
//!    drops any Windows-style drive/UNC prefix, so the result is always a
//!    plain relative path with no way to walk above `destination_dir` when
//!    joined onto it (Zip Slip / Path Traversal).
//! 2. `extract.rs` never materializes a symlink/hardlink entry read from an
//!    archive onto disk — every extractor skips them outright. That removes
//!    the entire symlink-attack class (an entry that would otherwise point
//!    outside `destination_dir`, or a TOCTOU swap between two entries) by
//!    construction rather than by validating link targets.

use std::path::PathBuf;

/// Normalizes an archive entry's raw path into a safe, relative path
/// suitable for joining onto the extraction destination. Returns `None`
/// if the entry carries no usable path at all (e.g. `"."`, `""`, or a name
/// made up entirely of `..`/root components) — such entries are skipped by
/// the caller.
///
/// Accepts both `/` and `\` separators (archives created on Windows use the
/// latter). A leading separator or a Windows drive prefix (`C:`) is
/// dropped rather than rejected outright: `"/etc/passwd"` becomes
/// `etc/passwd`, matching what most archive tools do, while `..` segments
/// anywhere in the path are always rejected rather than silently dropped,
/// since collapsing them (e.g. `"a/../../etc"` -> `"etc"`) could still
/// change which real filesystem entry is being referenced.
pub(crate) fn sanitize_entry_path(raw: &str) -> Option<PathBuf> {
    if veyra_core::security::validate_filename(raw).is_err() {
        // Null byte or absurdly long entry name — never a legitimate path,
        // and a null byte can truncate the eventual syscall unpredictably.
        return None;
    }
    let mut out = PathBuf::new();
    for part in raw.split(['/', '\\']) {
        match part {
            "" | "." => continue,
            ".." => return None,
            p if p.ends_with(':') => continue, // Windows drive prefix, e.g. "C:"
            p => out.push(p),
        }
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_through_plain_relative_paths() {
        assert_eq!(
            sanitize_entry_path("dir/file.txt"),
            Some(PathBuf::from("dir/file.txt"))
        );
    }

    #[test]
    fn rejects_parent_traversal_anywhere_in_the_path() {
        assert_eq!(sanitize_entry_path("../evil"), None);
        assert_eq!(sanitize_entry_path("dir/../../evil"), None);
        assert_eq!(sanitize_entry_path("dir/../evil"), None);
    }

    #[test]
    fn strips_leading_absolute_slash_instead_of_escaping() {
        assert_eq!(
            sanitize_entry_path("/etc/passwd"),
            Some(PathBuf::from("etc/passwd"))
        );
    }

    #[test]
    fn strips_windows_drive_and_backslash_separators() {
        assert_eq!(
            sanitize_entry_path("C:\\Windows\\evil.dll"),
            Some(PathBuf::from("Windows/evil.dll"))
        );
    }

    #[test]
    fn rejects_paths_that_normalize_to_nothing() {
        assert_eq!(sanitize_entry_path(""), None);
        assert_eq!(sanitize_entry_path("."), None);
        assert_eq!(sanitize_entry_path("./"), None);
        assert_eq!(sanitize_entry_path("/"), None);
    }

    #[test]
    fn rejects_null_byte_in_entry_name() {
        assert_eq!(sanitize_entry_path("evil\0.txt"), None);
    }

    #[test]
    fn rejects_overlong_entry_name() {
        let long = "a".repeat(veyra_core::security::MAX_PATH_BYTES + 1);
        assert_eq!(sanitize_entry_path(&long), None);
    }
}
