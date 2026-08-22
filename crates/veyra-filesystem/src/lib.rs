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
mod duplicates;
mod error;
mod git;
mod kind;
mod metadata;
mod ops;
mod path;
mod permissions;
mod progress;
mod queue;
mod reflink;
pub use reflink::set_reflink_enabled;
mod tags;

pub use advanced::{stat_advanced, AdvancedInfo};
pub use analyzer::{
    analyze_directory, AnalysisResult, SameSizeCandidateGroup, UsageEntry, UsageNode,
};
pub use archive::{
    create_archive, extract_archive, list_preview, set_compression_level, ArchiveFormat,
    ArchiveOutcome, ArchivePreviewEntry, SkipReason,
};
pub use conflict::{suggest_name, Conflict, ConflictDecision};
pub use dircount::{count_dir_recursive, DirCount};
pub use duplicates::{find_duplicates, DuplicateGroup};
pub use error::FsError;
pub use git::{
    find_git_root, git_status, query_dir_git_statuses, GitFileStatus, GitRepoStatus, GitStatusError,
};
pub use kind::FileKind;
pub use metadata::{format_size, FileItem, FileMetadata};
pub use ops::{
    chmod_recursive, copy, create_dir, create_file, delete, empty_trash, list_trash, move_entry,
    open, read_dir, read_dir_chunked, rename, restore_from_trash, set_permissions, stat, trash,
    trash_tracked, ChmodRecursiveOutcome, READ_DIR_CHUNK_SIZE,
};
pub use path::VeyraPath;
pub use permissions::FilePermissions;
pub use progress::Progress;
pub use queue::{
    run_operation, OperationControl, OperationKind, OperationOutcome, OperationRequest,
};
pub use tags::{
    clear_all_tags, clear_unused_tags, get_custom_tag_name, get_paths_by_tag, get_tag,
    list_all_tagged, remove_tag, reset_custom_tag_names, set_custom_tag_name, set_tag, tags_path,
    TagColor,
};
