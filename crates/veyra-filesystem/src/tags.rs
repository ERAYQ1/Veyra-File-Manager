//! Faz 62: Smart Color Tags — assigning one of six standard colors to any
//! file or folder, macOS-Finder-style. Persisted as `~/.config/veyra/
//! tags.json`, a flat URI → color map, written atomically (`.tmp` + rename,
//! `veyra_core::security::write_atomic_private`) so a crash mid-save never
//! corrupts the file (Rule #16/#17) — same pattern as `bookmarks.rs`/
//! `config.rs`/`shortcuts.rs` in `veyra-ui`.
//!
//! Keyed by URI (`VeyraPath::to_gio_file().uri()`) rather than a local path
//! so a tag survives round-tripping through any GVfs-backed location the
//! same way bookmarks do, not just plain local files.
//!
//! Every mutating function takes an explicit file path (`*_at`) with a
//! public wrapper defaulting to the real XDG-style config location — lets
//! tests exercise the read/write/CRUD logic against a temp file instead of
//! the user's real `tags.json`.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use gio::prelude::FileExt;

use crate::path::VeyraPath;

/// One of the six standard tag colors, each with a fixed hex swatch — no
/// user-defined colors (Kural: a small, predictable palette reads faster at
/// a glance than an unbounded one, matching macOS Finder's own six).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TagColor {
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
}

impl TagColor {
    pub const ALL: [TagColor; 6] = [
        TagColor::Red,
        TagColor::Orange,
        TagColor::Yellow,
        TagColor::Green,
        TagColor::Blue,
        TagColor::Purple,
    ];

    /// The color's fixed hex swatch, used for both the view badges and the
    /// sidebar's color dots.
    pub fn hex(self) -> &'static str {
        match self {
            TagColor::Red => "#e01b24",
            TagColor::Orange => "#ff7800",
            TagColor::Yellow => "#f6d32d",
            TagColor::Green => "#26a269",
            TagColor::Blue => "#3584e4",
            TagColor::Purple => "#9141ac",
        }
    }

    /// A stable, lowercase ASCII name — used as the CSS class suffix
    /// (`veyra-tag-pill-red`) and the `win.set-tag-selected` action's string
    /// parameter, so it never needs to change across locales.
    pub fn slug(self) -> &'static str {
        match self {
            TagColor::Red => "red",
            TagColor::Orange => "orange",
            TagColor::Yellow => "yellow",
            TagColor::Green => "green",
            TagColor::Blue => "blue",
            TagColor::Purple => "purple",
        }
    }

    pub fn from_slug(slug: &str) -> Option<TagColor> {
        TagColor::ALL.into_iter().find(|c| c.slug() == slug)
    }
}

/// The real, XDG-standard tags file path: `~/.config/veyra/tags.json`.
pub fn tags_path() -> PathBuf {
    glib::user_config_dir().join("veyra").join("tags.json")
}

/// Loads every tag from `path` as a URI → color map. A missing or corrupt
/// file is treated as an empty store, never an error (Rule #4/#48 — a bad
/// or not-yet-created tags file must never crash the app).
fn load_from(path: &Path) -> HashMap<String, TagColor> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    serde_json::from_str(&content).unwrap_or_default()
}

/// Writes `tags` to `path` atomically.
fn save_to(path: &Path, tags: &HashMap<String, TagColor>) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(tags).unwrap_or_else(|_| "{}".to_string());
    let tmp_path = path.with_extension("tmp");
    veyra_core::security::write_atomic_private(&tmp_path, path, json.as_bytes())
}

fn set_tag_at(path: &Path, target: &VeyraPath, color: TagColor) -> io::Result<()> {
    let uri = target.to_gio_file().uri().to_string();
    let mut tags = load_from(path);
    tags.insert(uri, color);
    save_to(path, &tags)
}

fn remove_tag_at(path: &Path, target: &VeyraPath) -> io::Result<()> {
    let uri = target.to_gio_file().uri().to_string();
    let mut tags = load_from(path);
    if tags.remove(&uri).is_none() {
        return Ok(());
    }
    save_to(path, &tags)
}

fn get_tag_at(path: &Path, target: &VeyraPath) -> Option<TagColor> {
    let uri = target.to_gio_file().uri().to_string();
    load_from(path).get(&uri).copied()
}

fn get_paths_by_tag_at(path: &Path, color: TagColor) -> Vec<VeyraPath> {
    load_from(path)
        .into_iter()
        .filter(|(_, c)| *c == color)
        .map(|(uri, _)| VeyraPath::from_uri(uri))
        .collect()
}

fn list_all_tagged_at(path: &Path) -> Vec<(VeyraPath, TagColor)> {
    load_from(path)
        .into_iter()
        .map(|(uri, color)| (VeyraPath::from_uri(uri), color))
        .collect()
}

/// Assigns `color` to `target` in the real tags file, overwriting any
/// existing tag it already had.
pub fn set_tag(target: &VeyraPath, color: TagColor) -> io::Result<()> {
    set_tag_at(&tags_path(), target, color)
}

/// Removes whatever tag `target` has in the real tags file. A no-op (not an
/// error) if it isn't tagged.
pub fn remove_tag(target: &VeyraPath) -> io::Result<()> {
    remove_tag_at(&tags_path(), target)
}

