//! Faz 25: keyboard shortcut configuration — a `ShortcutMap` of `win.*`
//! action name to accelerator strings (GTK's own `<Primary>c`-style syntax),
//! persisted as `~/.config/veyra/shortcuts.json` and applied to the
//! `Application`'s accelerator table on startup and whenever it changes.
//!
//! This is the single source of truth for every action's keybinding: the
//! Command Palette badges and the Keyboard Shortcuts help window both read
//! accelerators back out of GTK's own accel table (`Application::
//! accels_for_action`) rather than keeping a second copy, so they can never
//! drift out of sync with what's actually bound (Kural #29, keyboard-first).

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

/// A category grouping for the Keyboard Shortcuts help window, matching the
/// Command Palette's own categories.
pub(crate) const CATEGORY_NAVIGATION: &str = "Navigation";
pub(crate) const CATEGORY_FILE_OPERATIONS: &str = "File Operations";
pub(crate) const CATEGORY_VIEW: &str = "View";
pub(crate) const CATEGORY_TABS: &str = "Tabs";
pub(crate) const CATEGORY_TOOLS: &str = "Tools";

/// One entry in the shortcuts catalog: a `win.*` action's display label and
/// help-window category. Independent of whether the action currently has an
/// accelerator bound — `ShortcutMap` owns that.
pub(crate) struct ShortcutEntry {
    pub action: &'static str,
    pub label: &'static str,
    pub category: &'static str,
}

/// Every action the Keyboard Shortcuts help window lists, in display order.
/// `default_shortcuts()` below must have a default (possibly empty) entry
/// for each `action` here, and vice versa — checked by
/// `catalog_matches_defaults` in the tests.
pub(crate) fn catalog() -> Vec<ShortcutEntry> {
    vec![
        ShortcutEntry {
            action: "win.toggle-search",
            label: "Search Files",
            category: CATEGORY_NAVIGATION,
        },
        ShortcutEntry {
            action: "win.go-back",
            label: "Go Back",
            category: CATEGORY_NAVIGATION,
        },
        ShortcutEntry {
            action: "win.go-forward",
            label: "Go Forward",
            category: CATEGORY_NAVIGATION,
        },
        ShortcutEntry {
            action: "win.go-up",
            label: "Go Up",
            category: CATEGORY_NAVIGATION,
        },
        ShortcutEntry {
            action: "win.refresh",
            label: "Refresh",
            category: CATEGORY_NAVIGATION,
        },
        ShortcutEntry {
            action: "win.focus-address",
            label: "Go to Location",
            category: CATEGORY_NAVIGATION,
        },
        ShortcutEntry {
            action: "win.toggle-hidden-files",
            label: "Toggle Hidden Files",
            category: CATEGORY_NAVIGATION,
        },
        ShortcutEntry {
            action: "win.select-all",
            label: "Select All",
            category: CATEGORY_NAVIGATION,
        },
        ShortcutEntry {
            action: "win.copy-selection",
            label: "Copy",
            category: CATEGORY_FILE_OPERATIONS,
        },
        ShortcutEntry {
            action: "win.cut-selection",
            label: "Cut",
            category: CATEGORY_FILE_OPERATIONS,
        },
        ShortcutEntry {
            action: "win.paste",
            label: "Paste",
            category: CATEGORY_FILE_OPERATIONS,
        },
        ShortcutEntry {
            action: "win.trash-selection",
            label: "Move to Trash",
            category: CATEGORY_FILE_OPERATIONS,
        },
        ShortcutEntry {
            action: "win.delete-selection",
            label: "Delete Permanently",
            category: CATEGORY_FILE_OPERATIONS,
        },
        ShortcutEntry {
            action: "win.rename-selected",
            label: "Rename",
            category: CATEGORY_FILE_OPERATIONS,
        },
        ShortcutEntry {
            action: "win.create-folder",
            label: "New Folder",
            category: CATEGORY_FILE_OPERATIONS,
        },
        ShortcutEntry {
            action: "win.restore-selected",
            label: "Restore from Trash",
            category: CATEGORY_FILE_OPERATIONS,
        },
        ShortcutEntry {
            action: "win.undo",
            label: "Undo",
            category: CATEGORY_FILE_OPERATIONS,
        },
        ShortcutEntry {
            action: "win.redo",
            label: "Redo",
            category: CATEGORY_FILE_OPERATIONS,
        },
        ShortcutEntry {
            action: "win.copy-to-other-panel-selected",
            label: "Copy to Other Panel",
            category: CATEGORY_FILE_OPERATIONS,
        },
        ShortcutEntry {
            action: "win.move-to-other-panel-selected",
            label: "Move to Other Panel",
            category: CATEGORY_FILE_OPERATIONS,
        },
        ShortcutEntry {
            action: "win.toggle-preview",
            label: "Toggle Preview",
            category: CATEGORY_VIEW,
        },
        ShortcutEntry {
            action: "win.toggle-split-view",
            label: "Toggle Split View",
            category: CATEGORY_VIEW,
        },
        ShortcutEntry {
            action: "win.set-view-mode::icon",
            label: "Icon View",
            category: CATEGORY_VIEW,
        },
        ShortcutEntry {
            action: "win.set-view-mode::compact",
            label: "Compact View",
            category: CATEGORY_VIEW,
        },
        ShortcutEntry {
            action: "win.set-view-mode::details",
            label: "Details View",
            category: CATEGORY_VIEW,
        },
        ShortcutEntry {
            action: "win.new-tab",
            label: "New Tab",
            category: CATEGORY_TABS,
        },
        ShortcutEntry {
            action: "win.close-tab",
            label: "Close Tab",
            category: CATEGORY_TABS,
        },
        ShortcutEntry {
            action: "win.next-tab",
            label: "Next Tab",
            category: CATEGORY_TABS,
        },
        ShortcutEntry {
            action: "win.previous-tab",
            label: "Previous Tab",
            category: CATEGORY_TABS,
        },
        ShortcutEntry {
            action: "win.analyze-disk-current",
            label: "Analyze Disk Usage",
            category: CATEGORY_TOOLS,
        },
        ShortcutEntry {
            action: "win.open-terminal-here-current",
            label: "Open Terminal Here",
            category: CATEGORY_TOOLS,
        },
        ShortcutEntry {
            action: "win.command-palette",
            label: "Command Palette",
            category: CATEGORY_TOOLS,
        },
        ShortcutEntry {
            action: "win.properties-selected",
            label: "Properties",
            category: CATEGORY_TOOLS,
        },
        ShortcutEntry {
            action: "win.show-shortcuts-help",
            label: "Keyboard Shortcuts",
            category: CATEGORY_TOOLS,
        },
        ShortcutEntry {
            action: "win.reset-shortcuts",
            label: "Reset Shortcuts to Default",
            category: CATEGORY_TOOLS,
        },
    ]
}

