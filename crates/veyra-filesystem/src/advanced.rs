//! Advanced/inode-level filesystem metadata for the Faz 12 Properties
//! dialog's Advanced page: device id and on-disk filesystem type, plus
//! actual allocated disk usage — none of which is part of the directory
//! listing's `FULL_ATTRIBUTES` (Faz 12 is the first caller that needs them,
//! and they cost an extra `stat`/`statvfs`, so they stay opt-in).

use gio::prelude::*;

use crate::error::{map_gio_error, FsError};
use crate::path::VeyraPath;

/// Device/filesystem-level facts about a single entry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AdvancedInfo {
    /// The device containing `path` (`st_dev`), when the backend reports
    /// one.
    pub device_id: Option<u64>,
    /// Actual allocated space (`st_blocks * 512`), which can differ from
    /// the entry's logical size for sparse files or filesystem block
    /// rounding.
    pub disk_usage_bytes: Option<u64>,
    /// The mounted filesystem type (e.g. `ext4`, `btrfs`, `tmpfs`, `vfat`),
    /// when the backend reports one.
    pub filesystem_type: Option<String>,
    /// Faz 39: the entry's hardlink count (`st_nlink`), for the Developer
    /// Metadata Inspector.
    pub hard_link_count: Option<u64>,
}

/// Queries `path`'s device id, allocated disk usage, and containing
/// filesystem's type. Never follows symlinks for the entry stat itself
/// (matches every other `veyra-filesystem` query).
pub fn stat_advanced(path: &VeyraPath) -> Result<AdvancedInfo, FsError> {
    let file = path.to_gio_file();

    let info = file
        .query_info(
            "unix::device,unix::blocks,unix::nlink",
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            gio::Cancellable::NONE,
        )
        .map_err(|e| map_gio_error(path, e))?;

    let device_id = info
        .has_attribute("unix::device")
        .then(|| info.attribute_uint32("unix::device") as u64);
    let disk_usage_bytes = info
        .has_attribute("unix::blocks")
        .then(|| info.attribute_uint64("unix::blocks") * 512);
    let hard_link_count = info
        .has_attribute("unix::nlink")
        .then(|| info.attribute_uint32("unix::nlink") as u64);

    // Best-effort: some backends (or a filesystem query racing a mount
    // change) can fail here without that being a reason to fail the whole
    // call — the entry stat above already succeeded and is the useful part.
    let filesystem_type = file
        .query_filesystem_info("filesystem::type", gio::Cancellable::NONE)
        .ok()
        .and_then(|fs_info| fs_info.attribute_string("filesystem::type"))
        .map(|s| s.to_string());

    Ok(AdvancedInfo {
        device_id,
        disk_usage_bytes,
        filesystem_type,
        hard_link_count,
    })
}