/// The tag currently assigned to `target` in the real tags file, if any.
pub fn get_tag(target: &VeyraPath) -> Option<TagColor> {
    get_tag_at(&tags_path(), target)
}

/// Every path tagged `color` in the real tags file.
pub fn get_paths_by_tag(color: TagColor) -> Vec<VeyraPath> {
    get_paths_by_tag_at(&tags_path(), color)
}

/// Every tagged path in the real tags file, paired with its color.
pub fn list_all_tagged() -> Vec<(VeyraPath, TagColor)> {
    list_all_tagged_at(&tags_path())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_tags_path(test_name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "veyra-tags-test-{}-{test_name}.json",
            std::process::id()
        ))
    }

    #[test]
    fn set_then_get_round_trips() {
        let path = temp_tags_path("set_then_get_round_trips");
        std::fs::remove_file(&path).ok();

        let target = VeyraPath::from_local("/home/user/Projects");
        set_tag_at(&path, &target, TagColor::Blue).unwrap();

        assert_eq!(get_tag_at(&path, &target), Some(TagColor::Blue));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn set_tag_overwrites_existing_color() {
        let path = temp_tags_path("set_tag_overwrites_existing_color");
        std::fs::remove_file(&path).ok();

        let target = VeyraPath::from_local("/home/user/A");
        set_tag_at(&path, &target, TagColor::Red).unwrap();
        set_tag_at(&path, &target, TagColor::Green).unwrap();

        assert_eq!(get_tag_at(&path, &target), Some(TagColor::Green));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn remove_tag_deletes_the_entry() {
        let path = temp_tags_path("remove_tag_deletes_the_entry");
        std::fs::remove_file(&path).ok();

        let target = VeyraPath::from_local("/home/user/B");
        set_tag_at(&path, &target, TagColor::Yellow).unwrap();
        remove_tag_at(&path, &target).unwrap();

        assert_eq!(get_tag_at(&path, &target), None);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn remove_tag_on_untagged_path_is_a_no_op() {
        let path = temp_tags_path("remove_tag_on_untagged_path_is_a_no_op");
        std::fs::remove_file(&path).ok();

        let target = VeyraPath::from_local("/home/user/Untagged");
        assert!(remove_tag_at(&path, &target).is_ok());
        assert_eq!(get_tag_at(&path, &target), None);
    }

    #[test]
    fn get_tag_on_missing_file_is_none() {
        let path = temp_tags_path("get_tag_on_missing_file_is_none");
        std::fs::remove_file(&path).ok();

        assert_eq!(
            get_tag_at(&path, &VeyraPath::from_local("/home/user/Nope")),
            None
        );
    }

    #[test]
    fn get_paths_by_tag_returns_only_matching_color() {
        let path = temp_tags_path("get_paths_by_tag_returns_only_matching_color");
        std::fs::remove_file(&path).ok();

        let red_path = VeyraPath::from_local("/home/user/Red1");
        let also_red_path = VeyraPath::from_local("/home/user/Red2");
        let blue_path = VeyraPath::from_local("/home/user/Blue1");
        set_tag_at(&path, &red_path, TagColor::Red).unwrap();
        set_tag_at(&path, &also_red_path, TagColor::Red).unwrap();
        set_tag_at(&path, &blue_path, TagColor::Blue).unwrap();

        let reds: Vec<String> = get_paths_by_tag_at(&path, TagColor::Red)
            .iter()
            .map(|p| p.to_gio_file().uri().to_string())
            .collect();
        assert_eq!(reds.len(), 2);
        assert!(reds.contains(&red_path.to_gio_file().uri().to_string()));
        assert!(reds.contains(&also_red_path.to_gio_file().uri().to_string()));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn list_all_tagged_returns_every_entry() {
        let path = temp_tags_path("list_all_tagged_returns_every_entry");
        std::fs::remove_file(&path).ok();

        set_tag_at(&path, &VeyraPath::from_local("/home/user/A"), TagColor::Red).unwrap();
        set_tag_at(
            &path,
            &VeyraPath::from_local("/home/user/B"),
            TagColor::Green,
        )
        .unwrap();

        let all = list_all_tagged_at(&path);
        assert_eq!(all.len(), 2);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn load_from_missing_file_is_empty() {
        let path = temp_tags_path("load_from_missing_file_is_empty");
        std::fs::remove_file(&path).ok();
        assert!(load_from(&path).is_empty());
    }

    #[test]
    fn load_from_corrupt_json_falls_back_to_empty() {
        let path = temp_tags_path("load_from_corrupt_json_falls_back_to_empty");
        std::fs::write(&path, b"{ not valid json").unwrap();

        assert!(load_from(&path).is_empty());

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn slug_round_trips_through_from_slug() {
        for color in TagColor::ALL {
            assert_eq!(TagColor::from_slug(color.slug()), Some(color));
        }
    }

    #[test]
    fn from_slug_rejects_unknown_names() {
        assert_eq!(TagColor::from_slug("magenta"), None);
    }

    #[test]
    fn every_color_has_a_distinct_hex_value() {
        let hexes: Vec<&str> = TagColor::ALL.iter().map(|c| c.hex()).collect();
        for (i, hex) in hexes.iter().enumerate() {
            assert!(!hexes[..i].contains(hex), "duplicate hex value {hex}");
        }
    }
}
