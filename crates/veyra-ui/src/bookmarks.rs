//! Faz 16: XDG bookmarks (`~/.config/gtk-3.0/bookmarks`) — the same file
//! GTK3/GTK4 file choosers and Nautilus read and write, so Veyra's
//! Bookmarks section stays in sync with the rest of the desktop.
//!
//! Format is one bookmark per line: `<uri>[ <label>]`, e.g.
//! `file:///home/user/Projects My Projects`. Writes are atomic
//! (`.tmp` + rename) so a crash or concurrent writer never leaves a
//! truncated/corrupt file behind (Rule #16/#17).
//!
//! Every mutating function takes an explicit file path (`*_at`) with a
//! public wrapper defaulting to the real XDG location — this lets tests
//! exercise the parsing/serialization/mutation logic against a temp file
//! instead of the user's real bookmarks.

use std::io;
use std::path::{Path, PathBuf};

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;

use veyra_filesystem::VeyraPath;

/// A single bookmarked location: the raw URI as stored on disk, an optional
/// user-supplied display label, and the resolved `VeyraPath` used to
/// navigate to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Bookmark {
    pub uri: String,
    pub label: Option<String>,
    pub path: VeyraPath,
}

impl Bookmark {
    /// The label to show in the sidebar: the custom label if set, otherwise
    /// the location's final path segment, otherwise the raw URI.
    pub(crate) fn display_label(&self) -> String {
        self.label
            .clone()
            .or_else(|| self.path.file_name())
            .unwrap_or_else(|| self.uri.clone())
    }
}

/// The real, XDG-standard bookmarks file path: `~/.config/gtk-3.0/bookmarks`.
pub(crate) fn bookmarks_path() -> PathBuf {
    glib::user_config_dir().join("gtk-3.0").join("bookmarks")
}

fn parse_line(line: &str) -> Option<Bookmark> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let mut parts = line.splitn(2, ' ');
    let uri = parts.next()?.to_string();
    let label = parts
        .next()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let path = VeyraPath::from_gio_file(&gio::File::for_uri(&uri));
    Some(Bookmark { uri, label, path })
}

fn serialize_line(bookmark: &Bookmark) -> String {
    match &bookmark.label {
        Some(label) => format!("{} {}", bookmark.uri, label),
        None => bookmark.uri.clone(),
    }
}

/// Loads every bookmark from `path`. A missing file (never bookmarked
/// anything yet) is treated as an empty list, not an error.
fn load_from(path: &Path) -> Vec<Bookmark> {
    match std::fs::read_to_string(path) {
        Ok(content) => content.lines().filter_map(parse_line).collect(),
        Err(_) => Vec::new(),
    }
}

/// Writes `bookmarks` to `path` atomically (write to `<path>.tmp`, then
/// rename over `path`) so a crash mid-write never corrupts the file.
fn save_to(path: &Path, bookmarks: &[Bookmark]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut content: String = bookmarks
        .iter()
        .map(serialize_line)
        .collect::<Vec<_>>()
        .join("\n");
    if !bookmarks.is_empty() {
        content.push('\n');
    }
    let tmp_path = path.with_extension("tmp");
    veyra_core::security::write_atomic_private(&tmp_path, path, content.as_bytes())
}

fn add_at(path: &Path, target: &VeyraPath, label: Option<String>) -> io::Result<()> {
    let uri = target.to_gio_file().uri().to_string();
    let mut bookmarks = load_from(path);
    if bookmarks.iter().any(|b| b.uri == uri) {
        return Ok(());
    }
    bookmarks.push(Bookmark {
        uri,
        label,
        path: target.clone(),
    });
    save_to(path, &bookmarks)
}

fn remove_at(path: &Path, uri: &str) -> io::Result<()> {
    let mut bookmarks = load_from(path);
    bookmarks.retain(|b| b.uri != uri);
    save_to(path, &bookmarks)
}

fn rename_at(path: &Path, uri: &str, new_label: &str) -> io::Result<()> {
    let mut bookmarks = load_from(path);
    let Some(bookmark) = bookmarks.iter_mut().find(|b| b.uri == uri) else {
        return Ok(());
    };
    let trimmed = new_label.trim();
    bookmark.label = if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    };
    save_to(path, &bookmarks)
}

/// Loads every bookmark from the real XDG bookmarks file.
pub(crate) fn load() -> Vec<Bookmark> {
    load_from(&bookmarks_path())
}

/// Appends `target` to the real bookmarks file with an optional custom
/// `label`. A no-op (not an error) if `target` is already bookmarked.
pub(crate) fn add(target: &VeyraPath, label: Option<String>) -> io::Result<()> {
    add_at(&bookmarks_path(), target, label)
}

/// Removes the bookmark matching `uri` from the real bookmarks file.
/// A no-op if no bookmark matches.
pub(crate) fn remove(uri: &str) -> io::Result<()> {
    remove_at(&bookmarks_path(), uri)
}

/// Sets the custom label of the bookmark matching `uri` in the real
/// bookmarks file. An empty/whitespace-only `new_label` clears the custom
/// label (falling back to the location's own name). A no-op if no bookmark
/// matches.
pub(crate) fn rename(uri: &str, new_label: &str) -> io::Result<()> {
    rename_at(&bookmarks_path(), uri, new_label)
}

