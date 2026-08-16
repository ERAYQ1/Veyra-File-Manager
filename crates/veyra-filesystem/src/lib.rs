//! Veyra filesystem abstraction layer: a unified, GIO-backed model over
//! local and remote (GVfs) locations, plus the blocking CRUD/trash/open
//! operations built on top of it. Contains no UI, GTK or Tokio code —
//! `veyra-ui` calls into this crate from background workers only.

#![forbid(unsafe_code)]

mod advanced;
mod analyzer;
mod archive;
mod conflict;
mod dircount;
mod error;
mod kind;
mod metadata;
mod ops;
mod path;
mod permissions;
mod progress;
mod queue;

pub use advanced::{stat_advanced, AdvancedInfo};
pub use analyzer::{analyze_directory, AnalysisResult, DuplicateGroup, UsageEntry, UsageNode};
pub use archive::{create_archive, extract_archive, ArchiveFormat, ArchiveOutcome, SkipReason};
pub use conflict::{suggest_name, Conflict, ConflictDecision};
pub use dircount::{count_dir_recursive, DirCount};
pub use error::FsError;
pub use kind::FileKind;
pub use metadata::{format_size, FileItem, FileMetadata};
pub use ops::{
    chmod_recursive, copy, create_dir, create_file, delete, empty_trash, list_trash, move_entry,
    open, read_dir, read_dir_chunked, rename, restore_from_trash, set_permissions, stat, trash,
    ChmodRecursiveOutcome, READ_DIR_CHUNK_SIZE,
};
pub use path::VeyraPath;
pub use permissions::FilePermissions;
pub use progress::Progress;
pub use queue::{
    run_operation, OperationControl, OperationKind, OperationOutcome, OperationRequest,
};