/// Every action's default accelerator(s), keyed by detailed action name
/// (`win.foo`, or `win.foo::target` for the handful of parameterized
/// actions that bind per-target). Mirrors exactly what `window.rs`'s
/// `setup_*_actions` functions bind at startup — `apply_to_application`
/// re-applies this (or the user's customized copy) on top of those calls,
/// so a `shortcuts.json` with no override for a given action leaves its
/// compiled-in default untouched.
pub(crate) fn default_shortcuts() -> ShortcutMap {
    let mut map = BTreeMap::new();
    let mut set = |action: &str, accels: &[&str]| {
        map.insert(
            action.to_string(),
            accels.iter().map(|accel| accel.to_string()).collect(),
        );
    };
    set("win.toggle-search", &["<Primary>f"]);
    set("win.go-back", &["<Alt>Left"]);
    set("win.go-forward", &["<Alt>Right"]);
    set("win.go-up", &["<Alt>Up"]);
    set("win.refresh", &["F5"]);
    set("win.focus-address", &["<Primary>l"]);
    set("win.toggle-hidden-files", &["<Primary>h"]);
    set("win.select-all", &["<Primary>a"]);
    set("win.new-tab", &["<Primary>t"]);
    set("win.close-tab", &["<Primary>w"]);
    set("win.next-tab", &["<Primary>Tab"]);
    set("win.previous-tab", &["<Primary><Shift>Tab"]);
    set("win.toggle-split-view", &["F3"]);
    set("win.copy-to-other-panel-selected", &["<Primary><Shift>o"]);
    set("win.move-to-other-panel-selected", &["<Primary><Shift>m"]);
    set("win.copy-selection", &["<Primary>c"]);
    set("win.cut-selection", &["<Primary>x"]);
    set("win.paste", &["<Primary>v"]);
    set("win.trash-selection", &["Delete"]);
    set("win.delete-selection", &["<Shift>Delete"]);
    set("win.rename-selected", &["F2"]);
    set("win.create-folder", &["<Primary><Shift>n"]);
    set("win.properties-selected", &["<Alt>Return"]);
    set("win.toggle-preview", &["F9"]);
    set("win.analyze-disk-current", &["<Primary><Shift>u"]);
    set("win.open-terminal-here-current", &["F4", "<Primary><Alt>t"]);
    set("win.command-palette", &["<Primary>k", "<Primary><Shift>p"]);
    set("win.set-view-mode::icon", &["<Primary>1"]);
    set("win.set-view-mode::compact", &["<Primary>2"]);
    set("win.set-view-mode::details", &["<Primary>3"]);
    set("win.restore-selected", &["<Primary><Shift>r"]);
    set("win.undo", &["<Primary>z"]);
    set("win.redo", &["<Primary><Shift>z", "<Primary>y"]);
    set("win.show-shortcuts-help", &["<Primary>question"]);
    // No default accelerator: reset is destructive to a user's
    // customization and is reachable from the shortcuts help window and
    // Command Palette, so it doesn't need one (and shouldn't risk firing
    // by accident, echoing Kural #38/#39's spirit for destructive actions).
    set("win.reset-shortcuts", &[]);
    ShortcutMap(map)
}

