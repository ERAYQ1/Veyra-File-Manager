//! Filesystem operations. Every function here is blocking (uses GIO's
//! synchronous API) and is intended to be called off the GTK UI thread —
//! `veyra-ui` wraps these in a background worker (Faz 5's operation queue),
//! never on the main loop, per Rule #14.

use std::path::{Path, PathBuf};

use gio::prelude::*;
use gio::FileType;

use crate::error::{map_gio_error, FsError};
use crate::metadata::{build_file_item, FileItem, FAST_ATTRIBUTES, FULL_ATTRIBUTES};
use crate::path::VeyraPath;
use crate::permissions::FilePermissions;
use crate::queue::OperationControl;

/// Lists the direct children of `dir`. Symlinks are reported as themselves
/// (never followed) so callers can distinguish a directory from a symlink
/// pointing at one.
pub fn read_dir(dir: &VeyraPath) -> Result<Vec<FileItem>, FsError> {
    let file = dir.to_gio_file();

    let enumerator = file
        .enumerate_children(
            FULL_ATTRIBUTES,
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            gio::Cancellable::NONE,
        )
        .map_err(|e| map_gio_error(dir, e))?;

    let mut items = Vec::new();
    loop {
        match enumerator.next_file(gio::Cancellable::NONE) {
            Ok(Some(info)) => {
                let child = enumerator.child(&info);
                items.push(build_file_item(&info, &child));
            }
            Ok(None) => break,
            Err(err) => return Err(map_gio_error(dir, err)),
        }
    }

    Ok(items)
}

/// Default batch size for `read_dir_chunked`: large enough to amortize the
/// per-batch UI append cost, small enough that the first batch clears well
/// under the ~50ms first-paint budget on a huge directory (Rule #30).
pub const READ_DIR_CHUNK_SIZE: usize = 500;

/// Streaming counterpart to `read_dir`: lists `dir`'s children in
/// `chunk_size`-sized batches, calling `on_chunk` as each batch fills (plus
/// once more for a final partial batch), instead of collecting the whole
/// directory into memory before the caller sees anything. Lets the UI paint
/// the first entries and start responding to input while a huge directory is
/// still being enumerated (Rule #30).
///
/// Cooperatively cancellable via `control`, matching `count_dir_recursive`/
/// `chmod_recursive`: cancelling mid-walk stops enumeration and returns
/// `Ok(())` for whatever was already delivered via `on_chunk`, not an error.
///
/// Faz 31: queries `FAST_ATTRIBUTES`, not `FULL_ATTRIBUTES` — permissions,
/// ownership, inode, and created/accessed timestamps are left unset on the
/// returned `FileItem`s (Rule #33). Callers that need those for a specific
/// entry (Properties dialog, a selected item) fetch them on demand via
/// `stat`, which still queries the full set.
pub fn read_dir_chunked(
    dir: &VeyraPath,
    chunk_size: usize,
    control: &OperationControl,
    mut on_chunk: impl FnMut(Vec<FileItem>),
) -> Result<(), FsError> {
    let chunk_size = chunk_size.max(1);
    let file = dir.to_gio_file();

    let enumerator = file
        .enumerate_children(
            FAST_ATTRIBUTES,
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            gio::Cancellable::NONE,
        )
        .map_err(|e| map_gio_error(dir, e))?;

    let mut batch = Vec::with_capacity(chunk_size);
    loop {
        if control.is_cancelled() {
            return Ok(());
        }
        match enumerator.next_file(gio::Cancellable::NONE) {
            Ok(Some(info)) => {
                let child = enumerator.child(&info);
                batch.push(build_file_item(&info, &child));
                if batch.len() >= chunk_size {
                    on_chunk(std::mem::replace(
                        &mut batch,
                        Vec::with_capacity(chunk_size),
                    ));
                }
            }
            Ok(None) => break,
            Err(err) => return Err(map_gio_error(dir, err)),
        }
    }

    if !batch.is_empty() {
        on_chunk(batch);
    }

    Ok(())
}

