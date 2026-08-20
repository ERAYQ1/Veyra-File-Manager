//! Faz 21: remote GVfs locations — SFTP/SSH, SMB, FTP, and WebDAV
//! (`dav://`/`davs://`). URI parsing/normalization here is pure and
//! directly unit-testable, kept separate from `veyra_filesystem` (Rule
//! #44 — decouple network from the core filesystem crate) since it's UI
//! policy (which schemes the "Connect to Server" dialog offers), not a
//! filesystem primitive.
//!
//! Mounting goes through `gtk4::MountOperation`, whose built-in password
//! dialog satisfies GVfs's `connect_ask_password` without blocking the GTK
//! main thread (Rule #11/#12) or this module ever seeing a typed password
//! (Rule #23) — GTK owns the credentials dialog and hands the result
//! straight to GVfs.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

use veyra_filesystem::VeyraPath;

use crate::devices::DeviceEntry;

/// The virtual location the sidebar's "Network" entry navigates to.
pub(crate) const NETWORK_URI: &str = "network:///";

/// The remote filesystem families the "Connect to Server" dialog offers a
/// quick shortcut for. `Sftp` also matches the `ssh://` alias GVfs accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NetworkProtocol {
    Sftp,
    Smb,
    Ftp,
    Dav,
    Davs,
}

impl NetworkProtocol {
    pub(crate) const ALL: [NetworkProtocol; 5] = [
        NetworkProtocol::Sftp,
        NetworkProtocol::Smb,
        NetworkProtocol::Ftp,
        NetworkProtocol::Dav,
        NetworkProtocol::Davs,
    ];

    pub(crate) fn scheme(self) -> &'static str {
        match self {
            NetworkProtocol::Sftp => "sftp",
            NetworkProtocol::Smb => "smb",
            NetworkProtocol::Ftp => "ftp",
            NetworkProtocol::Dav => "dav",
            NetworkProtocol::Davs => "davs",
        }
    }

    /// Short label for the dialog's protocol quick-picker chips.
    pub(crate) fn chip_label(self) -> &'static str {
        match self {
            NetworkProtocol::Sftp => "SFTP",
            NetworkProtocol::Smb => "SMB",
            NetworkProtocol::Ftp => "FTP",
            NetworkProtocol::Dav => "WebDAV",
            NetworkProtocol::Davs => "WebDAV (TLS)",
        }
    }

    /// Icon shown in the sidebar's Network section and the recent-servers
    /// list.
    pub(crate) fn icon_name(self) -> &'static str {
        match self {
            NetworkProtocol::Smb => "network-workgroup-symbolic",
            _ => "network-server-symbolic",
        }
    }

    fn from_scheme(scheme: &str) -> Option<Self> {
        match scheme {
            "sftp" | "ssh" => Some(NetworkProtocol::Sftp),
            "smb" => Some(NetworkProtocol::Smb),
            "ftp" | "ftps" => Some(NetworkProtocol::Ftp),
            "dav" => Some(NetworkProtocol::Dav),
            "davs" => Some(NetworkProtocol::Davs),
            _ => None,
        }
    }
}

/// Detects the protocol of an already-schemed URI (`sftp://host/path`), or
/// `None` for a local path or a scheme this dialog doesn't offer (e.g. a
/// `mtp://` phone or a `trash://` mount, which stay in the Devices section).
pub(crate) fn detect_protocol(uri: &str) -> Option<NetworkProtocol> {
    let scheme = uri.split_once("://")?.0;
    NetworkProtocol::from_scheme(&scheme.to_ascii_lowercase())
}

/// A server address the user typed, parsed and normalized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedServerUri {
    pub uri: String,
    pub protocol: NetworkProtocol,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NetworkUriError {
    Empty,
    UnsupportedScheme(String),
    MissingHost,
}

impl fmt::Display for NetworkUriError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetworkUriError::Empty => write!(f, "Enter a server address"),
            NetworkUriError::UnsupportedScheme(scheme) => {
                write!(f, "Unsupported protocol \"{scheme}\"")
            }
            NetworkUriError::MissingHost => write!(f, "Enter a host name or address"),
        }
    }
}

/// Parses and normalizes a user-typed server address. `input` may already
/// carry a scheme (`sftp://user@host:2222/path`) or be bare
/// (`nas.local/media`), in which case `default_protocol` supplies the
/// scheme. Never touches the network — purely string validation, so it's
/// directly unit-testable.
pub(crate) fn parse_server_address(
    input: &str,
    default_protocol: NetworkProtocol,
) -> Result<ParsedServerUri, NetworkUriError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(NetworkUriError::Empty);
    }

    let (scheme, rest, protocol) = match trimmed.split_once("://") {
        Some((scheme, rest)) => {
            let protocol = NetworkProtocol::from_scheme(&scheme.to_ascii_lowercase())
                .ok_or_else(|| NetworkUriError::UnsupportedScheme(scheme.to_string()))?;
            (protocol.scheme(), rest, protocol)
        }
        None => (default_protocol.scheme(), trimmed, default_protocol),
    };

    let authority_end = rest.find('/').unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let path = &rest[authority_end..];

    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = host.split(':').next().unwrap_or(host);
    if host.is_empty() {
        return Err(NetworkUriError::MissingHost);
    }

    Ok(ParsedServerUri {
        uri: format!("{scheme}://{authority}{path}"),
        protocol,
    })
}