/// Watches the real bookmarks file for changes (made by Veyra itself or any
/// other application) and invokes `on_change` whenever it does. Returns
/// `None` (logging a warning) instead of panicking if the monitor can't be
/// created (Rule #15/#18) — the sidebar still shows the bookmarks loaded at
/// startup, it just won't live-refresh.
pub(crate) fn watch(on_change: impl Fn() + 'static) -> Option<gio::FileMonitor> {
    let file = gio::File::for_path(bookmarks_path());
    match file.monitor_file(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE) {
        Ok(monitor) => {
            monitor.connect_changed(move |_, _, _, _event| on_change());
            Some(monitor)
        }
        Err(err) => {
            tracing::warn!(error = %err, "failed to watch bookmarks file for changes");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_bookmarks_path(test_name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "veyra-bookmarks-test-{}-{test_name}",
            std::process::id()
        ))
    }

    #[test]
    fn parses_line_without_label() {
        let bookmark = parse_line("file:///home/user/Projects").unwrap();
        assert_eq!(bookmark.uri, "file:///home/user/Projects");
        assert_eq!(bookmark.label, None);
        assert_eq!(bookmark.display_label(), "Projects");
    }

    #[test]
    fn parses_line_with_label() {
        let bookmark = parse_line("file:///home/user/Projects My Projects").unwrap();
        assert_eq!(bookmark.uri, "file:///home/user/Projects");
        assert_eq!(bookmark.label.as_deref(), Some("My Projects"));
        assert_eq!(bookmark.display_label(), "My Projects");
    }

    #[test]
    fn parse_line_skips_blank_lines() {
        assert!(parse_line("").is_none());
        assert!(parse_line("   ").is_none());
    }

    #[test]
    fn serialize_round_trips_with_and_without_label() {
        let with_label = Bookmark {
            uri: "file:///a".to_string(),
            label: Some("A Folder".to_string()),
            path: VeyraPath::from_uri("file:///a"),
        };
        assert_eq!(serialize_line(&with_label), "file:///a A Folder");

        let without_label = Bookmark {
            uri: "file:///b".to_string(),
            label: None,
            path: VeyraPath::from_uri("file:///b"),
        };
        assert_eq!(serialize_line(&without_label), "file:///b");
    }

    #[test]
    fn add_then_load_round_trips() {
        let path = temp_bookmarks_path("add_then_load_round_trips");
        std::fs::remove_file(&path).ok();

        add_at(
            &path,
            &VeyraPath::from_local("/home/user/Projects"),
            Some("My Projects".to_string()),
        )
        .unwrap();

        let loaded = load_from(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].display_label(), "My Projects");
        assert_eq!(loaded[0].path, VeyraPath::from_local("/home/user/Projects"));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn add_is_idempotent_for_same_uri() {
        let path = temp_bookmarks_path("add_is_idempotent_for_same_uri");
        std::fs::remove_file(&path).ok();

        let target = VeyraPath::from_local("/home/user/Documents");
        add_at(&path, &target, None).unwrap();
        add_at(&path, &target, Some("Duplicate?".to_string())).unwrap();

        assert_eq!(load_from(&path).len(), 1);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn remove_deletes_matching_bookmark() {
        let path = temp_bookmarks_path("remove_deletes_matching_bookmark");
        std::fs::remove_file(&path).ok();

        add_at(&path, &VeyraPath::from_local("/home/user/A"), None).unwrap();
        add_at(&path, &VeyraPath::from_local("/home/user/B"), None).unwrap();

        let uri = gio::File::for_path("/home/user/A").uri().to_string();
        remove_at(&path, &uri).unwrap();

        let loaded = load_from(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].path, VeyraPath::from_local("/home/user/B"));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn rename_sets_custom_label() {
        let path = temp_bookmarks_path("rename_sets_custom_label");
        std::fs::remove_file(&path).ok();

        add_at(&path, &VeyraPath::from_local("/home/user/A"), None).unwrap();
        let uri = gio::File::for_path("/home/user/A").uri().to_string();
        rename_at(&path, &uri, "Renamed").unwrap();

        let loaded = load_from(&path);
        assert_eq!(loaded[0].label.as_deref(), Some("Renamed"));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn rename_with_blank_label_clears_custom_label() {
        let path = temp_bookmarks_path("rename_with_blank_label_clears_custom_label");
        std::fs::remove_file(&path).ok();

        add_at(
            &path,
            &VeyraPath::from_local("/home/user/A"),
            Some("Custom".to_string()),
        )
        .unwrap();
        let uri = gio::File::for_path("/home/user/A").uri().to_string();
        rename_at(&path, &uri, "   ").unwrap();

        let loaded = load_from(&path);
        assert_eq!(loaded[0].label, None);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn load_from_missing_file_is_empty() {
        let path = temp_bookmarks_path("load_from_missing_file_is_empty");
        std::fs::remove_file(&path).ok();
        assert!(load_from(&path).is_empty());
    }
}
