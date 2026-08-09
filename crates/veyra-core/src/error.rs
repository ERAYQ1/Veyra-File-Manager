use std::path::PathBuf;

/// Shared error type for Veyra's core and library crates.
#[derive(Debug, thiserror::Error)]
pub enum VeyraError {
    #[error("HOME environment variable is not set; cannot resolve XDG directories")]
    MissingHomeDirectory,

    #[error("failed to create directory {path}: {source}")]
    DirectoryCreation {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