const MAX_HISTORY: usize = 10;

/// The real, XDG-standard location for the "Connect to Server" recent
/// address history: `~/.config/veyra/network_history`.
fn history_path() -> PathBuf {
    glib::user_config_dir()
        .join("veyra")
        .join("network_history")
}

fn load_history_from(path: &Path) -> Vec<String> {
    match std::fs::read_to_string(path) {
        Ok(content) => content
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn save_history_to(path: &Path, entries: &[String]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut content = entries.join("\n");
    if !entries.is_empty() {
        content.push('\n');
    }
    let tmp_path = path.with_extension("tmp");
    veyra_core::security::write_atomic_private(&tmp_path, path, content.as_bytes())
}

fn record_history_at(path: &Path, uri: &str) -> io::Result<()> {
    let mut entries = load_history_from(path);
    entries.retain(|entry| entry != uri);
    entries.insert(0, uri.to_string());
    entries.truncate(MAX_HISTORY);
    save_history_to(path, &entries)
}

/// Loads the recent-servers list from the real history file, most recently
/// connected first.
pub(crate) fn load_history() -> Vec<String> {
    load_history_from(&history_path())
}

/// Records a successful connection to `uri` at the front of the real
/// history file, moving it there if already present and capping the list
/// at `MAX_HISTORY` entries.
pub(crate) fn record_history(uri: &str) {
    if let Err(err) = record_history_at(&history_path(), uri) {
        tracing::warn!(error = %err, "failed to persist network server history");
    }
}

/// Clears the entire recent-servers history (Faz 34 Privacy page's "Clear
/// Recent Servers" button) — same atomic-write emptying `save_history_to`
/// already does for a single removal, just with an empty list.
pub(crate) fn clear_history() {
    if let Err(err) = save_history_to(&history_path(), &[]) {
        tracing::warn!(error = %err, "failed to clear network server history");
    }
}

/// Removes `uri` from the real history file, if present.
pub(crate) fn remove_history_entry(uri: &str) {
    let path = history_path();
    let mut entries = load_history_from(&path);
    let before = entries.len();
    entries.retain(|entry| entry != uri);
    if entries.len() != before {
        if let Err(err) = save_history_to(&path, &entries) {
            tracing::warn!(error = %err, "failed to update network server history");
        }
    }
}

/// Mounts `uri` (an already-normalized `scheme://...` GVfs location),
/// prompting for credentials via GTK's native `GtkMountOperation` dialog
/// whenever the backend asks. `on_success` receives the resolved
/// `VeyraPath`; `on_error` receives an already user-facing message (Rule
/// #15/#18 — never a raw GLib error string).
pub(crate) fn mount_remote_location(
    window: &adw::ApplicationWindow,
    uri: String,
    on_success: impl Fn(VeyraPath) + 'static,
    on_error: impl Fn(String) + 'static,
) {
    let file = gio::File::for_uri(&uri);
    let operation = gtk4::MountOperation::new(Some(window));
    glib::spawn_future_local(async move {
        match file
            .mount_enclosing_volume_future(gio::MountMountFlags::NONE, Some(&operation))
            .await
        {
            Ok(()) => {
                record_history(&uri);
                on_success(VeyraPath::from_gio_file(&file));
            }
            Err(err) if err.matches(gio::IOErrorEnum::AlreadyMounted) => {
                record_history(&uri);
                on_success(VeyraPath::from_gio_file(&file));
            }
            Err(err) if err.matches(gio::IOErrorEnum::Cancelled) => {
                on_error("Connection cancelled".to_string());
            }
            Err(err) => {
                tracing::warn!(error = %err, "failed to mount network location");
                on_error(friendly_mount_error(&err));
            }
        }
    });
}

fn friendly_mount_error(err: &glib::Error) -> String {
    if err.matches(gio::IOErrorEnum::HostNotFound) {
        "Host not found — check the server address".to_string()
    } else if err.matches(gio::IOErrorEnum::TimedOut) {
        "Connection timed out".to_string()
    } else if err.matches(gio::IOErrorEnum::ConnectionRefused) {
        "Connection refused by the server".to_string()
    } else if err.matches(gio::IOErrorEnum::PermissionDenied) {
        "Authentication failed".to_string()
    } else {
        "Unable to connect to server".to_string()
    }
}

/// Scans `monitor` for active mounts backed by one of the protocols this
/// module recognizes (SFTP/SSH, SMB, FTP, WebDAV), building rows reusing
/// `devices::DeviceEntry` — a network row needs exactly the same
/// affordances (navigate, open in new tab, unmount, properties) a Devices
/// row already has. Every other GVfs mount (`mtp://`, `trash://`, ...)
/// stays out of this list and continues to show up under Devices, since
/// `devices::scan` only excludes the protocols recognized here.
pub(crate) fn scan_mounts(monitor: &gio::VolumeMonitor) -> Vec<DeviceEntry> {
    monitor
        .mounts()
        .into_iter()
        .filter_map(|mount| {
            let path = VeyraPath::from_gio_file(&mount.root());
            let VeyraPath::Uri(uri) = &path else {
                return None;
            };
            let protocol = detect_protocol(uri)?;
            Some(DeviceEntry {
                label: mount.name().to_string(),
                icon_name: protocol.icon_name(),
                path: Some(path.clone()),
                mount: Some(mount),
                volume: None,
                drive: None,
                is_root: false,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_known_protocols() {
        assert_eq!(
            detect_protocol("sftp://host/path"),
            Some(NetworkProtocol::Sftp)
        );
        assert_eq!(detect_protocol("ssh://host"), Some(NetworkProtocol::Sftp));
        assert_eq!(
            detect_protocol("smb://host/share"),
            Some(NetworkProtocol::Smb)
        );
        assert_eq!(detect_protocol("ftp://host"), Some(NetworkProtocol::Ftp));
        assert_eq!(detect_protocol("dav://host/x"), Some(NetworkProtocol::Dav));
        assert_eq!(
            detect_protocol("davs://host/x"),
            Some(NetworkProtocol::Davs)
        );
    }

    #[test]
    fn detect_protocol_rejects_unknown_or_local() {
        assert_eq!(detect_protocol("mtp://phone/dcim"), None);
        assert_eq!(detect_protocol("/home/user"), None);
        assert_eq!(detect_protocol("not-a-uri"), None);
    }

    #[test]
    fn parses_full_uri_with_user_and_port() {
        let parsed = parse_server_address(
            "sftp://user@backup.local:2222/var/www",
            NetworkProtocol::Smb,
        )
        .unwrap();
        assert_eq!(parsed.protocol, NetworkProtocol::Sftp);
        assert_eq!(parsed.uri, "sftp://user@backup.local:2222/var/www");
    }

    #[test]
    fn bare_host_uses_default_protocol_and_gains_scheme() {
        let parsed = parse_server_address("nas.local/media", NetworkProtocol::Smb).unwrap();
        assert_eq!(parsed.protocol, NetworkProtocol::Smb);
        assert_eq!(parsed.uri, "smb://nas.local/media");
    }

    #[test]
    fn bare_host_without_path_mounts_the_root() {
        let parsed = parse_server_address("host", NetworkProtocol::Ftp).unwrap();
        assert_eq!(parsed.uri, "ftp://host");
    }

    #[test]
    fn empty_address_is_rejected() {
        assert_eq!(
            parse_server_address("   ", NetworkProtocol::Sftp),
            Err(NetworkUriError::Empty)
        );
    }

    #[test]
    fn unsupported_scheme_is_rejected() {
        assert_eq!(
            parse_server_address("http://host/x", NetworkProtocol::Sftp),
            Err(NetworkUriError::UnsupportedScheme("http".to_string()))
        );
    }

    #[test]
    fn missing_host_is_rejected() {
        assert_eq!(
            parse_server_address("sftp://", NetworkProtocol::Sftp),
            Err(NetworkUriError::MissingHost)
        );
        assert_eq!(
            parse_server_address("sftp:///path", NetworkProtocol::Sftp),
            Err(NetworkUriError::MissingHost)
        );
    }

    #[test]
    fn scheme_is_case_insensitive() {
        let parsed = parse_server_address("SFTP://Host/Path", NetworkProtocol::Ftp).unwrap();
        assert_eq!(parsed.protocol, NetworkProtocol::Sftp);
        assert_eq!(parsed.uri, "sftp://Host/Path");
    }

    #[test]
    fn history_round_trips_and_dedupes_to_front() {
        let path =
            std::env::temp_dir().join(format!("veyra-network-history-test-{}", std::process::id()));
        std::fs::remove_file(&path).ok();

        record_history_at(&path, "sftp://a/x").unwrap();
        record_history_at(&path, "smb://b/y").unwrap();
        record_history_at(&path, "sftp://a/x").unwrap();

        let loaded = load_history_from(&path);
        assert_eq!(loaded, vec!["sftp://a/x", "smb://b/y"]);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn history_caps_at_max_entries() {
        let path = std::env::temp_dir().join(format!(
            "veyra-network-history-cap-test-{}",
            std::process::id()
        ));
        std::fs::remove_file(&path).ok();

        for i in 0..(MAX_HISTORY + 5) {
            record_history_at(&path, &format!("sftp://host{i}/x")).unwrap();
        }
        assert_eq!(load_history_from(&path).len(), MAX_HISTORY);

        std::fs::remove_file(&path).ok();
    }
}
