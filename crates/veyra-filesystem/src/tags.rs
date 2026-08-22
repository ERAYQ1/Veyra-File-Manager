//! Faz 62/63: Smart Color Tags — assigning one of six standard colors to any
//! file or folder, macOS-Finder-style, plus (Faz 63) letting the user
//! rename each color to whatever it means to them ("Red" -> "Urgent").
//! Persisted as `~/.config/veyra/tags.json`, written atomically (`.tmp` +
//! rename, `veyra_core::security::write_atomic_private`) so a crash
//! mid-save never corrupts the file (Rule #16/#17) — same pattern as
//! `bookmarks.rs`/`config.rs`/`shortcuts.rs` in `veyra-ui`.
//!
//! `TagsFile` holds two independent maps in the one file: `tags` (path
//! assignments, keyed by URI so a tag survives round-tripping through any
//! GVfs-backed location, not just plain local files) and `custom_names`
//! (per-color display-name overrides, keyed by `TagColor::slug()` rather
//! than relying on serde's enum-as-map-key support, so the on-disk shape
//! stays a plain, predictable `{slug: name}` object). They're independent
//! on purpose: clearing every tag assignment (`clear_all_tags`) must not
//! also discard a user's custom color names, and resetting names
//! (`reset_custom_tag_names`) must not touch which files are tagged.
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

/// The on-disk shape of `tags.json` — see the module doc comment for why
/// `tags` and `custom_names` are independent maps.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
struct TagsFile {
    #[serde(default)]
    tags: HashMap<String, TagColor>,
    #[serde(default)]
    custom_names: HashMap<String, String>,
}

/// Loads `path`'s tags file. A missing or corrupt file is treated as an
/// empty store, never an error (Rule #4/#48 — a bad or not-yet-created
/// tags file must never crash the app).
fn load_from(path: &Path) -> TagsFile {
    let Ok(content) = std::fs::read_to_string(path) else {
        return TagsFile::default();
    };
    serde_json::from_str(&content).unwrap_or_default()
}

/// Writes `file` to `path` atomically.
fn save_to(path: &Path, file: &TagsFile) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(file).unwrap_or_else(|_| "{}".to_string());
    let tmp_path = path.with_extension("tmp");
    veyra_core::security::write_atomic_private(&tmp_path, path, json.as_bytes())
}

fn set_tag_at(path: &Path, target: &VeyraPath, color: TagColor) -> io::Result<()> {
    let uri = target.to_gio_file().uri().to_string();
    let mut file = load_from(path);
    file.tags.insert(uri, color);
    save_to(path, &file)
}

fn remove_tag_at(path: &Path, target: &VeyraPath) -> io::Result<()> {
    let uri = target.to_gio_file().uri().to_string();
    let mut file = load_from(path);
    if file.tags.remove(&uri).is_none() {
        return Ok(());
    }
    save_to(path, &file)
}

fn get_tag_at(path: &Path, target: &VeyraPath) -> Option<TagColor> {
    let uri = target.to_gio_file().uri().to_string();
    load_from(path).tags.get(&uri).copied()
}

fn get_paths_by_tag_at(path: &Path, color: TagColor) -> Vec<VeyraPath> {
    load_from(path)
        .tags
        .into_iter()
        .filter(|(_, c)| *c == color)
        .map(|(uri, _)| VeyraPath::from_uri(uri))
        .collect()
}

fn list_all_tagged_at(path: &Path) -> Vec<(VeyraPath, TagColor)> {
    load_from(path)
        .tags
        .into_iter()
        .map(|(uri, color)| (VeyraPath::from_uri(uri), color))
        .collect()
}

fn clear_all_tags_at(path: &Path) -> io::Result<()> {
    let mut file = load_from(path);
    if file.tags.is_empty() {
        return Ok(());
    }
    file.tags.clear();
    save_to(path, &file)
}

