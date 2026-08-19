//! Faz 42: confirms Faz 20's same-size candidates (`analyzer.rs`) as real
//! duplicates via a three-stage content comparison — cheap first, expensive
//! last, so a huge same-size group that turns out to differ never pays for
//! a full read.
//!
//! - **Stage 1 (size grouping)** is `analyzer::analyze_directory`'s job
//!   already; this module starts from its `SameSizeCandidateGroup` output
//!   (single-file "groups" are never produced there, but this module
//!   ignores them defensively too).
//! - **Stage 2 (4 KB prefix hash)** hashes each candidate's first 4 KB —
//!   files that differ near the start (the overwhelming majority of
//!   same-size non-duplicates: different photos, different archives, ...)
//!   are eliminated without reading the rest.
//! - **Stage 3 (streaming full hash)** streams the complete content of
//!   whatever survived stage 2 through the same hasher in 256 KB chunks
//!   (matching `dev_tools::compute_checksums`'s buffer size), confirming
//!   true byte-for-byte duplicates.
//!
//! Cooperatively cancellable via `OperationControl`, checked between files
//! and between chunks of a single large file (Rule #13) — a cancelled pass
//! returns whatever groups were already confirmed rather than nothing.
//!
//! This module only *finds* duplicates; it never deletes or modifies
//! anything (Rule #38/#39) — that's the caller's job, gated behind explicit
//! user confirmation.

use std::collections::HashMap;
use std::io::Read;

use sha2::{Digest, Sha256};

use crate::analyzer::SameSizeCandidateGroup;
use crate::path::VeyraPath;
use crate::queue::OperationControl;

/// How much of a candidate's content stage 2 hashes before falling back to
/// a full stage-3 pass.
const PREFIX_SIZE: usize = 4096;

/// Read buffer size for the stage-3 full-content hash — matches
/// `dev_tools::compute_checksums` (Rule #33: bounded peak memory regardless
/// of file size).
const FULL_HASH_CHUNK_SIZE: usize = 256 * 1024;

/// A confirmed set of byte-for-byte identical files, ready for the
/// Duplicate Files view to render and the user to select from for cleanup.
#[derive(Debug, Clone, PartialEq)]
pub struct DuplicateGroup {
    /// The full-content SHA-256 digest shared by every file in the group.
    pub hash: String,
    pub size_per_file: u64,
    /// Space reclaimable by keeping exactly one copy: `(files.len() - 1) *
    /// size_per_file`.
    pub wasted_size: u64,
    pub files: Vec<VeyraPath>,
}

/// Runs the stage 2/3 content-hash pipeline over `candidates` (Faz 20's
/// same-size groups), returning only the subsets that turned out to be true
/// duplicates — sorted by `wasted_size` descending, so the most valuable
/// cleanup opportunity is first.
pub fn find_duplicates(
    candidates: &[SameSizeCandidateGroup],
    control: &OperationControl,
) -> Vec<DuplicateGroup> {
    let mut groups = Vec::new();

    'candidates: for candidate in candidates {
        if candidate.paths.len() < 2 {
            continue;
        }

        // Stage 2: cheap 4 KB prefix hash, run on every same-size file.
        let mut by_prefix: HashMap<String, Vec<VeyraPath>> = HashMap::new();
        for path in &candidate.paths {
            if control.is_cancelled() {
                break 'candidates;
            }
            if let Some(prefix_hash) = hash_prefix(path) {
                by_prefix.entry(prefix_hash).or_default().push(path.clone());
            }
        }

        // Stage 3: full streaming hash, only for files whose prefix matched
        // at least one other file's.
        for prefix_matched in by_prefix.into_values() {
            if prefix_matched.len() < 2 {
                continue;
            }

            let mut by_full: HashMap<String, Vec<VeyraPath>> = HashMap::new();
            for path in &prefix_matched {
                if control.is_cancelled() {
                    break 'candidates;
                }
                if let Some(full_hash) = hash_full(path, control) {
                    by_full.entry(full_hash).or_default().push(path.clone());
                }
            }

            for (hash, files) in by_full {
                if files.len() < 2 {
                    continue;
                }
                let wasted_size = candidate.size_bytes.saturating_mul(files.len() as u64 - 1);
                groups.push(DuplicateGroup {
                    hash,
                    size_per_file: candidate.size_bytes,
                    wasted_size,
                    files,
                });
            }
        }
    }

    groups.sort_by_key(|g| std::cmp::Reverse(g.wasted_size));
    groups
}

