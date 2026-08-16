//! Faz 34: the single source of truth for every user-configurable app
//! preference (appearance, navigation, files, search, preview, performance,
//! privacy), persisted as `~/.config/veyra/settings.json`.
//!
//! Mirrors `shortcuts.rs`'s own load/save/atomic-write pattern: a corrupt or
//! missing file silently falls back to `VeyraSettings::default()` (Kural #4,
//! #48 — a bad config must never crash the app), and every write goes
//! through `veyra_core::security::write_atomic_private` so a crash mid-save
//! can never leave a half-written file behind.

use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::glib;

/// Theme preference driving `libadwaita::StyleManager::set_color_scheme`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub(crate) enum ColorSchemePref {
    #[default]
    System,
    Light,
    Dark,
}

impl ColorSchemePref {
    pub(crate) fn to_adw(self) -> libadwaita::ColorScheme {
        match self {
            ColorSchemePref::System => libadwaita::ColorScheme::Default,
            ColorSchemePref::Light => libadwaita::ColorScheme::ForceLight,
            ColorSchemePref::Dark => libadwaita::ColorScheme::ForceDark,
        }
    }
}

/// Grid/Compact view icon pixel size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub(crate) enum IconSizePref {
    Small,
    #[serde(alias = "Medium")]
    #[default]
    Normal,
    Large,
    ExtraLarge,
}

impl IconSizePref {
    pub(crate) fn pixels(self) -> i32 {
        match self {
            IconSizePref::Small => 48,
            IconSizePref::Normal => 64,
            IconSizePref::Large => 96,
            IconSizePref::ExtraLarge => 128,
        }
    }

    pub(crate) const ALL: [IconSizePref; 4] = [
        IconSizePref::Small,
        IconSizePref::Normal,
        IconSizePref::Large,
        IconSizePref::ExtraLarge,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            IconSizePref::Small => "Small (48px)",
            IconSizePref::Normal => "Medium (64px)",
            IconSizePref::Large => "Large (96px)",
            IconSizePref::ExtraLarge => "Extra Large (128px)",
        }
    }
}

/// Whether a single click or a double click opens/navigates into an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub(crate) enum ClickPolicy {
    #[default]
    DoubleClick,
    SingleClick,
}

/// The view a newly opened tab starts in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub(crate) enum DefaultViewMode {
    #[default]
    Icon,
    Compact,
    Details,
}

impl DefaultViewMode {
    pub(crate) fn to_view_mode(self) -> crate::views::ViewMode {
        match self {
            DefaultViewMode::Icon => crate::views::ViewMode::Icon,
            DefaultViewMode::Compact => crate::views::ViewMode::Compact,
            DefaultViewMode::Details => crate::views::ViewMode::Details,
        }
    }
}

/// Every user-configurable Veyra preference. `#[serde(default)]` on every
/// field means a `settings.json` from an older Veyra version (missing
/// fields a newer version added) still loads cleanly instead of failing —
/// each missing field just takes its `Default` value.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct VeyraSettings {
    // --- Appearance ---
    #[serde(default)]
    pub color_scheme: ColorSchemePref,
    #[serde(default)]
    pub icon_size: IconSizePref,

    // --- Navigation ---
    #[serde(default)]
    pub click_policy: ClickPolicy,
    #[serde(default)]
    pub open_folders_in_new_tab: bool,
    #[serde(default)]
    pub restore_tabs_on_startup: bool,

    // --- Files & Display ---
    #[serde(default)]
    pub show_hidden: bool,
    #[serde(default = "default_true")]
    pub folders_first: bool,
    #[serde(default)]
    pub default_view_mode: DefaultViewMode,
    #[serde(default = "default_true")]
    pub confirm_trash_empty: bool,
    #[serde(default = "default_true")]
    pub confirm_permanent_delete: bool,

    // --- Search & Indexing ---
    #[serde(default = "default_true")]
    pub enable_fts_index: bool,
    #[serde(default = "default_max_search_results")]
    pub max_search_results: usize,

    // --- Preview ---
    #[serde(default = "default_true")]
    pub enable_preview_panel: bool,
    #[serde(default = "default_max_preview_size_kb")]
    pub max_preview_size_kb: usize,
    #[serde(default = "default_true")]
    pub show_folder_count: bool,

    // --- Performance ---
    #[serde(default = "default_stream_chunk_size")]
    pub stream_chunk_size: usize,
    #[serde(default = "default_thumbnail_cache_capacity")]
    pub thumbnail_cache_capacity: usize,

    // --- Privacy ---
    #[serde(default)]
    pub sanitize_log_paths: bool,
    #[serde(default = "default_true")]
    pub store_recent_files: bool,
}

fn default_true() -> bool {
    true
}

fn default_max_search_results() -> usize {
    500
}

fn default_max_preview_size_kb() -> usize {
    1024
}

