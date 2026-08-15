//! Faz 20: the Disk Analyzer's recursive scan engine. Builds a `UsageNode`
//! tree (sizes bottom-up, children sorted largest-first for the UI's
//! proportional breakdown bar) plus three flat derived views computed in the
//! same walk: the largest files, the largest directories, and groups of
//! files sharing an identical size (duplicate candidates, ahead of a real
//! content-hash comparison in a later phase).
//!
//! Cooperatively cancellable via the same `OperationControl` `dircount.rs`
//! uses (Rule #13), and shares its symlink policy: `NOFOLLOW_SYMLINKS`
//! means a symlinked directory is reported as `FileType::SymbolicLink`, not
//! `Directory`, so it's counted as a leaf entry and never traversed into —
//! no symlink-cycle risk (Rule #22). A subdirectory that fails to enumerate
//! mid-walk (permission denied, removed concurrently) is skipped rather than
//! aborting the whole scan (Rule #18); only a failure resolving the root
//! itself is a hard error.

use std::collections::HashMap;

use gio::prelude::*;

use crate::error::{map_gio_error, FsError};
use crate::path::VeyraPath;
use crate::queue::OperationControl;

/// How many entries `analyze_directory` keeps for the largest-files and
/// largest-directories lists.
const TOP_N: usize = 200;

/// Minimum file size considered for duplicate-size grouping — matches the
/// Faz 20 spec's "aynı boyutu paylaşan dosyalar (örn. size > 1 MB)" so the
/// index doesn't fill up with every empty or near-empty file on a large
/// tree.
const DUPLICATE_MIN_SIZE: u64 = 1_048_576;

/// One node in the scanned tree: a file or directory under the analyzed
/// root. `size_bytes` is the cumulative size of the subtree for directories
/// (recursive), or the entry's own size for files. `children` is sorted
/// largest-`size_bytes`-first, ready for the breakdown bar/list to render
/// directly.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageNode {
    pub name: String,
    pub path: VeyraPath,
    pub is_dir: bool,
    pub size_bytes: u64,
    pub direct_file_count: u64,
    pub direct_dir_count: u64,
    pub children: Vec<UsageNode>,
}

/// A single flat entry in the largest-files/largest-directories lists.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageEntry {
    pub name: String,
    pub path: VeyraPath,
    pub size_bytes: u64,
}

/// A group of files under the scanned root that share the same byte size —
/// a cheap, hash-free heuristic for "possibly duplicated", surfaced as
/// candidates for the user (or a future content-hash pass) to confirm.
#[derive(Debug, Clone, PartialEq)]
pub struct DuplicateGroup {
    pub size_bytes: u64,
    pub paths: Vec<VeyraPath>,
}

/// The full result of scanning a directory: the tree itself plus the three
/// derived views the Disk Analyzer UI's tabs render.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisResult {
    pub tree: UsageNode,
    pub largest_files: Vec<UsageEntry>,
    pub largest_dirs: Vec<UsageEntry>,
    pub duplicate_candidates: Vec<DuplicateGroup>,
}

/// Recursively scans `dir`, building its `UsageNode` tree and the derived
/// largest-files/largest-dirs/duplicate-candidate views in one walk.
/// Cancelling `control` mid-walk stops early and returns whatever was
/// accumulated so far as `Ok`, matching `count_dir_recursive`'s contract.
pub fn analyze_directory(
    dir: &VeyraPath,
    control: &OperationControl,
) -> Result<AnalysisResult, FsError> {
    let root_file = dir.to_gio_file();
    // Only the root itself is a hard error (nonexistent/inaccessible target
    // directory); every subdirectory failure below is skipped instead.
    root_file
        .query_info(
            "standard::type",
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            gio::Cancellable::NONE,
        )
        .map_err(|e| map_gio_error(dir, e))?;

    let name = dir.file_name().unwrap_or_else(|| dir.to_string());
    let mut largest_files = Vec::new();
    let mut largest_dirs = Vec::new();
    let mut duplicate_index: HashMap<u64, Vec<VeyraPath>> = HashMap::new();

    let tree = walk(
        &root_file,
        dir,
        name,
        control,
        &mut largest_files,
        &mut largest_dirs,
        &mut duplicate_index,
    );

    largest_files.sort_by_key(|e| std::cmp::Reverse(e.size_bytes));
    largest_files.truncate(TOP_N);

    largest_dirs.sort_by_key(|e| std::cmp::Reverse(e.size_bytes));
    largest_dirs.truncate(TOP_N);

    let mut duplicate_candidates: Vec<DuplicateGroup> = duplicate_index
        .into_iter()
        .filter(|(_, paths)| paths.len() >= 2)
        .map(|(size_bytes, paths)| DuplicateGroup { size_bytes, paths })
        .collect();
    duplicate_candidates.sort_by_key(|g| std::cmp::Reverse(g.size_bytes));

    Ok(AnalysisResult {
        tree,
        largest_files,
        largest_dirs,
        duplicate_candidates,
    })
}