/// Action name (e.g. `win.copy-selection`, or `win.set-view-mode::icon` for
/// a parameterized action bound per-target) to its accelerator strings, in
/// GTK's own `<Primary>c` syntax. Serializes as a plain JSON object for
/// `~/.config/veyra/shortcuts.json`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ShortcutMap(BTreeMap<String, Vec<String>>);

impl ShortcutMap {
    /// The accelerators currently configured for `action` in this map, or
    /// an empty slice if it has none (either never had one, or the user
    /// cleared it). `window.rs` reads accelerators back out of GTK's own
    /// accel table instead (`Application::accels_for_action`) once this map
    /// has been applied, so both stay in lockstep with what's actually
    /// bound; this accessor exists for tests and for a future shortcut
    /// editor to read the configured (not-yet-applied) value.
    #[cfg(test)]
    pub(crate) fn accels(&self, action: &str) -> &[String] {
        self.0.get(action).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Loads `~/.config/veyra/shortcuts.json`, falling back to
    /// `default_shortcuts()` when the file is missing, unreadable, or not
    /// valid JSON (Kural #4 — a corrupt config must never break the app).
    /// Any accelerator string that fails to parse is dropped rather than
    /// applied, so a hand-edited or corrupted file can't hand GTK a bogus
    /// accel string.
    pub(crate) fn load() -> Self {
        load_from(&config_path())
    }

    /// Atomically writes this map to `~/.config/veyra/shortcuts.json`
    /// (write to a `.tmp` sibling, then `rename` over the real path — the
    /// same pattern `network.rs`'s history file uses, so a crash mid-write
    /// never leaves a half-written config behind).
    pub(crate) fn save(&self) -> io::Result<()> {
        save_to(&config_path(), self)
    }

    /// Applies every action's accelerators to `app`'s accel table. Actions
    /// this map has no entry for are left exactly as `window.rs`'s
    /// `setup_*_actions` compiled-in calls already set them — this only
    /// overrides actions the map actually lists, which is every action in
    /// `default_shortcuts()` plus whatever a hand-edited config adds.
    pub(crate) fn apply_to_application(&self, app: &adw::Application) {
        for (action, accels) in &self.0 {
            let accels: Vec<&str> = accels.iter().map(String::as_str).collect();
            app.set_accels_for_action(action, &accels);
        }
    }
}

fn config_dir() -> PathBuf {
    glib::user_config_dir().join("veyra")
}

fn config_path() -> PathBuf {
    config_dir().join("shortcuts.json")
}

/// A structural check that `accel` matches the accelerator strings this
/// module ever produces or consumes — zero or more `<Primary>`/`<Shift>`/
/// `<Alt>`/`<Super>`/`<Meta>`/`<Control>` modifier tokens, followed by a
/// non-empty alphanumeric key name (`c`, `F5`, `Return`, `Tab`, `question`,
/// …). Used both to reject bogus entries from a hand-edited config and to
/// validate a user's customization before it's saved.
///
/// Deliberately a plain string check rather than `gtk4::accelerator_parse`:
/// that binding requires GTK to already be initialized (a display
/// connection), which headless unit tests don't have, and this module's
/// whole point is staying testable without one — matching `command_palette`
/// keeping its matching logic free of GTK widget construction.
pub(crate) fn is_valid_accel(accel: &str) -> bool {
    if accel.is_empty() {
        return false;
    }
    let mut rest = accel;
    while let Some(stripped) = rest.strip_prefix('<') {
        let Some(end) = stripped.find('>') else {
            return false;
        };
        let token = &stripped[..end];
        if !matches!(
            token,
            "Primary" | "Control" | "Shift" | "Alt" | "Super" | "Meta"
        ) {
            return false;
        }
        rest = &stripped[end + 1..];
    }
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphanumeric())
}