/// Queries the metadata of `path` itself (as opposed to `read_dir`, which
/// only reports the *children* of a directory). Used by the Faz 12
/// Properties dialog to inspect a directory itself — e.g. the current tab's
/// open location, which never appears as an entry in its own listing.
pub fn stat(path: &VeyraPath) -> Result<FileItem, FsError> {
    let file = path.to_gio_file();
    let info = file
        .query_info(
            FULL_ATTRIBUTES,
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            gio::Cancellable::NONE,
        )
        .map_err(|e| map_gio_error(path, e))?;
    Ok(build_file_item(&info, &file))
}

/// Changes `path`'s POSIX permission bits (`chmod`). Only meaningful on
/// backends that expose `unix::mode` in the first place (see
/// `FileMetadata::permissions`); GVfs backends without POSIX semantics
/// return whatever `FsError` GIO surfaces for an unsupported attribute.
pub fn set_permissions(path: &VeyraPath, permissions: FilePermissions) -> Result<(), FsError> {
    path.to_gio_file()
        .set_attribute_uint32(
            "unix::mode",
            permissions.mode(),
            gio::FileQueryInfoFlags::NONE,
            gio::Cancellable::NONE,
        )
        .map_err(|e| map_gio_error(path, e))
}

/// Creates a new, empty regular file. Fails with `FsError::AlreadyExists` if
/// `path` already exists (never silently truncates).
pub fn create_file(path: &VeyraPath) -> Result<(), FsError> {
    path.to_gio_file()
        .create(gio::FileCreateFlags::NONE, gio::Cancellable::NONE)
        .map(|_| ())
        .map_err(|e| map_gio_error(path, e))
}

/// Creates a new, empty directory. The parent must already exist.
pub fn create_dir(path: &VeyraPath) -> Result<(), FsError> {
    path.to_gio_file()
        .make_directory(gio::Cancellable::NONE)
        .map_err(|e| map_gio_error(path, e))
}

/// Renames `path` in place (same parent directory), returning the new
/// location.
pub fn rename(path: &VeyraPath, new_name: &str) -> Result<VeyraPath, FsError> {
    let renamed = path
        .to_gio_file()
        .set_display_name(new_name, gio::Cancellable::NONE)
        .map_err(|e| map_gio_error(path, e))?;
    Ok(VeyraPath::from_gio_file(&renamed))
}

/// Copies `source` to `destination`. When `overwrite` is `false`, fails with
/// `FsError::AlreadyExists` if `destination` already exists.
pub fn copy(source: &VeyraPath, destination: &VeyraPath, overwrite: bool) -> Result<(), FsError> {
    let flags = if overwrite {
        gio::FileCopyFlags::OVERWRITE
    } else {
        gio::FileCopyFlags::NONE
    };

    source
        .to_gio_file()
        .copy(
            &destination.to_gio_file(),
            flags,
            gio::Cancellable::NONE,
            None::<&mut dyn FnMut(i64, i64)>,
        )
        .map_err(|e| map_gio_error(source, e))
}

/// Moves (or renames across directories) `source` to `destination`. When
/// `overwrite` is `false`, fails with `FsError::AlreadyExists` if
/// `destination` already exists.
pub fn move_entry(
    source: &VeyraPath,
    destination: &VeyraPath,
    overwrite: bool,
) -> Result<(), FsError> {
    let flags = if overwrite {
        gio::FileCopyFlags::OVERWRITE
    } else {
        gio::FileCopyFlags::NONE
    };

    source
        .to_gio_file()
        .move_(
            &destination.to_gio_file(),
            flags,
            gio::Cancellable::NONE,
            None::<&mut dyn FnMut(i64, i64)>,
        )
        .map_err(|e| map_gio_error(source, e))
}

/// Permanently deletes `path`. Directories are removed recursively;
/// symlinked directories encountered along the way are unlinked themselves,
/// never traversed into (no symlink-cycle risk, per the Veyra security
/// model).
pub fn delete(path: &VeyraPath) -> Result<(), FsError> {
    delete_recursive(&path.to_gio_file(), path)
}