#[allow(clippy::too_many_arguments)]
fn walk(
    file: &gio::File,
    path: &VeyraPath,
    name: String,
    control: &OperationControl,
    largest_files: &mut Vec<UsageEntry>,
    largest_dirs: &mut Vec<UsageEntry>,
    duplicate_index: &mut HashMap<u64, Vec<VeyraPath>>,
) -> UsageNode {
    let mut node = UsageNode {
        name,
        path: path.clone(),
        is_dir: true,
        size_bytes: 0,
        direct_file_count: 0,
        direct_dir_count: 0,
        children: Vec::new(),
    };

    let Ok(enumerator) = file.enumerate_children(
        "standard::name,standard::display-name,standard::type,standard::size",
        gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
        gio::Cancellable::NONE,
    ) else {
        return node;
    };

    loop {
        if control.is_cancelled() {
            break;
        }
        match enumerator.next_file(gio::Cancellable::NONE) {
            Ok(Some(info)) => {
                let child_file = enumerator.child(&info);
                let child_path = VeyraPath::from_gio_file(&child_file);
                let child_name = info.display_name().to_string();

                if info.file_type() == gio::FileType::Directory {
                    node.direct_dir_count += 1;
                    let child_node = walk(
                        &child_file,
                        &child_path,
                        child_name,
                        control,
                        largest_files,
                        largest_dirs,
                        duplicate_index,
                    );
                    node.size_bytes += child_node.size_bytes;
                    largest_dirs.push(UsageEntry {
                        name: child_node.name.clone(),
                        path: child_node.path.clone(),
                        size_bytes: child_node.size_bytes,
                    });
                    node.children.push(child_node);
                } else {
                    node.direct_file_count += 1;
                    let size = info.size().max(0) as u64;
                    node.size_bytes += size;
                    largest_files.push(UsageEntry {
                        name: child_name.clone(),
                        path: child_path.clone(),
                        size_bytes: size,
                    });
                    if size >= DUPLICATE_MIN_SIZE {
                        duplicate_index
                            .entry(size)
                            .or_default()
                            .push(child_path.clone());
                    }
                    node.children.push(UsageNode {
                        name: child_name,
                        path: child_path,
                        is_dir: false,
                        size_bytes: size,
                        direct_file_count: 0,
                        direct_dir_count: 0,
                        children: Vec::new(),
                    });
                }
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }

    node.children
        .sort_by_key(|n| std::cmp::Reverse(n.size_bytes));
    node
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn dir(p: impl Into<std::path::PathBuf>) -> VeyraPath {
        VeyraPath::from_local(p.into())
    }

    #[test]
    fn builds_tree_with_sizes_and_sorted_children() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("big")).unwrap();
        fs::write(tmp.path().join("big/a.bin"), vec![0u8; 300]).unwrap();
        fs::create_dir(tmp.path().join("small")).unwrap();
        fs::write(tmp.path().join("small/b.bin"), vec![0u8; 50]).unwrap();
        fs::write(tmp.path().join("top.txt"), b"hi").unwrap();

        let control = OperationControl::new();
        let result = analyze_directory(&dir(tmp.path()), &control).unwrap();

        assert_eq!(result.tree.size_bytes, 352);
        assert_eq!(result.tree.direct_file_count, 1);
        assert_eq!(result.tree.direct_dir_count, 2);
        assert_eq!(result.tree.children.len(), 3);
        // Largest child first: "big" (300) > "small" (50) > "top.txt" (2).
        assert_eq!(result.tree.children[0].name, "big");
        assert_eq!(result.tree.children[0].size_bytes, 300);
        assert_eq!(result.tree.children[2].name, "top.txt");
    }

    #[test]
    fn largest_files_sorted_descending_across_whole_tree() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("small.bin"), vec![0u8; 10]).unwrap();
        fs::write(tmp.path().join("sub/huge.bin"), vec![0u8; 1000]).unwrap();
        fs::write(tmp.path().join("sub/medium.bin"), vec![0u8; 100]).unwrap();

        let control = OperationControl::new();
        let result = analyze_directory(&dir(tmp.path()), &control).unwrap();

        let sizes: Vec<u64> = result.largest_files.iter().map(|f| f.size_bytes).collect();
        assert_eq!(sizes, vec![1000, 100, 10]);
        assert_eq!(result.largest_files[0].name, "huge.bin");
    }

    #[test]
    fn largest_dirs_reports_recursive_subtree_size() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("parent/child")).unwrap();
        fs::write(tmp.path().join("parent/a.bin"), vec![0u8; 200]).unwrap();
        fs::write(tmp.path().join("parent/child/b.bin"), vec![0u8; 300]).unwrap();

        let control = OperationControl::new();
        let result = analyze_directory(&dir(tmp.path()), &control).unwrap();

        let parent = result
            .largest_dirs
            .iter()
            .find(|d| d.name == "parent")
            .unwrap();
        assert_eq!(parent.size_bytes, 500);
        let child = result
            .largest_dirs
            .iter()
            .find(|d| d.name == "child")
            .unwrap();
        assert_eq!(child.size_bytes, 300);
    }

    #[test]
    fn duplicate_candidates_group_files_sharing_a_size_above_threshold() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = vec![0u8; DUPLICATE_MIN_SIZE as usize];
        fs::write(tmp.path().join("a.bin"), &payload).unwrap();
        fs::write(tmp.path().join("b.bin"), &payload).unwrap();
        fs::write(
            tmp.path().join("unique.bin"),
            vec![0u8; DUPLICATE_MIN_SIZE as usize + 1],
        )
        .unwrap();
        fs::write(tmp.path().join("tiny.txt"), b"hi").unwrap();

        let control = OperationControl::new();
        let result = analyze_directory(&dir(tmp.path()), &control).unwrap();

        assert_eq!(result.duplicate_candidates.len(), 1);
        let group = &result.duplicate_candidates[0];
        assert_eq!(group.size_bytes, DUPLICATE_MIN_SIZE);
        assert_eq!(group.paths.len(), 2);
    }

    #[test]
    fn duplicate_candidates_ignore_files_below_threshold() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a.txt"), b"same").unwrap();
        fs::write(tmp.path().join("b.txt"), b"same").unwrap();

        let control = OperationControl::new();
        let result = analyze_directory(&dir(tmp.path()), &control).unwrap();

        assert!(result.duplicate_candidates.is_empty());
    }

    #[test]
    fn symlinked_directory_is_not_traversed() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("real")).unwrap();
        fs::write(tmp.path().join("real/inner.bin"), vec![0u8; 500]).unwrap();
        std::os::unix::fs::symlink(tmp.path().join("real"), tmp.path().join("link")).unwrap();

        let control = OperationControl::new();
        let result = analyze_directory(&dir(tmp.path()), &control).unwrap();

        // The symlink is reported as a leaf (its own small size), not a
        // directory whose subtree gets counted a second time.
        assert_eq!(result.tree.direct_dir_count, 1);
        assert_eq!(result.tree.direct_file_count, 1);
    }

    #[test]
    fn cancelled_before_start_returns_empty_root() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a.bin"), vec![0u8; 100]).unwrap();

        let control = OperationControl::new();
        control.cancel();
        let result = analyze_directory(&dir(tmp.path()), &control).unwrap();

        assert_eq!(result.tree.size_bytes, 0);
        assert!(result.tree.children.is_empty());
    }

    #[test]
    fn nonexistent_root_is_a_hard_error() {
        let control = OperationControl::new();
        let result = analyze_directory(
            &dir("/nonexistent/does-not-exist-veyra-analyzer-test"),
            &control,
        );
        assert!(result.is_err());
    }
}
