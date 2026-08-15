use crate::path::VeyraPath;

/// Errors surfaced by every `veyra-filesystem` operation. Never constructed
/// from a `panic!`/`unwrap!` path — always the direct `Result` mapping of an
/// underlying `std::io` or GIO failure, per Rule #17/#18 (never panic on a
/// normal user filesystem error).
#[derive(Debug, thiserror::Error)]
pub enum FsError {
    #[error("not found: {0}")]
    NotFound(VeyraPath),

    #[error("permission denied: {0}")]
    PermissionDenied(VeyraPath),

    #[error("already exists: {0}")]
    AlreadyExists(VeyraPath),

    #[error("not a directory: {0}")]
    NotADirectory(VeyraPath),

    #[error("directory is not empty: {0}")]
    NotEmpty(VeyraPath),

    #[error("filesystem is read-only: {0}")]
    ReadOnly(VeyraPath),

    #[error("invalid path: {0}")]
    InvalidPath(String),

    #[error("no application registered to open {0}")]
    NoHandlerAvailable(VeyraPath),

    #[error("{path}: {message}")]
    Gio { path: VeyraPath, message: String },

    #[error("{path}: {source}")]
    Io {
        path: VeyraPath,
        #[source]
        source: std::io::Error,
    },

    #[error("archive error: {0}")]
    Archive(String),
}

/// Maps a `glib::Error` returned by a GIO call against `path` into the
/// appropriate typed `FsError`, falling back to `FsError::Gio` for kinds we
/// don't special-case.
pub(crate) fn map_gio_error(path: &VeyraPath, error: glib::Error) -> FsError {
    use gio::IOErrorEnum;

    match error.kind::<IOErrorEnum>() {
        Some(IOErrorEnum::NotFound) => FsError::NotFound(path.clone()),
        Some(IOErrorEnum::PermissionDenied) => FsError::PermissionDenied(path.clone()),
        Some(IOErrorEnum::Exists) => FsError::AlreadyExists(path.clone()),
        Some(IOErrorEnum::NotDirectory) => FsError::NotADirectory(path.clone()),
        Some(IOErrorEnum::NotEmpty) => FsError::NotEmpty(path.clone()),
        Some(IOErrorEnum::ReadOnly) => FsError::ReadOnly(path.clone()),
        _ => FsError::Gio {
            path: path.clone(),
            message: error.message().to_string(),
        },
    }
}