/// Hashes the first `PREFIX_SIZE` bytes of `path`'s content. Returns `None`
/// for a non-local path (no local bytes to stream without first downloading
/// it, matching `checksum_dialog`'s precedent) or on any I/O error — a file
/// that vanished or became unreadable mid-scan is simply excluded rather
/// than aborting the whole pass (Rule #16/#18).
fn hash_prefix(path: &VeyraPath) -> Option<String> {
    let local = path.as_local_path()?;
    let mut file = std::fs::File::open(local).ok()?;
    let mut buf = vec![0u8; PREFIX_SIZE];
    let mut total_read = 0;
    while total_read < buf.len() {
        let read = file.read(&mut buf[total_read..]).ok()?;
        if read == 0 {
            break;
        }
        total_read += read;
    }
    let mut hasher = Sha256::new();
    hasher.update(&buf[..total_read]);
    Some(format!("{:x}", hasher.finalize()))
}

/// Streams `path`'s full content through SHA-256 in `FULL_HASH_CHUNK_SIZE`
/// chunks, checking `control` between reads so cancelling mid-hash of a
/// huge file stops promptly instead of running to completion in the
/// background (Rule #13). Returns `None` on the same conditions as
/// `hash_prefix`, plus cancellation.
fn hash_full(path: &VeyraPath, control: &OperationControl) -> Option<String> {
    let local = path.as_local_path()?;
    let mut file = std::fs::File::open(local).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; FULL_HASH_CHUNK_SIZE];
    loop {
        if control.is_cancelled() {
            return None;
        }
        let read = file.read(&mut buf).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn candidate(size_bytes: u64, paths: Vec<VeyraPath>) -> SameSizeCandidateGroup {
        SameSizeCandidateGroup { size_bytes, paths }
    }

    fn local(p: impl Into<std::path::PathBuf>) -> VeyraPath {
        VeyraPath::from_local(p.into())
    }

    #[test]
    fn identical_files_form_one_confirmed_group() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = vec![7u8; 10_000];
        fs::write(tmp.path().join("a.bin"), &payload).unwrap();
        fs::write(tmp.path().join("b.bin"), &payload).unwrap();

        let candidates = vec![candidate(
            10_000,
            vec![
                local(tmp.path().join("a.bin")),
                local(tmp.path().join("b.bin")),
            ],
        )];
        let control = OperationControl::new();
        let groups = find_duplicates(&candidates, &control);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].size_per_file, 10_000);
        assert_eq!(groups[0].wasted_size, 10_000);
        assert_eq!(groups[0].files.len(), 2);
    }

    #[test]
    fn same_size_but_different_prefix_is_not_a_duplicate() {
        let tmp = tempfile::tempdir().unwrap();
        let mut a = vec![1u8; 10_000];
        let b = vec![2u8; 10_000];
        a[0] = 0; // differs from b immediately, well within the 4 KB prefix
        fs::write(tmp.path().join("a.bin"), &a).unwrap();
        fs::write(tmp.path().join("b.bin"), &b).unwrap();

        let candidates = vec![candidate(
            10_000,
            vec![
                local(tmp.path().join("a.bin")),
                local(tmp.path().join("b.bin")),
            ],
        )];
        let control = OperationControl::new();
        let groups = find_duplicates(&candidates, &control);

        assert!(groups.is_empty());
    }

    #[test]
    fn same_prefix_but_different_tail_is_not_a_duplicate() {
        let tmp = tempfile::tempdir().unwrap();
        let mut a = vec![9u8; 10_000];
        let mut b = vec![9u8; 10_000];
        a[9_999] = 1;
        b[9_999] = 2;
        fs::write(tmp.path().join("a.bin"), &a).unwrap();
        fs::write(tmp.path().join("b.bin"), &b).unwrap();

        let candidates = vec![candidate(
            10_000,
            vec![
                local(tmp.path().join("a.bin")),
                local(tmp.path().join("b.bin")),
            ],
        )];
        let control = OperationControl::new();
        let groups = find_duplicates(&candidates, &control);

        assert!(groups.is_empty());
    }

    #[test]
    fn three_identical_plus_one_different_splits_into_correct_groups() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = vec![5u8; 5_000];
        let other = vec![6u8; 5_000];
        fs::write(tmp.path().join("a.bin"), &payload).unwrap();
        fs::write(tmp.path().join("b.bin"), &payload).unwrap();
        fs::write(tmp.path().join("c.bin"), &payload).unwrap();
        fs::write(tmp.path().join("d.bin"), &other).unwrap();

        let candidates = vec![candidate(
            5_000,
            vec![
                local(tmp.path().join("a.bin")),
                local(tmp.path().join("b.bin")),
                local(tmp.path().join("c.bin")),
                local(tmp.path().join("d.bin")),
            ],
        )];
        let control = OperationControl::new();
        let groups = find_duplicates(&candidates, &control);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].files.len(), 3);
        assert_eq!(groups[0].wasted_size, 10_000);
    }

    #[test]
    fn wasted_size_accounts_for_every_extra_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = vec![3u8; 2_048];
        for name in ["a.bin", "b.bin", "c.bin", "d.bin"] {
            fs::write(tmp.path().join(name), &payload).unwrap();
        }

        let candidates = vec![candidate(
            2_048,
            vec![
                local(tmp.path().join("a.bin")),
                local(tmp.path().join("b.bin")),
                local(tmp.path().join("c.bin")),
                local(tmp.path().join("d.bin")),
            ],
        )];
        let control = OperationControl::new();
        let groups = find_duplicates(&candidates, &control);

        assert_eq!(groups.len(), 1);
        // 4 files sharing content -> 3 wasted copies.
        assert_eq!(groups[0].wasted_size, 2_048 * 3);
    }

    #[test]
    fn single_file_candidate_group_is_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a.bin"), vec![1u8; 100]).unwrap();

        let candidates = vec![candidate(100, vec![local(tmp.path().join("a.bin"))])];
        let control = OperationControl::new();
        let groups = find_duplicates(&candidates, &control);

        assert!(groups.is_empty());
    }

    #[test]
    fn missing_file_is_skipped_not_fatal() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = vec![4u8; 1_000];
        fs::write(tmp.path().join("a.bin"), &payload).unwrap();
        // "b.bin" is never created — simulates a file removed mid-scan.

        let candidates = vec![candidate(
            1_000,
            vec![
                local(tmp.path().join("a.bin")),
                local(tmp.path().join("b.bin")),
            ],
        )];
        let control = OperationControl::new();
        let groups = find_duplicates(&candidates, &control);

        assert!(groups.is_empty());
    }

    #[test]
    fn cancelled_before_start_returns_no_groups() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = vec![8u8; 1_000];
        fs::write(tmp.path().join("a.bin"), &payload).unwrap();
        fs::write(tmp.path().join("b.bin"), &payload).unwrap();

        let candidates = vec![candidate(
            1_000,
            vec![
                local(tmp.path().join("a.bin")),
                local(tmp.path().join("b.bin")),
            ],
        )];
        let control = OperationControl::new();
        control.cancel();
        let groups = find_duplicates(&candidates, &control);

        assert!(groups.is_empty());
    }
}