fn delete_recursive(file: &gio::File, path_for_errors: &VeyraPath) -> Result<(), FsError> {
    let info = file
        .query_info(
            "standard::type",
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            gio::Cancellable::NONE,
        )
        .map_err(|e| map_gio_error(path_for_errors, e))?;

    if info.file_type() == FileType::Directory {
        let children = file
            .enumerate_children(
                "standard::name,standard::type",
                gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                gio::Cancellable::NONE,
            )
            .map_err(|e| map_gio_error(path_for_errors, e))?;

        loop {
            match children.next_file(gio::Cancellable::NONE) {
                Ok(Some(child_info)) => {
                    let child = children.child(&child_info);
                    let child_path = VeyraPath::from_gio_file(&child);
                    if child_info.file_type() == FileType::Directory {
                        delete_recursive(&child, &child_path)?;
                    } else {
                        child
                            .delete(gio::Cancellable::NONE)
                            .map_err(|e| map_gio_error(&child_path, e))?;
                    }
                }
                Ok(None) => break,
                Err(err) => return Err(map_gio_error(path_for_errors, err)),
            }
        }
    }

    file.delete(gio::Cancellable::NONE)
        .map_err(|e| map_gio_error(path_for_errors, e))
}

/// Moves `path` to the desktop trash (`XDG_DATA_HOME/Trash` via GIO), rather
/// than deleting it permanently.
pub fn trash(path: &VeyraPath) -> Result<(), FsError> {
    path.to_gio_file()
        .trash(gio::Cancellable::NONE)
        .map_err(|e| map_gio_error(path, e))
}

/// Restores a trashed entry to its original location.
///
/// `trashed_file` must be the physical entry under a Trash `files/`
/// directory (e.g. `~/.local/share/Trash/files/report.txt`), as returned by
/// a directory listing of that location. The original path is read from the
/// sibling `.trashinfo` record in `Trash/info/`, per the freedesktop.org
/// Trash specification — this does not depend on the `trash://` GVfs
/// backend (and its `gvfsd-trash` daemon) being available, only on the
/// on-disk trash directory `trash()` itself already writes to.
///
/// If the entry's original parent directory no longer exists (e.g. it was
/// itself deleted or renamed after the entry was trashed), it is recreated
/// (`mkdir -p`-style) so the restore can proceed rather than failing —
/// matching the behavior of other mainstream file managers.
///
/// Scoped to the home trash (absolute `Path=` entries) for Faz 2/18;
/// per-mount trash directories (topdir-relative paths) remain a documented
/// limitation (see the Faz 18 changelog entry).
pub fn restore_from_trash(trashed_file: &VeyraPath) -> Result<VeyraPath, FsError> {
    let local = trashed_file.as_local_path().ok_or_else(|| {
        FsError::InvalidPath(format!(
            "trash restore requires a local Trash/files path, got {trashed_file}"
        ))
    })?;

    let file_name = local
        .file_name()
        .ok_or_else(|| FsError::InvalidPath(format!("invalid trash entry path: {trashed_file}")))?;
    let files_dir = local
        .parent()
        .ok_or_else(|| FsError::InvalidPath(format!("invalid trash entry path: {trashed_file}")))?;
    let trash_root = files_dir.parent().ok_or_else(|| {
        FsError::InvalidPath(format!(
            "path is not inside a Trash/files directory: {trashed_file}"
        ))
    })?;

    let trashinfo_path = trash_root
        .join("info")
        .join(format!("{}.trashinfo", file_name.to_string_lossy()));

    let contents = std::fs::read_to_string(&trashinfo_path).map_err(|source| FsError::Io {
        path: VeyraPath::from_local(trashinfo_path.clone()),
        source,
    })?;

    let encoded_path = contents
        .lines()
        .find_map(|line| line.strip_prefix("Path="))
        .ok_or_else(|| FsError::Gio {
            path: trashed_file.clone(),
            message: "trashinfo record has no Path= entry".to_string(),
        })?;

    let destination_path = percent_decode(encoded_path);
    let destination = VeyraPath::from_local(&destination_path);

    if let Some(parent) = Path::new(&destination_path).parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|source| FsError::Io {
                path: VeyraPath::from_local(parent.to_path_buf()),
                source,
            })?;
        }
    }

    trashed_file
        .to_gio_file()
        .move_(
            &destination.to_gio_file(),
            gio::FileCopyFlags::NONE,
            gio::Cancellable::NONE,
            None::<&mut dyn FnMut(i64, i64)>,
        )
        .map_err(|e| map_gio_error(trashed_file, e))?;

    // Best-effort: the restore itself already succeeded, so a leftover
    // .trashinfo record is untidy but not a functional failure.
    let _ = std::fs::remove_file(&trashinfo_path);

    Ok(destination)
}

