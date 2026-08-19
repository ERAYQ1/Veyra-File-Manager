//! Faz 39: Developer Mode — a `win.toggle-developer-mode` (`Ctrl+Shift+D`)
//! switch that reveals a "Developer" context-menu submenu for advanced
//! users: absolute/URI/relative path copying, launching an external editor,
//! and MD5/SHA-256 checksums (`dialogs::checksum_dialog`).
//!
//! Kept in `veyra-filesystem`-free territory except for `VeyraPath`/
//! `OperationControl` themselves — every function here is plain, GTK-free
//! Rust so it stays unit-testable without a display connection, matching
//! `terminal.rs`'s own split between "resolve a launchable candidate" (pure)
//! and "spawn it" (a thin `Command::spawn` wrapper).

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

use gio::prelude::FileExt;
use md5::{Digest, Md5};
use sha2::{Sha256, Sha512};

use veyra_filesystem::{OperationControl, VeyraPath};

/// Read buffer size for `compute_checksums` — large enough to amortize
/// syscall overhead on big files, small enough to keep peak memory bounded
/// regardless of file size (Rule #33).
const CHECKSUM_CHUNK_SIZE: usize = 256 * 1024;

/// The `file://`-style URI GIO resolves `path` to, for the "Copy URI"
/// context-menu entry.
pub(crate) fn copy_uri(path: &VeyraPath) -> String {
    path.to_gio_file().uri().to_string()
}

/// `path` rendered relative to `base` (the Git root when `path` is inside
/// one, otherwise the currently open directory) — e.g. `src/main.rs`, or
/// `../docs/readme.md` when `path` is a sibling of `base` rather than a
/// descendant. Falls back to the absolute path only when the two share no
/// common ancestor at all (Rule #17, never guess a nonsensical relative
/// path) — on a single Unix filesystem tree that's effectively never, since
/// `/` is always a shared ancestor.
pub(crate) fn relative_path(path: &Path, base: &Path) -> String {
    if let Ok(stripped) = path.strip_prefix(base) {
        if stripped.as_os_str().is_empty() {
            return ".".to_string();
        }
        return stripped.display().to_string();
    }

    // No direct prefix relationship — walk up from `base` to the shared
    // ancestor, prefixing one `..` per level, then descend into `path`'s
    // remaining components.
    let mut base_ancestors: Vec<&Path> = base.ancestors().collect();
    base_ancestors.reverse();
    let mut path_ancestors: Vec<&Path> = path.ancestors().collect();
    path_ancestors.reverse();

    let common_len = base_ancestors
        .iter()
        .zip(path_ancestors.iter())
        .take_while(|(a, b)| a == b)
        .count();

    if common_len == 0 {
        return path.display().to_string();
    }

    let up_count = base_ancestors.len() - common_len;
    let mut result = PathBuf::new();
    for _ in 0..up_count {
        result.push("..");
    }
    if let Ok(remainder) = path.strip_prefix(base_ancestors[common_len - 1]) {
        result.push(remainder);
    }
    result.display().to_string()
}

/// MD5, SHA-256, and SHA-512 hex digests of a file's contents, computed in
/// one streaming read pass (Rule #33 — no reason to read a large file three
/// times for three independent hashers).
pub(crate) struct ChecksumResult {
    pub md5: String,
    pub sha256: String,
    pub sha512: String,
}

/// The algorithm a pasted checksum turned out to match, from
/// `matching_algorithm` — its `label()` is the exact text every checksum
/// row's title already renders (`"MD5"`/`"SHA-256"`/`"SHA-512"`), so a
/// match message can quote it back without a separate name table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChecksumAlgorithm {
    Md5,
    Sha256,
    Sha512,
}

impl ChecksumAlgorithm {
    pub(crate) fn label(self) -> &'static str {
        match self {
            ChecksumAlgorithm::Md5 => "MD5",
            ChecksumAlgorithm::Sha256 => "SHA-256",
            ChecksumAlgorithm::Sha512 => "SHA-512",
        }
    }
}