/// Removes every tag assignment whose target no longer exists on disk
/// (the file/folder was since deleted, moved, or renamed outside Veyra's
/// own move/rename paths, which otherwise keep `tags.json` in sync) —
/// Faz 65's "Kullanılmayan Etiketleri Temizle" (Clear Unused Tags) button.
/// Returns how many stale entries were removed. A local-path URI resolves
/// back to a plain filesystem check (`Path::exists`); a non-local URI
/// (remote GVfs mount) is left alone — its target may simply be
/// unreachable right now (Rule #16/#17: a path isn't stale just because
/// it's momentarily unreachable).
fn clear_unused_tags_at(path: &Path) -> io::Result<usize> {
    let mut file = load_from(path);
    let before = file.tags.len();
    file.tags
        .retain(|uri, _| match gio::File::for_uri(uri).path() {
            Some(local) => local.exists(),
            None => true,
        });
    let removed = before - file.tags.len();
    if removed == 0 {
        return Ok(0);
    }
    save_to(path, &file)?;
    Ok(removed)
}

/// Sets `color`'s custom display name to `name`. A blank/whitespace-only
/// `name` clears the override instead of storing an empty string, falling
/// back to the default localized color name — same "blank clears the
/// custom value" contract `bookmarks::rename_at` uses for bookmark labels.
fn set_custom_tag_name_at(path: &Path, color: TagColor, name: &str) -> io::Result<()> {
    let trimmed = name.trim();
    let mut file = load_from(path);
    if trimmed.is_empty() {
        if file.custom_names.remove(color.slug()).is_none() {
            return Ok(());
        }
    } else {
        file.custom_names
            .insert(color.slug().to_string(), trimmed.to_string());
    }
    save_to(path, &file)
}

fn get_custom_tag_name_at(path: &Path, color: TagColor) -> Option<String> {
    load_from(path).custom_names.get(color.slug()).cloned()
}