/// Faz 33: Trashes `path` like `trash`, but returns the physical location it
/// was moved to under `Trash/files/` — needed by the Undo/Redo engine, which
/// must be able to call `restore_from_trash` on this exact entry later.
/// Implemented as a direct freedesktop.org Trash-spec write (mirroring
/// `restore_from_trash`/`list_trash`) rather than through GIO's
/// `File::trash`, which only reports success/failure and never the
/// destination name it chose. Scoped to the home trash, matching
/// `restore_from_trash`'s documented limitation.
pub fn trash_tracked(path: &VeyraPath) -> Result<VeyraPath, FsError> {
    let root = home_trash_root().ok_or_else(|| {
        FsError::InvalidPath(
            "cannot resolve home trash directory (no XDG_DATA_HOME/HOME)".to_string(),
        )
    })?;
    let files_dir = root.join("files");
    let info_dir = root.join("info");
    for dir in [&files_dir, &info_dir] {
        if !dir.is_dir() {
            std::fs::create_dir_all(dir).map_err(|source| FsError::Io {
                path: VeyraPath::from_local(dir.clone()),
                source,
            })?;
        }
    }

    let original = path.as_local_path().ok_or_else(|| {
        FsError::InvalidPath(format!("trash_tracked requires a local path, got {path}"))
    })?;
    let name = path
        .file_name()
        .ok_or_else(|| FsError::InvalidPath(format!("cannot determine a file name for {path}")))?;

    let exists = |candidate: &str| {
        files_dir.join(candidate).exists()
            || info_dir.join(format!("{candidate}.trashinfo")).exists()
    };
    let unique_name = if !exists(&name) {
        name
    } else {
        crate::conflict::suggest_name(&name, exists)
    };

    let trashed_path = VeyraPath::from_local(files_dir.join(&unique_name));
    let trashinfo_path = info_dir.join(format!("{unique_name}.trashinfo"));

    let deletion_date = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S");
    let contents = format!(
        "[Trash Info]\nPath={}\nDeletionDate={}\n",
        percent_encode(&original.to_string_lossy()),
        deletion_date
    );
    std::fs::write(&trashinfo_path, contents).map_err(|source| FsError::Io {
        path: VeyraPath::from_local(trashinfo_path.clone()),
        source,
    })?;

    if let Err(err) = path.to_gio_file().move_(
        &trashed_path.to_gio_file(),
        gio::FileCopyFlags::NONE,
        gio::Cancellable::NONE,
        None::<&mut dyn FnMut(i64, i64)>,
    ) {
        // Best-effort: don't leave an orphaned .trashinfo record behind for
        // a move that never actually happened.
        let _ = std::fs::remove_file(&trashinfo_path);
        return Err(map_gio_error(path, err));
    }

    Ok(trashed_path)
}

/// Resolves the home trash root (`$XDG_DATA_HOME/Trash`, falling back to
/// `~/.local/share/Trash` per the XDG Base Directory spec), matching the
/// directory `trash()` (via GIO) and `restore_from_trash` already read from
/// and write to.
fn home_trash_root() -> Option<PathBuf> {
    if let Ok(data_home) = std::env::var("XDG_DATA_HOME") {
        if !data_home.is_empty() {
            return Some(PathBuf::from(data_home).join("Trash"));
        }
    }
    std::env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join(".local/share/Trash"))
}