fn default_stream_chunk_size() -> usize {
    veyra_filesystem::READ_DIR_CHUNK_SIZE
}

fn default_thumbnail_cache_capacity() -> usize {
    1000
}

impl Default for VeyraSettings {
    fn default() -> Self {
        VeyraSettings {
            color_scheme: ColorSchemePref::default(),
            icon_size: IconSizePref::default(),
            click_policy: ClickPolicy::default(),
            open_folders_in_new_tab: false,
            restore_tabs_on_startup: false,
            show_hidden: false,
            folders_first: true,
            default_view_mode: DefaultViewMode::default(),
            confirm_trash_empty: true,
            confirm_permanent_delete: true,
            enable_fts_index: true,
            max_search_results: default_max_search_results(),
            enable_preview_panel: true,
            max_preview_size_kb: default_max_preview_size_kb(),
            show_folder_count: true,
            stream_chunk_size: default_stream_chunk_size(),
            thumbnail_cache_capacity: default_thumbnail_cache_capacity(),
            sanitize_log_paths: false,
            store_recent_files: true,
        }
    }
}

/// The allowed picker values for the Search / Preview / Performance pages'
/// combo rows (spec-mandated fixed choices rather than free-form numeric
/// entry).
pub(crate) const MAX_PREVIEW_SIZE_CHOICES_KB: [usize; 3] = [512, 1024, 2048];
pub(crate) const STREAM_CHUNK_SIZE_CHOICES: [usize; 3] = [250, 500, 1000];
pub(crate) const THUMBNAIL_CACHE_CAPACITY_CHOICES: [usize; 3] = [500, 1000, 2000];

/// The runtime-shared handle every window/panel/view/dialog clones to read
/// and mutate the app's live preferences, matching the `Rc<RefCell<_>>`
/// convention `TabPage`/`AppState` already use.
pub(crate) type SharedSettings = Rc<RefCell<VeyraSettings>>;

impl VeyraSettings {
    /// Loads `~/.config/veyra/settings.json`, falling back to
    /// `VeyraSettings::default()` when the file is missing, unreadable, or
    /// not valid JSON.
    pub(crate) fn load() -> Self {
        load_from(&config_path())
    }

    /// Atomically writes this settings snapshot to
    /// `~/.config/veyra/settings.json`.
    pub(crate) fn save(&self) -> io::Result<()> {
        save_to(&config_path(), self)
    }
}

fn config_dir() -> PathBuf {
    glib::user_config_dir().join("veyra")
}

fn config_path() -> PathBuf {
    config_dir().join("settings.json")
}

fn load_from(path: &Path) -> VeyraSettings {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return VeyraSettings::default();
    };
    match serde_json::from_str(&contents) {
        Ok(settings) => settings,
        Err(err) => {
            tracing::warn!(path = %path.display(), error = %err, "settings.json is not valid JSON, using defaults");
            VeyraSettings::default()
        }
    }
}

fn save_to(path: &Path, settings: &VeyraSettings) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(settings)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let tmp_path = path.with_extension("json.tmp");
    veyra_core::security::write_atomic_private(&tmp_path, path, json.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_round_trips_through_json() {
        let settings = VeyraSettings::default();
        let json = serde_json::to_string(&settings).unwrap();
        let restored: VeyraSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, settings);
    }

    #[test]
    fn load_from_missing_file_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist.json");
        assert_eq!(load_from(&path), VeyraSettings::default());
    }

    #[test]
    fn load_from_corrupt_file_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, b"{ not valid json").unwrap();
        assert_eq!(load_from(&path), VeyraSettings::default());
    }

    #[test]
    fn load_from_empty_object_fills_in_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, b"{}").unwrap();
        assert_eq!(load_from(&path), VeyraSettings::default());
    }

    #[test]
    fn load_from_partial_object_merges_with_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, br#"{"show_hidden": true}"#).unwrap();
        let loaded = load_from(&path);
        assert!(loaded.show_hidden);
        assert!(loaded.folders_first);
    }

    #[test]
    fn save_then_load_round_trips_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("settings.json");
        let settings = VeyraSettings {
            show_hidden: true,
            icon_size: IconSizePref::Large,
            color_scheme: ColorSchemePref::Dark,
            ..VeyraSettings::default()
        };
        save_to(&path, &settings).unwrap();
        assert!(path.exists());
        assert!(!path.with_extension("json.tmp").exists());
        let loaded = load_from(&path);
        assert_eq!(loaded, settings);
    }

    #[test]
    fn icon_size_pixels_match_spec() {
        assert_eq!(IconSizePref::Small.pixels(), 48);
        assert_eq!(IconSizePref::Normal.pixels(), 64);
        assert_eq!(IconSizePref::Large.pixels(), 96);
        assert_eq!(IconSizePref::ExtraLarge.pixels(), 128);
    }
}
