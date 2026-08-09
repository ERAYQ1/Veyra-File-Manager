use std::path::PathBuf;

use thiserror::Error;

/// Errors from opening or querying the Faz 9 search index.
#[derive(Debug, Error)]
pub enum SearchError {
    #[error("failed to create search index directory at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("search index database error: {0}")]
    Database(#[from] rusqlite::Error),
}