/// Compares `expected` (arbitrary pasted user text — leading/trailing
/// whitespace and letter case both ignored, since that's exactly the kind
/// of copy-paste noise a checksum pasted from a download page's README or
/// terminal output carries) against `result`'s three digests, returning
/// whichever algorithm matched. An empty (after trimming) `expected` never
/// matches anything — that's "nothing pasted yet", not a mismatch.
pub(crate) fn matching_algorithm(
    result: &ChecksumResult,
    expected: &str,
) -> Option<ChecksumAlgorithm> {
    let expected = expected.trim();
    if expected.is_empty() {
        return None;
    }
    if result.md5.eq_ignore_ascii_case(expected) {
        Some(ChecksumAlgorithm::Md5)
    } else if result.sha256.eq_ignore_ascii_case(expected) {
        Some(ChecksumAlgorithm::Sha256)
    } else if result.sha512.eq_ignore_ascii_case(expected) {
        Some(ChecksumAlgorithm::Sha512)
    } else {
        None
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ChecksumError {
    #[error("checksum computation was cancelled")]
    Cancelled,
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Streams `path` through MD5, SHA-256, and SHA-512 simultaneously,
/// checking `control` between chunks so a dialog closed mid-hash of a huge
/// file stops promptly instead of running to completion in the background
/// (Rule #13).
pub(crate) fn compute_checksums(
    path: &Path,
    control: &OperationControl,
) -> Result<ChecksumResult, ChecksumError> {
    let mut file = std::fs::File::open(path)?;
    let mut md5 = Md5::new();
    let mut sha256 = Sha256::new();
    let mut sha512 = Sha512::new();
    let mut buf = vec![0u8; CHECKSUM_CHUNK_SIZE];

    loop {
        if control.is_cancelled() {
            return Err(ChecksumError::Cancelled);
        }
        let read = file.read(&mut buf)?;
        if read == 0 {
            break;
        }
        md5.update(&buf[..read]);
        sha256.update(&buf[..read]);
        sha512.update(&buf[..read]);
    }

    Ok(ChecksumResult {
        md5: hex_lower(&md5.finalize()),
        sha256: hex_lower(&sha256.finalize()),
        sha512: hex_lower(&sha512.finalize()),
    })
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A resolved, launchable editor candidate — mirrors `terminal::Candidate`.
struct Candidate {
    program: PathBuf,
    args: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum EditorError {
    #[error("no code editor was found on this system")]
    NotFound,
    #[error("failed to launch editor: {0}")]
    Spawn(#[source] io::Error),
}

/// GUI/terminal code editors checked on `$PATH` as the last-resort fallback
/// tier, in priority order, once `$VISUAL`/`$EDITOR` have both been tried.
const KNOWN_EDITORS: &[&str] = &["code", "zed", "subl", "nvim", "gedit"];

/// Opens `path` (a file or directory) in the user's preferred editor:
/// `$VISUAL`, then `$EDITOR`, then the first of [`KNOWN_EDITORS`] found on
/// `$PATH`. Every candidate is a binary resolved to actually exist on disk
/// first, then launched via `Command::new` with `path` as a plain argument
/// — never a shell string (Rule #19).
pub(crate) fn open_in_editor(path: &VeyraPath) -> Result<(), EditorError> {
    let local = path.as_local_path().ok_or(EditorError::NotFound)?;
    let candidate = resolve_editor().ok_or(EditorError::NotFound)?;
    Command::new(&candidate.program)
        .args(&candidate.args)
        .arg(local)
        .spawn()
        .map(|_| ())
        .map_err(EditorError::Spawn)
}

fn resolve_editor() -> Option<Candidate> {
    resolve_env_editor("VISUAL")
        .or_else(|| resolve_env_editor("EDITOR"))
        .or_else(resolve_known_editor)
}

fn resolve_env_editor(var: &str) -> Option<Candidate> {
    let value = std::env::var(var).ok()?;
    let mut tokens = value.split_whitespace();
    let bin = tokens.next()?;
    let program = crate::terminal::find_in_path(bin)?;
    let args = tokens.map(str::to_string).collect();
    Some(Candidate { program, args })
}

fn resolve_known_editor() -> Option<Candidate> {
    KNOWN_EDITORS.iter().find_map(|name| {
        crate::terminal::find_in_path(name).map(|program| Candidate {
            program,
            args: Vec::new(),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_path_of_a_direct_descendant() {
        let base = Path::new("/home/user/project");
        let path = Path::new("/home/user/project/src/main.rs");
        assert_eq!(relative_path(path, base), "src/main.rs");
    }

    #[test]
    fn relative_path_of_the_base_itself_is_dot() {
        let base = Path::new("/home/user/project");
        assert_eq!(relative_path(base, base), ".");
    }

    #[test]
    fn relative_path_walks_up_and_back_down() {
        let base = Path::new("/home/user/project/src");
        let path = Path::new("/home/user/project/docs/readme.md");
        assert_eq!(relative_path(path, base), "../docs/readme.md");
    }

    #[test]
    fn relative_path_walks_up_multiple_levels_to_a_shared_root() {
        let base = Path::new("/home/user/project");
        let path = Path::new("/mnt/other/file.txt");
        assert_eq!(relative_path(path, base), "../../../mnt/other/file.txt");
    }

    #[test]
    fn checksums_of_known_content_match_expected_digests() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("hello.txt");
        std::fs::write(&file, b"hello world").unwrap();

        let control = OperationControl::new();
        let result = compute_checksums(&file, &control).unwrap();

        assert_eq!(result.md5, "5eb63bbbe01eeed093cb22bb8f5acdc3");
        assert_eq!(
            result.sha256,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
        assert_eq!(
            result.sha512,
            "309ecc489c12d6eb4cc40f50c902f2b4d0ed77ee511a7c7a9bcd3ca86d4cd86f989dd35bc5ff499670da34255b45b0cfd830e81f605dcf7dc5542e93ae9cd76f"
        );
    }

    #[test]
    fn checksums_of_an_empty_file_match_the_well_known_empty_digests() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("empty.txt");
        std::fs::write(&file, b"").unwrap();

        let control = OperationControl::new();
        let result = compute_checksums(&file, &control).unwrap();

        assert_eq!(result.md5, "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(
            result.sha256,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            result.sha512,
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
        );
    }

    #[test]
    fn matching_algorithm_finds_each_algorithm_case_and_whitespace_insensitively() {
        let result = ChecksumResult {
            md5: "5eb63bbbe01eeed093cb22bb8f5acdc3".to_string(),
            sha256: "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
                .to_string(),
            sha512: "309ecc489c12d6eb4cc40f50c902f2b4d0ed77ee511a7c7a9bcd3ca86d4cd86f989dd35bc5ff499670da34255b45b0cfd830e81f605dcf7dc5542e93ae9cd76f".to_string(),
        };

        assert_eq!(
            matching_algorithm(&result, "  5EB63BBBE01EEED093CB22BB8F5ACDC3  "),
            Some(ChecksumAlgorithm::Md5)
        );
        assert_eq!(
            matching_algorithm(
                &result,
                "B94D27B9934D3E08A52E52D7DA7DABFAC484EFE37A5380EE9088F7ACE2EFCDE9"
            ),
            Some(ChecksumAlgorithm::Sha256)
        );
        assert_eq!(
            matching_algorithm(&result, &result.sha512),
            Some(ChecksumAlgorithm::Sha512)
        );
    }

    #[test]
    fn matching_algorithm_returns_none_for_an_unrelated_or_empty_string() {
        let result = ChecksumResult {
            md5: "5eb63bbbe01eeed093cb22bb8f5acdc3".to_string(),
            sha256: "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9".to_string(),
            sha512: "x".repeat(128),
        };

        assert_eq!(matching_algorithm(&result, "not a checksum"), None);
        assert_eq!(matching_algorithm(&result, ""), None);
        assert_eq!(matching_algorithm(&result, "   "), None);
    }

    #[test]
    fn checksums_are_cancelled_when_control_is_already_cancelled() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("hello.txt");
        std::fs::write(&file, b"hello world").unwrap();

        let control = OperationControl::new();
        control.cancel();
        assert!(matches!(
            compute_checksums(&file, &control),
            Err(ChecksumError::Cancelled)
        ));
    }

    #[test]
    fn open_in_editor_rejects_remote_locations() {
        let path = VeyraPath::from_uri("sftp://example.com/home/user/file.txt");
        assert!(matches!(open_in_editor(&path), Err(EditorError::NotFound)));
    }

    #[test]
    fn known_editors_list_has_no_duplicates() {
        let mut sorted = KNOWN_EDITORS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), KNOWN_EDITORS.len());
    }
}