fn reset_custom_tag_names_at(path: &Path) -> io::Result<()> {
    let mut file = load_from(path);
    if file.custom_names.is_empty() {
        return Ok(());
    }
    file.custom_names.clear();
    save_to(path, &file)
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

/// Removes every path's tag assignment in the real tags file — the
/// "Tüm Etiketleri Temizle" (Clear All Tags) Preferences button — leaving
/// custom color names untouched.
pub fn clear_all_tags() -> io::Result<()> {
    clear_all_tags_at(&tags_path())
}

/// Removes every tag assignment pointing at a since-deleted local path in
/// the real tags file — the "Kullanılmayan Etiketleri Temizle" (Clear
/// Unused Tags) Preferences button. Returns how many entries were removed.
pub fn clear_unused_tags() -> io::Result<usize> {
    clear_unused_tags_at(&tags_path())
}

/// Sets `color`'s custom display name in the real tags file (Faz 63). A
/// blank/whitespace-only `name` clears the override.
pub fn set_custom_tag_name(color: TagColor, name: &str) -> io::Result<()> {
    set_custom_tag_name_at(&tags_path(), color, name)
}

/// `color`'s custom display name in the real tags file, if the user has set
/// one.
pub fn get_custom_tag_name(color: TagColor) -> Option<String> {
    get_custom_tag_name_at(&tags_path(), color)
}

/// Clears every color's custom display name in the real tags file, back to
/// the default localized color names.
pub fn reset_custom_tag_names() -> io::Result<()> {
    reset_custom_tag_names_at(&tags_path())
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
        let file = load_from(&path);
        assert!(file.tags.is_empty());
        assert!(file.custom_names.is_empty());
    }

    #[test]
    fn load_from_corrupt_json_falls_back_to_empty() {
        let path = temp_tags_path("load_from_corrupt_json_falls_back_to_empty");
        std::fs::write(&path, b"{ not valid json").unwrap();

        let file = load_from(&path);
        assert!(file.tags.is_empty());
        assert!(file.custom_names.is_empty());

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn set_custom_tag_name_then_get_round_trips() {
        let path = temp_tags_path("set_custom_tag_name_then_get_round_trips");
        std::fs::remove_file(&path).ok();

        set_custom_tag_name_at(&path, TagColor::Red, "Urgent").unwrap();

        assert_eq!(
            get_custom_tag_name_at(&path, TagColor::Red),
            Some("Urgent".to_string())
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn set_custom_tag_name_trims_whitespace() {
        let path = temp_tags_path("set_custom_tag_name_trims_whitespace");
        std::fs::remove_file(&path).ok();

        set_custom_tag_name_at(&path, TagColor::Blue, "  Work  ").unwrap();

        assert_eq!(
            get_custom_tag_name_at(&path, TagColor::Blue),
            Some("Work".to_string())
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn set_custom_tag_name_blank_clears_the_override() {
        let path = temp_tags_path("set_custom_tag_name_blank_clears_the_override");
        std::fs::remove_file(&path).ok();

        set_custom_tag_name_at(&path, TagColor::Green, "Done").unwrap();
        set_custom_tag_name_at(&path, TagColor::Green, "   ").unwrap();

        assert_eq!(get_custom_tag_name_at(&path, TagColor::Green), None);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn get_custom_tag_name_on_missing_file_is_none() {
        let path = temp_tags_path("get_custom_tag_name_on_missing_file_is_none");
        std::fs::remove_file(&path).ok();

        assert_eq!(get_custom_tag_name_at(&path, TagColor::Purple), None);
    }

    #[test]
    fn reset_custom_tag_names_clears_every_override_but_keeps_tag_assignments() {
        let path = temp_tags_path(
            "reset_custom_tag_names_clears_every_override_but_keeps_tag_assignments",
        );
        std::fs::remove_file(&path).ok();

        set_custom_tag_name_at(&path, TagColor::Red, "Urgent").unwrap();
        set_custom_tag_name_at(&path, TagColor::Blue, "Work").unwrap();
        let tagged_path = VeyraPath::from_local("/home/user/A");
        set_tag_at(&path, &tagged_path, TagColor::Red).unwrap();

        reset_custom_tag_names_at(&path).unwrap();

        assert_eq!(get_custom_tag_name_at(&path, TagColor::Red), None);
        assert_eq!(get_custom_tag_name_at(&path, TagColor::Blue), None);
        assert_eq!(get_tag_at(&path, &tagged_path), Some(TagColor::Red));

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn clear_all_tags_empties_assignments_but_keeps_custom_names() {
        let path = temp_tags_path("clear_all_tags_empties_assignments_but_keeps_custom_names");
        std::fs::remove_file(&path).ok();

        set_custom_tag_name_at(&path, TagColor::Red, "Urgent").unwrap();
        set_tag_at(&path, &VeyraPath::from_local("/home/user/A"), TagColor::Red).unwrap();
        set_tag_at(
            &path,
            &VeyraPath::from_local("/home/user/B"),
            TagColor::Blue,
        )
        .unwrap();

        clear_all_tags_at(&path).unwrap();

        assert!(list_all_tagged_at(&path).is_empty());
        assert_eq!(
            get_custom_tag_name_at(&path, TagColor::Red),
            Some("Urgent".to_string())
        );

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn clear_all_tags_on_empty_store_is_a_no_op() {
        let path = temp_tags_path("clear_all_tags_on_empty_store_is_a_no_op");
        std::fs::remove_file(&path).ok();

        assert!(clear_all_tags_at(&path).is_ok());
        assert!(list_all_tagged_at(&path).is_empty());
    }

    #[test]
    fn clear_unused_tags_removes_only_entries_for_deleted_local_paths() {
        let path = temp_tags_path("clear_unused_tags_removes_only_entries_for_deleted_local_paths");
        std::fs::remove_file(&path).ok();

        let existing = tempfile::NamedTempFile::new().unwrap();
        let existing_path = VeyraPath::from_local(existing.path());
        let missing_path = VeyraPath::from_local("/nonexistent/veyra-tags-test/gone.txt");
        set_tag_at(&path, &existing_path, TagColor::Red).unwrap();
        set_tag_at(&path, &missing_path, TagColor::Blue).unwrap();

        let removed = clear_unused_tags_at(&path).unwrap();

        assert_eq!(removed, 1);
        assert_eq!(get_tag_at(&path, &existing_path), Some(TagColor::Red));
        assert_eq!(get_tag_at(&path, &missing_path), None);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn clear_unused_tags_on_empty_store_is_a_no_op() {
        let path = temp_tags_path("clear_unused_tags_on_empty_store_is_a_no_op");
        std::fs::remove_file(&path).ok();

        assert_eq!(clear_unused_tags_at(&path).unwrap(), 0);
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