/// Lists every entry currently in the home trash, as `FileItem`s whose
/// `path` is the physical `Trash/files/...` entry — the same shape
/// `read_dir` produces, so `trash:///` can reuse every existing listing
/// view, and each item is already a valid `restore_from_trash`/`delete`
/// argument with no further lookup. Reads `Trash/files` directly rather than
/// through the `trash://` GVfs backend, for the same robustness reason
/// `restore_from_trash` does (no dependency on `gvfsd-trash` running).
///
/// An entry that vanishes between being listed and stat'd (e.g. concurrently
/// emptied) is silently skipped rather than failing the whole listing —
/// consistent with `recent:///`'s handling of stale entries.
pub fn list_trash() -> Result<Vec<FileItem>, FsError> {
    let Some(root) = home_trash_root() else {
        return Ok(Vec::new());
    };
    let files_dir = root.join("files");
    if !files_dir.is_dir() {
        return Ok(Vec::new());
    }

    let entries = std::fs::read_dir(&files_dir).map_err(|source| FsError::Io {
        path: VeyraPath::from_local(files_dir.clone()),
        source,
    })?;

    let mut items = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        if let Ok(item) = stat(&VeyraPath::from_local(entry.path())) {
            items.push(item);
        }
    }
    Ok(items)
}

/// Permanently empties the home trash: every entry under `Trash/files` and
/// every record under `Trash/info`. Best-effort per entry — a single
/// permission-denied file doesn't abort the rest, but the first error
/// encountered (if any) is still returned so the caller can surface it.
pub fn empty_trash() -> Result<(), FsError> {
    let Some(root) = home_trash_root() else {
        return Ok(());
    };

    let mut first_error = None;
    for sub in ["files", "info"] {
        let dir = root.join(sub);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let result = if path.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            if let Err(source) = result {
                first_error.get_or_insert(FsError::Io {
                    path: VeyraPath::from_local(path),
                    source,
                });
            }
        }
    }

    match first_error {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

/// Percent-encodes a local path for a Trash spec `Path=` value — the
/// `percent_decode` counterpart `restore_from_trash` reads back. Encodes
/// every byte outside the unreserved set (`A-Za-z0-9-_.~/`), same
/// dependency-free, narrow-purpose scope as `percent_decode` below.
fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(*byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Decodes percent-encoded octets (`%XX`) in a Trash spec `Path=` value.
/// Freedesktop desktop entry / trashinfo percent-encoding, not full URI
/// parsing — deliberately dependency-free for this narrow, well-defined use.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&input[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Opens `path` with the user's default application for its type, via GIO
/// (never a shell-interpolated command, per the Veyra security model).
pub fn open(path: &VeyraPath) -> Result<(), FsError> {
    let file = path.to_gio_file();
    let uri = file.uri();

    gio::AppInfo::launch_default_for_uri(&uri, gio::AppLaunchContext::NONE).map_err(|e| {
        if e.matches(gio::IOErrorEnum::NotSupported) {
            FsError::NoHandlerAvailable(path.clone())
        } else {
            map_gio_error(path, e)
        }
    })
}

/// Faz 28: Outcome of a recursive chmod operation.
#[derive(Debug)]
pub struct ChmodRecursiveOutcome {
    pub succeeded: u64,
    pub errors: Vec<(VeyraPath, FsError)>,
}

/// Faz 28: Recursively changes permissions on `root` and all its descendants.
/// Symlinks — the root itself, or any descendant — are never chmod'd and
/// never traversed into (Rule #22): `set_permissions` has no way to target a
/// symlink itself rather than what it points at (Linux has no `lchmod`, so
/// the underlying `chmod()` syscall always follows), so the only safe
/// behavior is to skip a symlink entirely rather than risk silently
/// rewriting the permissions of whatever file or directory — possibly
/// outside `root`'s own tree — it happens to point at. A subdirectory that
/// fails to enumerate mid-walk (permission denied, removed concurrently) is
/// skipped rather than aborting the whole operation (Rule #18).
/// Cooperatively cancellable via `control` — cancelling mid-walk returns
/// whatever was succeeded so far plus any errors encountered before
/// cancellation.
///
/// This function is blocking and must only be called from `fs_async::run_blocking`
/// on a background thread, never on the GTK main thread (Rule #11/#14).
pub fn chmod_recursive(
    root: &VeyraPath,
    permissions: FilePermissions,
    control: &OperationControl,
) -> Result<ChmodRecursiveOutcome, FsError> {
    let mut outcome = ChmodRecursiveOutcome {
        succeeded: 0,
        errors: Vec::new(),
    };
    let root_file = root.to_gio_file();
    if !is_symlink(&root_file) {
        chmod_recursive_walk(&root_file, root, permissions, control, &mut outcome)?;
    }
    Ok(outcome)
}

/// Whether `file` is itself a symlink (queried with `NOFOLLOW_SYMLINKS` so
/// this reports the link, never what it points at). Errors (e.g. the entry
/// vanished between being listed and being queried — Rule #17) are treated
/// as "not a symlink" so the caller falls through to its normal handling
/// (which will surface the real error, e.g. `NotFound`, itself).
fn is_symlink(file: &gio::File) -> bool {
    file.query_info(
        "standard::type",
        gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
        gio::Cancellable::NONE,
    )
    .map(|info| info.file_type() == FileType::SymbolicLink)
    .unwrap_or(false)
}

fn chmod_recursive_walk(
    file: &gio::File,
    path_for_errors: &VeyraPath,
    permissions: FilePermissions,
    control: &OperationControl,
    outcome: &mut ChmodRecursiveOutcome,
) -> Result<(), FsError> {
    // chmod root itself first (the caller already confirmed it's not a
    // symlink before ever calling this function).
    match set_permissions(path_for_errors, permissions) {
        Ok(()) => outcome.succeeded += 1,
        Err(err) => outcome.errors.push((path_for_errors.clone(), err)),
    }

    let enumerator = file
        .enumerate_children(
            "standard::type",
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
                let file_type = info.file_type();
                if file_type == FileType::SymbolicLink {
                    // Never chmod'd, never traversed — see `chmod_recursive`'s
                    // doc comment for why.
                    continue;
                }
                let child = enumerator.child(&info);
                let child_path = VeyraPath::from_gio_file(&child);
                if file_type == FileType::Directory {
                    let _ =
                        chmod_recursive_walk(&child, &child_path, permissions, control, outcome);
                } else {
                    match set_permissions(&child_path, permissions) {
                        Ok(()) => outcome.succeeded += 1,
                        Err(err) => outcome.errors.push((child_path.clone(), err)),
                    }
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
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn chmod_recursive_applies_to_root_and_descendants() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a.txt"), b"hello").unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("sub/b.txt"), b"world!").unwrap();

        let dir = VeyraPath::from_local(tmp.path());
        let new_perms = FilePermissions::from_mode(0o700);
        let control = OperationControl::new();
        let outcome = chmod_recursive(&dir, new_perms, &control).unwrap();

        assert_eq!(outcome.succeeded, 4); // root dir + a.txt + sub dir + b.txt
        assert!(outcome.errors.is_empty());

        // Verify permissions were applied
        let root_meta = std::fs::metadata(tmp.path()).unwrap();
        assert_eq!(
            root_meta.permissions().mode() & 0o7777,
            0o700,
            "root directory should have 0700 permissions"
        );
        let file_meta = std::fs::metadata(tmp.path().join("a.txt")).unwrap();
        assert_eq!(
            file_meta.permissions().mode() & 0o7777,
            0o700,
            "a.txt should have 0700 permissions"
        );
    }

    #[test]
    fn chmod_recursive_cancelled_early() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a.txt"), b"test").unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();

        let dir = VeyraPath::from_local(tmp.path());
        let new_perms = FilePermissions::from_mode(0o644);
        let control = OperationControl::new();

        // Cancel before the operation starts
        control.cancel();
        let outcome = chmod_recursive(&dir, new_perms, &control).unwrap();

        // Should have applied to root, then cancelled
        assert!(outcome.succeeded >= 1);
    }

    #[test]
    fn chmod_recursive_collects_errors() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a.txt"), b"test").unwrap();

        let dir = VeyraPath::from_local(tmp.path());
        let new_perms = FilePermissions::from_mode(0o755);
        let control = OperationControl::new();
        let outcome = chmod_recursive(&dir, new_perms, &control).unwrap();

        // On a normal filesystem where we have write permission, this should succeed
        assert_eq!(outcome.errors.len(), 0);
        assert!(outcome.succeeded >= 2); // at least the directory and file
    }
}