fn load_from(path: &Path) -> ShortcutMap {
    let defaults = default_shortcuts();
    let Ok(contents) = std::fs::read_to_string(path) else {
        return defaults;
    };
    let Ok(raw) = serde_json::from_str::<BTreeMap<String, Vec<String>>>(&contents) else {
        tracing::warn!(path = %path.display(), "shortcuts.json is not valid JSON, using defaults");
        return defaults;
    };

    let mut merged = defaults;
    for (action, accels) in raw {
        let valid: Vec<String> = accels
            .into_iter()
            .filter(|accel| is_valid_accel(accel))
            .collect();
        merged.0.insert(action, valid);
    }
    merged
}

fn save_to(path: &Path, map: &ShortcutMap) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&map.0)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let tmp_path = path.with_extension("json.tmp");
    veyra_core::security::write_atomic_private(&tmp_path, path, json.as_bytes())
}

/// Renders `accel` (GTK's `<Primary>c` syntax) as a display string like
/// `Ctrl+Shift+N`, for the Command Palette's shortcut badges and the
/// Keyboard Shortcuts help window. Pure string formatting (no GTK display
/// connection required), so it stays testable and deterministic regardless
/// of locale.
pub(crate) fn format_accel(accel: &str) -> String {
    let mut modifiers = Vec::new();
    let mut rest = accel;
    while rest.starts_with('<') {
        let Some(end) = rest.find('>') else { break };
        let token = &rest[1..end];
        modifiers.push(match token {
            "Primary" | "Control" => "Ctrl",
            "Shift" => "Shift",
            "Alt" => "Alt",
            "Super" | "Meta" => "Super",
            other => other,
        });
        rest = &rest[end + 1..];
    }

    let key = format_key(rest);
    if key.is_empty() {
        modifiers.join("+")
    } else {
        modifiers.push(&key);
        modifiers.join("+")
    }
}

