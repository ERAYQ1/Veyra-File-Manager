//! Faz 17: Devices & Volumes. Scans `gio::VolumeMonitor` for the root
//! filesystem, every active mount, and every volume that isn't mounted yet
//! (a USB stick plugged in but not opened, an optical disc not yet
//! accessed) — `sidebar.rs` renders the result and stays live via the
//! monitor's hotplug signals. Filesystem usage (`query_filesystem_info`)
//! always runs off the GTK main thread via `fs_async::run_blocking`
//! (Rule #11/#12), since a slow network mount can stall a `statvfs`-style
//! call for seconds.

use gtk4::gio;
use gtk4::prelude::*;

use veyra_filesystem::VeyraPath;

/// Coarse device classification driving icon choice. Kept separate from the
/// live `gio` types so `classify` is a pure, directly unit-testable
/// function — the messy "how do we know a `Drive` is removable" heuristics
/// live in `scan` instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeviceKind {
    System,
    InternalDisk,
    Removable,
    Optical,
    Network,
}

pub(crate) fn classify(is_root: bool, removable: bool, optical: bool, network: bool) -> DeviceKind {
    if is_root {
        DeviceKind::System
    } else if optical {
        DeviceKind::Optical
    } else if network {
        DeviceKind::Network
    } else if removable {
        DeviceKind::Removable
    } else {
        DeviceKind::InternalDisk
    }
}

pub(crate) fn icon_name(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::System => "drive-harddisk-system-symbolic",
        DeviceKind::InternalDisk => "drive-harddisk-symbolic",
        DeviceKind::Removable => "drive-removable-media-symbolic",
        DeviceKind::Optical => "media-optical-symbolic",
        DeviceKind::Network => "network-server-symbolic",
    }
}

/// One row's worth of device state. `path` is `None` only for a volume that
/// isn't mounted yet — clicking such a row mounts it first, then navigates.
pub(crate) struct DeviceEntry {
    pub label: String,
    pub icon_name: &'static str,
    pub path: Option<VeyraPath>,
    pub mount: Option<gio::Mount>,
    pub volume: Option<gio::Volume>,
    pub drive: Option<gio::Drive>,
    pub is_root: bool,
}

impl DeviceEntry {
    pub(crate) fn can_mount(&self) -> bool {
        self.mount.is_none() && self.volume.as_ref().is_some_and(|v| v.can_mount())
    }

    pub(crate) fn can_unmount(&self) -> bool {
        !self.is_root && self.mount.as_ref().is_some_and(|m| m.can_unmount())
    }

    pub(crate) fn can_eject(&self) -> bool {
        if self.is_root {
            return false;
        }
        self.mount.as_ref().is_some_and(|m| m.can_eject())
            || self.volume.as_ref().is_some_and(|v| v.can_eject())
            || self.drive.as_ref().is_some_and(|d| d.can_eject())
    }
}

/// Scans mounts + not-yet-mounted volumes into device rows, root filesystem
/// always first. Mounts already back a volume (the common case) are not
/// duplicated when that same volume is also iterated from
/// `monitor.volumes()` — only volumes with no current mount are added.
pub(crate) fn scan(monitor: &gio::VolumeMonitor) -> Vec<DeviceEntry> {
    let mut entries: Vec<DeviceEntry> = Vec::new();
    let mut root_index = None;

    for mount in monitor.mounts() {
        let path = mount_path(&mount.root());
        // Faz 21: SFTP/SMB/FTP/WebDAV mounts get their own sidebar Network
        // section (`network::scan_mounts`) instead of appearing here too.
        // Every other remote GVfs backend (MTP phones, `trash://`, ...)
        // stays in Devices, unaffected.
        if matches!(&path, VeyraPath::Uri(uri) if crate::network::detect_protocol(uri).is_some()) {
            continue;
        }
        let is_root = matches!(&path, VeyraPath::Local(p) if p.as_os_str() == "/");
        let volume = mount.volume();
        let drive = mount.drive();
        let kind = classify(
            is_root,
            is_removable(volume.as_ref(), drive.as_ref()),
            is_optical(volume.as_ref(), drive.as_ref()),
            is_network(&path),
        );
        if is_root {
            root_index = Some(entries.len());
        }
        entries.push(DeviceEntry {
            label: if is_root {
                "System".to_string()
            } else {
                mount.name().to_string()
            },
            icon_name: icon_name(kind),
            path: Some(path),
            mount: Some(mount),
            volume,
            drive,
            is_root,
        });
    }

    match root_index {
        Some(index) if index != 0 => entries.swap(0, index),
        Some(_) => {}
        None => entries.insert(
            0,
            DeviceEntry {
                label: "System".to_string(),
                icon_name: icon_name(DeviceKind::System),
                path: Some(VeyraPath::from_local("/")),
                mount: None,
                volume: None,
                drive: None,
                is_root: true,
            },
        ),
    }

    for volume in monitor.volumes() {
        if volume.get_mount().is_some() {
            continue;
        }
        let drive = volume.drive();
        let kind = classify(
            false,
            is_removable(Some(&volume), drive.as_ref()),
            is_optical(Some(&volume), drive.as_ref()),
            false,
        );
        entries.push(DeviceEntry {
            label: volume.name().to_string(),
            icon_name: icon_name(kind),
            path: None,
            mount: None,
            volume: Some(volume),
            drive,
            is_root: false,
        });
    }

    entries
}

fn mount_path(root: &gio::File) -> VeyraPath {
    match root.path() {
        Some(local) => VeyraPath::from_local(local),
        None => VeyraPath::from_uri(root.uri().to_string()),
    }
}

fn is_removable(_volume: Option<&gio::Volume>, drive: Option<&gio::Drive>) -> bool {
    drive.is_some_and(|d| d.is_removable() || d.is_media_removable())
}

