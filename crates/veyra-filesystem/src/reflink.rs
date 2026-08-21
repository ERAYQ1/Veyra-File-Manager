//! Faz 64: reflink (copy-on-write clone) fast path for same-filesystem file
//! copies on Btrfs/XFS/ZFS/bcachefs. `ioctl(dest_fd, FICLONE, src_fd)` tells
//! the kernel to share the source's extents with the destination inode
//! instead of copying bytes — a 50GB file "copies" in about a millisecond
//! and consumes ~0 extra disk space until either copy is later modified
//! (the filesystem's own copy-on-write then splits the shared extents apart
//! lazily, per-block, same as any other CoW clone).
//!
//! This is purely a fast path in front of `queue.rs`'s existing
//! `gio::File::copy` byte-copy loop: any failure — different filesystem,
//! different device, unsupported filesystem type, a remote/GVfs location,
//! or any other unexpected error — is treated as "not available here" and
//! silently falls back to that existing path, never surfaced as an error of
//! its own (this module's public function returns `bool`, not `Result`).

use std::fs::File;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use rustix::fs::{futimens, ioctl_ficlone, Timespec, Timestamps};
use rustix::io::Errno;

use crate::path::VeyraPath;

/// Attempts to clone `source` into `dest` via `FICLONE`. Returns `true` if
/// the clone succeeded (dest now holds a full, independent-looking copy of
/// source's contents, permissions, and timestamps — the caller should treat
/// this file as fully written and skip its normal copy path for it).
/// Returns `false` for anything else — unsupported filesystem/device pair,
/// a remote location, or a genuine I/O error probing either file — in which
/// case the caller must fall back to a normal copy; the destination is left
/// exactly as it was before this call (any empty stub this function itself
/// created on a failed attempt is cleaned up before returning).
pub fn try_reflink_clone(source: &VeyraPath, dest: &VeyraPath) -> bool {
    let (Some(src_path), Some(dest_path)) = (source.as_local_path(), dest.as_local_path()) else {
        // Reflink is a local-block-filesystem-only optimization; GVfs/remote
        // locations always take the byte-copy path.
        return false;
    };
    match try_reflink_clone_local(src_path, dest_path) {
        Ok(cloned) => cloned,
        Err(err) => {
            tracing::debug!(
                error = %err,
                source = %src_path.display(),
                dest = %dest_path.display(),
                "reflink clone attempt failed, falling back to byte copy"
            );
            false
        }
    }
}

fn try_reflink_clone_local(source: &Path, dest: &Path) -> io::Result<bool> {
    let src_file = File::open(source)?;
    let src_meta = src_file.metadata()?;
    if !src_meta.is_file() {
        // FICLONE only clones regular file data; directories/symlinks/
        // special files always take the normal copy path.
        return Ok(false);
    }

    let dest_existed = dest.exists();
    let dest_file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(dest)?;

    // `ioctl_ficlone` (`FICLONE`, `_IOW(0x94, 9, int)`) tells the kernel to
    // share `src_file`'s extents into `dest_file` instead of copying bytes;
    // rustix wraps the raw ioctl safely, so no `unsafe` is needed here.
    if let Err(errno) = ioctl_ficlone(&dest_file, &src_file) {
        drop(dest_file);
        if !dest_existed {
            // We created this empty stub ourselves; remove it so the
            // caller's fallback copy (which may itself require the
            // destination not to already exist) starts from a clean slate.
            let _ = std::fs::remove_file(dest);
        }
        return match errno {
            // OPNOTSUPP: filesystem doesn't implement reflink at all.
            // NOTTY: not even a filesystem that recognizes the ioctl.
            // XDEV: source and destination are on different filesystems.
            // INVAL: e.g. overlapping ranges, or one side isn't a regular
            // file in a way `is_file()` above didn't already catch.
            Errno::OPNOTSUPP | Errno::NOTTY | Errno::XDEV | Errno::INVAL => Ok(false),
            _ => Err(errno.into()),
        };
    }

    // Clone succeeded — sync permissions and timestamps so the clone is
    // indistinguishable from a byte-for-byte copy, not just in content.
    std::fs::set_permissions(dest, src_meta.permissions())?;
    sync_timestamps(&dest_file, &src_meta)?;

    Ok(true)
}

/// Copies `src_meta`'s access/modification timestamps onto `dest_file`,
/// mirroring what a normal `gio::File::copy` preserves.
fn sync_timestamps(dest_file: &File, src_meta: &std::fs::Metadata) -> io::Result<()> {
    let times = Timestamps {
        last_access: Timespec {
            tv_sec: src_meta.atime(),
            tv_nsec: src_meta.atime_nsec() as _,
        },
        last_modification: Timespec {
            tv_sec: src_meta.mtime(),
            tv_nsec: src_meta.mtime_nsec() as _,
        },
    };
    futimens(dest_file, &times).map_err(io::Error::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn reflink_clone_produces_byte_identical_content() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source.bin");
        let dest = tmp.path().join("dest.bin");
        std::fs::write(&source, b"hello reflink world").unwrap();

        let cloned = try_reflink_clone(
            &VeyraPath::from_local(&source),
            &VeyraPath::from_local(&dest),
        );

        // Either this filesystem supports reflink (tmpfs on most CI/dev
        // boxes does not, so this is typically `false`) or it doesn't —
        // both are valid outcomes here. What matters is: if it claims
        // success, the result must actually be correct.
        if cloned {
            assert_eq!(
                std::fs::read(&dest).unwrap(),
                std::fs::read(&source).unwrap()
            );
        } else {
            assert!(
                !dest.exists(),
                "a failed clone must not leave a stub file behind"
            );
        }
    }

    #[test]
    fn reflink_clone_preserves_permissions_and_size_on_success() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("perm_source.bin");
        let dest = tmp.path().join("perm_dest.bin");
        std::fs::write(&source, vec![b'x'; 4096]).unwrap();
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o640)).unwrap();

        if try_reflink_clone(
            &VeyraPath::from_local(&source),
            &VeyraPath::from_local(&dest),
        ) {
            let src_meta = std::fs::metadata(&source).unwrap();
            let dest_meta = std::fs::metadata(&dest).unwrap();
            assert_eq!(dest_meta.len(), src_meta.len());
            assert_eq!(dest_meta.permissions().mode() & 0o777, 0o640);
        }
    }

    #[test]
    fn reflink_clone_on_a_directory_is_not_attempted() {
        let tmp = tempfile::tempdir().unwrap();
        let source_dir = tmp.path().join("a_dir");
        std::fs::create_dir(&source_dir).unwrap();
        let dest = tmp.path().join("dest_dir_target");

        assert!(!try_reflink_clone(
            &VeyraPath::from_local(&source_dir),
            &VeyraPath::from_local(&dest)
        ));
        assert!(!dest.exists());
    }

    #[test]
    fn reflink_clone_on_a_remote_uri_is_not_attempted() {
        let source = VeyraPath::from_uri("sftp://example.com/file.txt");
        let dest = VeyraPath::from_uri("sftp://example.com/copy.txt");
        assert!(!try_reflink_clone(&source, &dest));
    }

    #[test]
    fn reflink_clone_on_a_missing_source_is_not_attempted() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("does_not_exist.bin");
        let dest = tmp.path().join("dest.bin");

        assert!(!try_reflink_clone(
            &VeyraPath::from_local(&source),
            &VeyraPath::from_local(&dest)
        ));
        assert!(!dest.exists());
    }
}