fn format_key(key: &str) -> String {
    match key {
        "Return" => "Enter".to_string(),
        "question" => "?".to_string(),
        "" => String::new(),
        other if other.chars().count() == 1 => other.to_uppercase(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalog_action_has_a_default_entry() {
        let defaults = default_shortcuts();
        for entry in catalog() {
            assert!(
                defaults.0.contains_key(entry.action),
                "catalog action {} has no default_shortcuts() entry",
                entry.action
            );
        }
    }

    #[test]
    fn every_default_entry_is_in_the_catalog() {
        let defaults = default_shortcuts();
        let catalog_actions: Vec<&str> = catalog().iter().map(|entry| entry.action).collect();
        for action in defaults.0.keys() {
            assert!(
                catalog_actions.contains(&action.as_str()),
                "default_shortcuts() action {action} is missing from the catalog"
            );
        }
    }

    #[test]
    fn default_accels_are_all_valid() {
        for accels in default_shortcuts().0.values() {
            for accel in accels {
                assert!(is_valid_accel(accel), "invalid default accel: {accel}");
            }
        }
    }

    #[test]
    fn accels_returns_empty_slice_for_unknown_action() {
        let map = default_shortcuts();
        assert!(map.accels("win.does-not-exist").is_empty());
    }

    #[test]
    fn accels_returns_configured_values() {
        let map = default_shortcuts();
        assert_eq!(
            map.accels("win.copy-selection"),
            &["<Primary>c".to_string()]
        );
    }

    #[test]
    fn reset_shortcuts_has_no_default_accel() {
        let map = default_shortcuts();
        assert!(map.accels("win.reset-shortcuts").is_empty());
    }

    #[test]
    fn round_trips_through_json() {
        let map = default_shortcuts();
        let json = serde_json::to_string(&map.0).unwrap();
        let restored: BTreeMap<String, Vec<String>> = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, map.0);
    }

    #[test]
    fn load_from_missing_file_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist.json");
        assert_eq!(load_from(&path), default_shortcuts());
    }

    #[test]
    fn load_from_corrupt_file_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shortcuts.json");
        std::fs::write(&path, b"{ not valid json").unwrap();
        assert_eq!(load_from(&path), default_shortcuts());
    }

    #[test]
    fn load_from_valid_file_overrides_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shortcuts.json");
        std::fs::write(&path, r#"{"win.copy-selection": ["<Primary><Shift>c"]}"#).unwrap();
        let map = load_from(&path);
        assert_eq!(
            map.accels("win.copy-selection"),
            &["<Primary><Shift>c".to_string()]
        );
        // Untouched actions keep their default.
        assert_eq!(map.accels("win.paste"), &["<Primary>v".to_string()]);
    }

    #[test]
    fn load_drops_invalid_accel_strings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shortcuts.json");
        std::fs::write(&path, r#"{"win.copy-selection": ["not-a-real-accel"]}"#).unwrap();
        let map = load_from(&path);
        assert!(map.accels("win.copy-selection").is_empty());
    }

    #[test]
    fn save_then_load_round_trips_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("shortcuts.json");
        let mut map = default_shortcuts();
        map.0.insert(
            "win.copy-selection".to_string(),
            vec!["<Primary><Alt>c".to_string()],
        );
        save_to(&path, &map).unwrap();
        assert!(path.exists());
        assert!(!path.with_extension("json.tmp").exists());
        let loaded = load_from(&path);
        assert_eq!(
            loaded.accels("win.copy-selection"),
            &["<Primary><Alt>c".to_string()]
        );
    }

    #[test]
    fn is_valid_accel_rejects_garbage() {
        assert!(!is_valid_accel(""));
        assert!(!is_valid_accel("not-a-real-accel"));
    }

    #[test]
    fn is_valid_accel_accepts_known_forms() {
        assert!(is_valid_accel("<Primary>c"));
        assert!(is_valid_accel("F5"));
        assert!(is_valid_accel("<Primary><Shift>n"));
        assert!(is_valid_accel("Delete"));
    }

    #[test]
    fn format_accel_single_modifier() {
        assert_eq!(format_accel("<Primary>c"), "Ctrl+C");
    }

    #[test]
    fn format_accel_multiple_modifiers() {
        assert_eq!(format_accel("<Primary><Shift>n"), "Ctrl+Shift+N");
        assert_eq!(format_accel("<Primary><Shift>u"), "Ctrl+Shift+U");
    }

    #[test]
    fn format_accel_function_key_has_no_modifier() {
        assert_eq!(format_accel("F3"), "F3");
        assert_eq!(format_accel("F9"), "F9");
    }

    #[test]
    fn format_accel_special_keys() {
        assert_eq!(format_accel("<Alt>Return"), "Alt+Enter");
        assert_eq!(format_accel("<Primary>question"), "Ctrl+?");
        assert_eq!(format_accel("Delete"), "Delete");
        assert_eq!(format_accel("<Shift>Delete"), "Shift+Delete");
    }

    #[test]
    fn format_accel_matches_previous_command_palette_strings() {
        assert_eq!(format_accel("<Primary><Shift>n"), "Ctrl+Shift+N");
        assert_eq!(format_accel("<Primary>t"), "Ctrl+T");
        assert_eq!(format_accel("<Primary>w"), "Ctrl+W");
        assert_eq!(format_accel("F3"), "F3");
        assert_eq!(format_accel("F4"), "F4");
        assert_eq!(format_accel("<Primary>h"), "Ctrl+H");
        assert_eq!(format_accel("F9"), "F9");
        assert_eq!(format_accel("<Primary>1"), "Ctrl+1");
        assert_eq!(format_accel("<Primary>2"), "Ctrl+2");
        assert_eq!(format_accel("<Primary>3"), "Ctrl+3");
        assert_eq!(format_accel("<Primary><Shift>u"), "Ctrl+Shift+U");
        assert_eq!(format_accel("<Alt>Return"), "Alt+Enter");
        assert_eq!(format_accel("<Primary>f"), "Ctrl+F");
    }
}