fn is_optical(volume: Option<&gio::Volume>, drive: Option<&gio::Drive>) -> bool {
    drive.is_some_and(|d| d.identifier("class").as_deref() == Some("cdrom"))
        || volume.is_some_and(|v| v.identifier("class").as_deref() == Some("cdrom"))
}

fn is_network(path: &VeyraPath) -> bool {
    match path {
        VeyraPath::Local(_) => false,
        VeyraPath::Uri(uri) => !uri.starts_with("file:"),
    }
}

/// Disk usage for a single mount point, as reported by
/// `filesystem::size` / `filesystem::free` / `filesystem::used`.
pub(crate) struct UsageInfo {
    pub total: u64,
    pub used: u64,
    pub free: u64,
    pub fs_type: Option<String>,
}

/// Blocking — must run via `fs_async::run_blocking`, never on the GTK main
/// thread (Rule #11/#12). Returns `None` if the backend doesn't support
/// filesystem info (e.g. some GVfs backends) rather than erroring the row.
pub(crate) fn query_usage(path: &VeyraPath) -> Option<UsageInfo> {
    let file = path.to_gio_file();
    let info = file
        .query_filesystem_info(
            "filesystem::size,filesystem::free,filesystem::used,filesystem::type",
            gio::Cancellable::NONE,
        )
        .ok()?;
    let total = info.attribute_uint64("filesystem::size");
    let free = info.attribute_uint64("filesystem::free");
    let used = if info.has_attribute("filesystem::used") {
        info.attribute_uint64("filesystem::used")
    } else {
        total.saturating_sub(free)
    };
    let fs_type = info
        .attribute_string("filesystem::type")
        .map(|s| s.to_string());
    Some(UsageInfo {
        total,
        used,
        free,
        fs_type,
    })
}

/// Fraction of the filesystem currently used, clamped to `[0.0, 1.0]`.
/// `0.0` for a filesystem that reports zero total size rather than dividing
/// by zero.
pub(crate) fn usage_fraction(usage: &UsageInfo) -> f64 {
    if usage.total == 0 {
        return 0.0;
    }
    (usage.used as f64 / usage.total as f64).clamp(0.0, 1.0)
}

/// e.g. `"245.0 GB free of 512.0 GB (ext4, 52% used)"`, or without the
/// filesystem type when the backend didn't report one.
pub(crate) fn format_usage(usage: &UsageInfo) -> String {
    let percent = (usage_fraction(usage) * 100.0).round() as u32;
    let free = veyra_filesystem::format_size(usage.free);
    let total = veyra_filesystem::format_size(usage.total);
    match &usage.fs_type {
        Some(fs_type) => format!("{free} free of {total} ({fs_type}, {percent}% used)"),
        None => format!("{free} free of {total} ({percent}% used)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_root_is_always_system() {
        assert_eq!(classify(true, true, true, true), DeviceKind::System);
        assert_eq!(classify(true, false, false, false), DeviceKind::System);
    }

    #[test]
    fn classify_optical_beats_removable_and_network() {
        assert_eq!(classify(false, true, true, true), DeviceKind::Optical);
    }

    #[test]
    fn classify_network_beats_removable() {
        assert_eq!(classify(false, true, false, true), DeviceKind::Network);
    }

    #[test]
    fn classify_removable() {
        assert_eq!(classify(false, true, false, false), DeviceKind::Removable);
    }

    #[test]
    fn classify_default_is_internal_disk() {
        assert_eq!(
            classify(false, false, false, false),
            DeviceKind::InternalDisk
        );
    }

    #[test]
    fn icon_name_maps_every_kind() {
        assert_eq!(
            icon_name(DeviceKind::System),
            "drive-harddisk-system-symbolic"
        );
        assert_eq!(
            icon_name(DeviceKind::InternalDisk),
            "drive-harddisk-symbolic"
        );
        assert_eq!(
            icon_name(DeviceKind::Removable),
            "drive-removable-media-symbolic"
        );
        assert_eq!(icon_name(DeviceKind::Optical), "media-optical-symbolic");
        assert_eq!(icon_name(DeviceKind::Network), "network-server-symbolic");
    }

    #[test]
    fn usage_fraction_basic() {
        let usage = UsageInfo {
            total: 1000,
            used: 250,
            free: 750,
            fs_type: None,
        };
        assert!((usage_fraction(&usage) - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn usage_fraction_zero_total_is_safe() {
        let usage = UsageInfo {
            total: 0,
            used: 0,
            free: 0,
            fs_type: None,
        };
        assert_eq!(usage_fraction(&usage), 0.0);
    }

    #[test]
    fn usage_fraction_clamps_above_one() {
        // Some backends report `used` slightly above `total` due to
        // reserved blocks; the fraction must still stay a valid ratio.
        let usage = UsageInfo {
            total: 100,
            used: 150,
            free: 0,
            fs_type: None,
        };
        assert_eq!(usage_fraction(&usage), 1.0);
    }

    #[test]
    fn format_usage_reports_free_total_and_percent() {
        let usage = UsageInfo {
            total: 1024 * 1024 * 1024,
            used: 512 * 1024 * 1024,
            free: 512 * 1024 * 1024,
            fs_type: Some("ext4".to_string()),
        };
        assert_eq!(
            format_usage(&usage),
            "512.0 MB free of 1.0 GB (ext4, 50% used)"
        );
    }

    #[test]
    fn format_usage_omits_parens_type_when_unknown() {
        let usage = UsageInfo {
            total: 1024 * 1024 * 1024,
            used: 512 * 1024 * 1024,
            free: 512 * 1024 * 1024,
            fs_type: None,
        };
        assert_eq!(format_usage(&usage), "512.0 MB free of 1.0 GB (50% used)");
    }
}
