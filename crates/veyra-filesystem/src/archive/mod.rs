//! Faz 19: archive extraction and creation for ZIP, TAR, TAR.GZ, TAR.XZ,
//! TAR.ZST and 7z. Blocking, like the rest of `veyra-filesystem` — always
//! called from a background thread (Rule #11).
//!
//! Extraction is hardened against Zip Slip / Path Traversal and symlink
//! attacks (Rule #21/#22): see `security.rs` for the details and why
//! archives never get to write an actual filesystem symlink.

mod compress;
mod extract;
mod format;
mod list_preview;
mod security;

pub use compress::create_archive;
pub use extract::{extract_archive, ArchiveOutcome, SkipReason};
pub use format::ArchiveFormat;
pub use list_preview::{list_preview, ArchivePreviewEntry};
