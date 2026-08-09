use std::path::PathBuf;

use gio::prelude::*;

/// The structural type of a filesystem entry.
///
/// `Hidden` and `Executable` are deliberately *not* variants here: they are
/// orthogonal attributes (a directory can be hidden, an executable can be a
/// symlink) and are exposed as separate flags on `FileMetadata` instead, so
/// they can be combined freely without illegal enum states.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FileKind {
    Regular,
    Directory,
    /// A symbolic link. `target` is the raw link target (unresolved, as
    /// stored on disk) when it could be read; `is_broken` is `true` when the
    /// target does not resolve to an existing entry.
    Symlink {
        target: Option<PathBuf>,
        is_broken: bool,
    },
    Fifo,
    Socket,
    BlockDevice,
    CharDevice,
    /// Entry exists but its type could not be determined (corrupt inode,
    /// exotic GVfs backend, etc.). Never used as a substitute for a proper
    /// error — only when the entry is enumerable but untypeable.
    Unknown,
}

impl FileKind {
    pub fn is_directory(&self) -> bool {
        matches!(self, FileKind::Directory)
    }

    pub fn is_symlink(&self) -> bool {
        matches!(self, FileKind::Symlink { .. })
    }

    pub fn is_broken_symlink(&self) -> bool {
        matches!(
            self,
            FileKind::Symlink {
                is_broken: true,
                ..
            }
        )
    }

    /// Determines the structural kind of `child` from its (non-following)
    /// `FileInfo`. For symlinks, additionally probes whether the target
    /// resolves, without ever following the link for the *primary* query
    /// (the caller must have queried `child` with
    /// `FileQueryInfoFlags::NOFOLLOW_SYMLINKS`).
    pub(crate) fn from_gio(info: &gio::FileInfo, child: &gio::File) -> Self {
        use gio::FileType;

        if info.file_type() == FileType::SymbolicLink || info.is_symlink() {
            let target = info.symlink_target();
            let is_broken = symlink_target_missing(child);
            return FileKind::Symlink { target, is_broken };
        }

        if let Some(mode) = info
            .has_attribute("unix::mode")
            .then(|| info.attribute_uint32("unix::mode"))
        {
            return kind_from_unix_mode(mode);
        }

        match info.file_type() {
            FileType::Regular => FileKind::Regular,
            FileType::Directory => FileKind::Directory,
            _ => FileKind::Unknown,
        }
    }
}

fn kind_from_unix_mode(mode: u32) -> FileKind {
    const S_IFMT: u32 = 0o170000;
    const S_IFSOCK: u32 = 0o140000;
    const S_IFREG: u32 = 0o100000;
    const S_IFBLK: u32 = 0o060000;
    const S_IFDIR: u32 = 0o040000;
    const S_IFCHR: u32 = 0o020000;
    const S_IFIFO: u32 = 0o010000;

    match mode & S_IFMT {
        S_IFSOCK => FileKind::Socket,
        S_IFREG => FileKind::Regular,
        S_IFBLK => FileKind::BlockDevice,
        S_IFDIR => FileKind::Directory,
        S_IFCHR => FileKind::CharDevice,
        S_IFIFO => FileKind::Fifo,
        _ => FileKind::Unknown,
    }
}

/// Best-effort existence probe for a symlink's target.
///
/// GIO's local backend does not raise an error for a dangling symlink when
/// queried with symlink-following flags: it silently falls back to the
/// link's own (unfollowed) info. So a resolved type of `SymbolicLink` here
/// — meaning "following did not actually reach a real target" — is the
/// signal, not an `Err`. An explicit `NotFound` (seen on some GVfs backends)
/// is treated the same way. Any other failure (permission denied,
/// unsupported backend, ...) is treated as "unknown, assume valid" so the UI
/// never shows a false broken-link warning it can't actually justify.
fn symlink_target_missing(child: &gio::File) -> bool {
    match child.query_info(
        "standard::type",
        gio::FileQueryInfoFlags::NONE,
        gio::Cancellable::NONE,
    ) {
        Ok(info) => info.file_type() == gio::FileType::SymbolicLink,
        Err(err) => err.matches(gio::IOErrorEnum::NotFound),
    }
}
