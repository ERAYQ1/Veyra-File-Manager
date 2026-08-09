//! Veyra search engine: a SQLite + FTS5 full-text index, a resource-aware
//! background indexer, and the `name:`/`type:`/`size:`/`modified:` query
//! parser (Faz 9).
//!
//! Deliberately decoupled from both `veyra-ui` (Rule #42: search stays
//! independent of the UI layer) and `veyra-filesystem` (the indexer walks
//! plain `std::fs`, not GIO — it runs on its own background thread and
//! must never depend on the GLib main context the UI owns).
//!
//! Not `#![forbid(unsafe_code)]` like this workspace's other crates: the
//! background indexer makes one deliberate, documented `unsafe` call
//! (`libc::nice`, in `indexer::lower_priority`) to deprioritize itself —
//! isolated behind a `# Safety` doc comment per Rule #10, the same pattern
//! `veyra-app::root_guard` already uses for `libc::geteuid`.

mod classify;
mod error;
mod index;
mod indexer;
mod query;
mod schema;

pub use classify::classify;
pub use error::SearchError;
pub use index::{IndexedEntry, SearchIndex, SearchResult};
pub use indexer::spawn_background_index;
pub use query::{parse, FileTypeFilter, ModifiedPreset, ParsedQuery, SizeFilter, SizeOp};

use std::path::{Path, PathBuf};

/// Default location of the on-disk search index:
/// `<cache_dir>/search_index.db` (typically `~/.cache/veyra/search_index.db`
/// — see `veyra_core::XdgDirs::cache_dir`).
pub fn default_db_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join("search_index.db")
}
