//! Recursive directory content counting for the Faz 12 Properties dialog's
//! "Contains" field: total descendant file/directory counts and cumulative
//! size. This is the one Properties computation that can take a long time
//! on huge trees, so it's cooperatively cancellable via the same
//! `OperationControl` the Faz 5 bulk-operation engine already uses (Rule
//! #13) — the UI wraps it in `fs_async::run_blocking` and cancels it when
//! the dialog closes.

use gio::prelude::*;

use crate::error::{map_gio_error, FsError};
use crate::path::VeyraPath;
use crate::queue::OperationControl;

/// Descendant counts and total size accumulated under a directory (not
/// including the directory itself).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DirCount {
    pub file_count: u64,
    pub dir_count: u64,
    pub total_size: u64,
}

/// Recursively counts everything under `dir`. Symlinked directories are
/// counted as entries but never traversed into — no symlink-cycle risk,
/// matching `ops::delete`'s recursion policy (Rule #22). A subdirectory
/// that fails to enumerate mid-walk (permission denied, removed
/// concurrently) is skipped rather than aborting the whole count (Rule
/// #18); only a failure enumerating `dir` itself is a hard error.
/// Cancelling `control` mid-walk simply stops early and returns whatever
/// was accumulated so far as `Ok`, not an error.
pub fn count_dir_recursive(
    dir: &VeyraPath,
    control: &OperationControl,
) -> Result<DirCount, FsError> {
    let mut count = DirCount::default();
    walk(&dir.to_gio_file(), dir, control, &mut count)?;
    Ok(count)
}

fn walk(
    file: &gio::File,
    path_for_errors: &VeyraPath,
    control: &OperationControl,
    count: &mut DirCount,
) -> Result<(), FsError> {
    let enumerator = file
        .enumerate_children(
            "standard::type,standard::size",
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            gio::Cancellable::NONE,
        )
        .map_err(|e| map_gio_error(path_for_errors, e))?;

    loop {
        if control.is_cancelled() {
            return Ok(());
        }
        match enumerator.next_file(gio::Cancellable::NONE) {
            Ok(Some(info)) => {
                let child = enumerator.child(&info);
                if info.file_type() == gio::FileType::Directory {
                    count.dir_count += 1;
                    let child_path = VeyraPath::from_gio_file(&child);
                    let _ = walk(&child, &child_path, control, count);
                } else {
                    count.file_count += 1;
                    count.total_size += info.size().max(0) as u64;
                }
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn counts_files_and_dirs_recursively() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a.txt"), b"hello").unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("sub/b.txt"), b"world!").unwrap();

        let dir = VeyraPath::from_local(tmp.path());
        let control = OperationControl::new();
        let count = count_dir_recursive(&dir, &control).unwrap();

        assert_eq!(count.file_count, 2);
        assert_eq!(count.dir_count, 1);
        assert_eq!(count.total_size, 11);
    }

    #[test]
    fn cancelled_before_start_returns_empty_ok() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a.txt"), b"hello").unwrap();

        let dir = VeyraPath::from_local(tmp.path());
        let control = OperationControl::new();
        control.cancel();
        let count = count_dir_recursive(&dir, &control).unwrap();

        assert_eq!(count, DirCount::default());
    }

    #[test]
    fn nonexistent_dir_is_a_hard_error() {
        let dir = VeyraPath::from_local("/nonexistent/does-not-exist-veyra-test");
        let control = OperationControl::new();
        assert!(count_dir_recursive(&dir, &control).is_err());
    }
}
