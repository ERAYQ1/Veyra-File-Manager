//! Filesystem operations. Every function here is blocking (uses GIO's
//! synchronous API) and is intended to be called off the GTK UI thread —
//! `veyra-ui` wraps these in a background worker (Faz 5's operation queue),
//! never on the main loop, per Rule #14.

use gio::prelude::*;
use gio::FileType;

use crate::error::{map_gio_error, FsError};
use crate::metadata::{build_file_item, FileItem, FULL_ATTRIBUTES};
use crate::path::VeyraPath;

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
/// Scoped to the home trash (absolute `Path=` entries) for Faz 2; per-mount
/// trash directories (topdir-relative paths) are deferred to Faz 18 (Trash
/// Integration).
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

    let destination = VeyraPath::from_local(percent_decode(encoded_path));

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
