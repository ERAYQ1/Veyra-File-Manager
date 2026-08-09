use chrono::{DateTime, Utc};

use crate::kind::FileKind;
use crate::path::VeyraPath;
use crate::permissions::FilePermissions;

const SIZE_UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

/// Formats a byte count as a human-readable string using 1024-based units
/// (`B`, `KB`, `MB`, `GB`, `TB`), e.g. `format_size(1_536)` -> `"1.5 KB"`.
pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }

    let mut value = bytes as f64;
    let mut unit_index = 0;
    while value >= 1024.0 && unit_index < SIZE_UNITS.len() - 1 {
        value /= 1024.0;
        unit_index += 1;
    }

    format!("{value:.1} {}", SIZE_UNITS[unit_index])
}

/// Structured, backend-agnostic metadata for a single filesystem entry.
#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub name: String,
    pub path: VeyraPath,
    pub kind: FileKind,
    pub size_bytes: u64,
    pub modified: Option<DateTime<Utc>>,
    pub created: Option<DateTime<Utc>>,
    pub accessed: Option<DateTime<Utc>>,
    /// `None` on backends that don't expose POSIX mode bits (some GVfs
    /// mounts). Never fabricated as a fake default.
    pub permissions: Option<FilePermissions>,
    pub owner: Option<String>,
    pub group: Option<String>,
    pub mime_type: String,
    pub inode: Option<u64>,
    pub is_hidden: bool,
}

impl FileMetadata {
    pub fn size_human(&self) -> String {
        format_size(self.size_bytes)
    }

    pub fn is_executable(&self) -> bool {
        self.permissions.is_some_and(|p| p.is_executable())
    }
}

/// A single filesystem entry as returned by `veyra-filesystem` operations:
/// its location, structured metadata, and (for symlinks) resolved target.
#[derive(Debug, Clone)]
pub struct FileItem {
    pub path: VeyraPath,
    pub metadata: FileMetadata,
    pub target_symlink: Option<VeyraPath>,
}

impl FileItem {
    pub fn name(&self) -> &str {
        &self.metadata.name
    }

    pub fn kind(&self) -> &FileKind {
        &self.metadata.kind
    }
}

/// Attributes requested from GIO for every directory entry. Kept as a single
/// constant so `read_dir` and `delete`'s directory-type probe stay aligned.
pub(crate) const FULL_ATTRIBUTES: &str = concat!(
    "standard::name,standard::display-name,standard::type,standard::size,",
    "standard::content-type,standard::is-hidden,standard::is-symlink,standard::symlink-target,",
    "unix::mode,unix::uid,unix::gid,unix::inode,",
    "owner::user,owner::group,",
    "time::modified,time::created,time::access",
);

pub(crate) fn build_file_item(info: &gio::FileInfo, child: &gio::File) -> FileItem {
    let path = VeyraPath::from_gio_file(child);
    let kind = FileKind::from_gio(info, child);

    let target_symlink = match &kind {
        FileKind::Symlink {
            target: Some(t), ..
        } => Some(VeyraPath::from_local(t.clone())),
        _ => None,
    };

    let name = info.display_name().to_string();

    let mime_type = info
        .content_type()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            mime_guess::from_path(&name)
                .first_or_octet_stream()
                .essence_str()
                .to_string()
        });

    let permissions = has_attribute(info, "unix::mode")
        .then(|| FilePermissions::from_mode(info.attribute_uint32("unix::mode")));

    let metadata = FileMetadata {
        name,
        path: path.clone(),
        kind,
        size_bytes: info.size().max(0) as u64,
        modified: attribute_time(info, "time::modified"),
        created: attribute_time(info, "time::created"),
        accessed: attribute_time(info, "time::access"),
        permissions,
        owner: info.attribute_string("owner::user").map(|s| s.to_string()),
        group: info.attribute_string("owner::group").map(|s| s.to_string()),
        mime_type,
        inode: has_attribute(info, "unix::inode").then(|| info.attribute_uint64("unix::inode")),
        is_hidden: info.is_hidden(),
    };

    FileItem {
        path,
        metadata,
        target_symlink,
    }
}

fn has_attribute(info: &gio::FileInfo, key: &str) -> bool {
    info.has_attribute(key)
}

fn attribute_time(info: &gio::FileInfo, key: &str) -> Option<DateTime<Utc>> {
    if !has_attribute(info, key) {
        return None;
    }
    let secs = info.attribute_uint64(key);
    if secs == 0 {
        return None;
    }
    DateTime::<Utc>::from_timestamp(secs as i64, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
    }

    #[test]
    fn format_size_kilobytes() {
        assert_eq!(format_size(1536), "1.5 KB");
    }

    #[test]
    fn format_size_megabytes_and_up() {
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(format_size(2 * 1024 * 1024 * 1024), "2.0 GB");
        assert_eq!(format_size(3 * 1024_u64.pow(4)), "3.0 TB");
    }

    #[test]
    fn format_size_caps_at_terabytes() {
        // Larger than TB should still render in TB, not overflow the unit table.
        let huge = 4096_u64 * 1024_u64.pow(4);
        assert!(format_size(huge).ends_with(" TB"));
    }
}
