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

/// Faz 35: a Veyra-specific accent color override for Libadwaita's
/// `accent_color`/`accent_bg_color`/`accent_fg_color` named colors.
/// `System` leaves whatever the desktop's own Libadwaita accent is
/// untouched — every other variant installs a small CSS provider
/// redefining just those three variables, so widget metrics, spacing and
/// every other themed value stay exactly what the system theme says
/// (spec requirement: "sistem temasını ve widget metriklerini bozmadan").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub(crate) enum AccentColorPref {
    #[default]
    System,
    Blue,
    Teal,
    Green,
    Yellow,
    Orange,
    Red,
    Purple,
    Pink,
    Slate,
}

impl AccentColorPref {
    pub(crate) const ALL: [AccentColorPref; 9] = [
        AccentColorPref::Blue,
        AccentColorPref::Teal,
        AccentColorPref::Green,
        AccentColorPref::Yellow,
        AccentColorPref::Orange,
        AccentColorPref::Red,
        AccentColorPref::Purple,
        AccentColorPref::Pink,
        AccentColorPref::Slate,
    ];

    /// `None` for `System` (no override CSS to install), the spec-mandated
    /// hex string otherwise.
    pub(crate) fn hex(self) -> Option<&'static str> {
        match self {
            AccentColorPref::System => None,
            AccentColorPref::Blue => Some("#3584e4"),
            AccentColorPref::Teal => Some("#2190a4"),
            AccentColorPref::Green => Some("#3a944c"),
            AccentColorPref::Yellow => Some("#cd9309"),
            AccentColorPref::Orange => Some("#e66100"),
            AccentColorPref::Red => Some("#c01c28"),
            AccentColorPref::Purple => Some("#9141ac"),
            AccentColorPref::Pink => Some("#d56199"),
            AccentColorPref::Slate => Some("#5e5c64"),
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            AccentColorPref::System => crate::i18n::t("prefs.common.system_default"),
            AccentColorPref::Blue => crate::i18n::t("prefs.appearance.accent_color.blue"),
            AccentColorPref::Teal => crate::i18n::t("prefs.appearance.accent_color.teal"),
            AccentColorPref::Green => crate::i18n::t("prefs.appearance.accent_color.green"),
            AccentColorPref::Yellow => crate::i18n::t("prefs.appearance.accent_color.yellow"),
            AccentColorPref::Orange => crate::i18n::t("prefs.appearance.accent_color.orange"),
            AccentColorPref::Red => crate::i18n::t("prefs.appearance.accent_color.red"),
            AccentColorPref::Purple => crate::i18n::t("prefs.appearance.accent_color.purple"),
            AccentColorPref::Pink => crate::i18n::t("prefs.appearance.accent_color.pink"),
            AccentColorPref::Slate => crate::i18n::t("prefs.appearance.accent_color.slate"),
        }
    }

    /// The `@define-color` block installed for this accent, or `None` for
    /// `System` (nothing to install — any existing override is removed
    /// instead, see `apply_accent_color`). `accent_bg_color` mirrors
    /// `accent_color` (Libadwaita's own convention for solid accent
    /// surfaces), and `accent_fg_color` is whichever of black/white gives
    /// the better contrast against it.
    fn css(self) -> Option<String> {
        let hex = self.hex()?;
        let fg = contrasting_fg(hex);
        Some(format!(
            "@define-color accent_color {hex};\n\
             @define-color accent_bg_color {hex};\n\
             @define-color accent_fg_color {fg};\n"
        ))
    }
}

/// Parses a `#rrggbb` hex string into `(r, g, b)` bytes. Panics on
/// malformed input — every caller passes one of the hardcoded, unit-tested
/// literals from `AccentColorPref::hex`, never user input.
fn parse_hex_rgb(hex: &str) -> (u8, u8, u8) {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).expect("valid hex accent color");
    let g = u8::from_str_radix(&hex[2..4], 16).expect("valid hex accent color");
    let b = u8::from_str_radix(&hex[4..6], 16).expect("valid hex accent color");
    (r, g, b)
}

/// Picks black or white text, whichever contrasts better against `hex`,
/// using the standard WCAG relative-luminance perceptual weighting.
fn contrasting_fg(hex: &str) -> &'static str {
    let (r, g, b) = parse_hex_rgb(hex);
    let luminance = 0.299 * f64::from(r) + 0.587 * f64::from(g) + 0.114 * f64::from(b);
    if luminance > 140.0 {
        "#000000"
    } else {
        "#ffffff"
    }
}

thread_local! {
    /// The currently-installed accent CSS provider, so a later call can
    /// remove it before installing a new one (or before returning to
    /// `System`) instead of stacking providers on the display forever.
    static ACCENT_PROVIDER: RefCell<Option<gtk4::CssProvider>> = const { RefCell::new(None) };
}

/// Installs (or, for `System`, removes) the Veyra accent-color CSS
/// override on the default display. Safe to call repeatedly — each call
/// first tears down whatever provider a previous call installed.
///
/// No-op if there is no default display (e.g. headless test/CI
/// environment), matching the rest of Veyra's "never panic on environment
/// oddities" posture (Kural #15).
pub(crate) fn apply_accent_color(pref: AccentColorPref) {
    let Some(display) = gtk4::gdk::Display::default() else {
        return;
    };
    ACCENT_PROVIDER.with(|cell| {
        if let Some(old) = cell.borrow_mut().take() {
            gtk4::style_context_remove_provider_for_display(&display, &old);
        }
    });
    let Some(css) = pref.css() else {
        return;
    };
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(&css);
    gtk4::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    ACCENT_PROVIDER.with(|cell| *cell.borrow_mut() = Some(provider));
}

/// Faz 37: the persisted UI-language preference. `System` (the default)
/// means "resolve via `i18n::detect_system_locale()` at startup"; `En`/`Tr`
/// are an explicit override that always wins regardless of what the OS is
/// set to. Deliberately a distinct type from `i18n::Locale` (which has no
/// `System` variant) — see that module's doc comment for why.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub(crate) enum LanguagePref {
    #[default]
    System,
    En,
    Tr,
}

impl LanguagePref {
    pub(crate) const ALL: [LanguagePref; 3] =
        [LanguagePref::System, LanguagePref::En, LanguagePref::Tr];

    /// `None` for `System` (caller falls back to `i18n::detect_system_locale()`).
    pub(crate) fn to_locale(self) -> Option<crate::i18n::Locale> {
        match self {
            LanguagePref::System => None,
            LanguagePref::En => Some(crate::i18n::Locale::En),
            LanguagePref::Tr => Some(crate::i18n::Locale::Tr),
        }
    }

    /// The label a Preferences `ComboRow` shows for this choice — `System`
    /// is itself translated (it's chrome around the language picker, not a
    /// language name), while `En`/`Tr` show `Locale::label()`'s
    /// always-in-itself name so a user can find their language regardless
    /// of what's currently selected.
    pub(crate) fn label(self) -> &'static str {
        match self {
            LanguagePref::System => crate::i18n::t("prefs.common.system_default"),
            LanguagePref::En => crate::i18n::Locale::En.label(),
            LanguagePref::Tr => crate::i18n::Locale::Tr.label(),
        }
    }

    /// The effective locale this preference resolves to right now —
    /// `System` consults `i18n::detect_system_locale()` fresh each call
    /// rather than caching, so a changed `$LANG` between two Preferences-
    /// dialog opens would (if Veyra were restarted, which is when this
    /// actually gets re-read) reflect the new system language rather than
    /// a stale detection from process start.
    pub(crate) fn resolve(self) -> crate::i18n::Locale {
        self.to_locale()
            .unwrap_or_else(crate::i18n::detect_system_locale)
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
            IconSizePref::Small => crate::i18n::t("prefs.appearance.icon_size.small"),
            IconSizePref::Normal => crate::i18n::t("prefs.appearance.icon_size.normal"),
            IconSizePref::Large => crate::i18n::t("prefs.appearance.icon_size.large"),
            IconSizePref::ExtraLarge => crate::i18n::t("prefs.appearance.icon_size.extra_large"),
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

/// Faz 65: how a timestamp (modified/created/accessed) renders in the
/// Details view and item info panels — "5 minutes ago" vs the absolute
/// `21.08.2026 14:30` form. See `config::format_timestamp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub(crate) enum DateFormatPref {
    #[default]
    Relative,
    Absolute,
}

/// Faz 65: binary (1024-based, KiB/MiB/GiB) vs decimal (1000-based, KB/MB/GB)
/// size unit rendering. See `config::format_size`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub(crate) enum SizeUnitPref {
    #[default]
    Binary,
    Decimal,
}

thread_local! {
    /// The date-format and size-unit preferences every `format_size`/
    /// `format_timestamp` call in this module reads — same thread-local
    /// pattern as `i18n::CURRENT_LOCALE` (GTK is single-threaded, Rule #11),
    /// set once at startup and again on every Preferences "Appearance" page
    /// change.
    static CURRENT_DATE_FORMAT: std::cell::Cell<DateFormatPref> =
        const { std::cell::Cell::new(DateFormatPref::Relative) };
    static CURRENT_SIZE_UNIT: std::cell::Cell<SizeUnitPref> =
        const { std::cell::Cell::new(SizeUnitPref::Binary) };
}

pub(crate) fn set_date_format(pref: DateFormatPref) {
    CURRENT_DATE_FORMAT.with(|cell| cell.set(pref));
}

pub(crate) fn set_size_unit(pref: SizeUnitPref) {
    CURRENT_SIZE_UNIT.with(|cell| cell.set(pref));
}

const DECIMAL_SIZE_UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];

/// Renders `bytes` using the current `size_unit` preference: binary
/// (1024-based, `format_size`'s existing `KB`/`MB`/... ) or decimal
/// (1000-based, `kB`/`MB`/...).
pub(crate) fn format_size(bytes: u64) -> String {
    match CURRENT_SIZE_UNIT.with(std::cell::Cell::get) {
        SizeUnitPref::Binary => veyra_filesystem::format_size(bytes),
        SizeUnitPref::Decimal => {
            if bytes < 1000 {
                return format!("{bytes} B");
            }
            let mut value = bytes as f64;
            let mut unit_index = 0;
            while value >= 1000.0 && unit_index < DECIMAL_SIZE_UNITS.len() - 1 {
                value /= 1000.0;
                unit_index += 1;
            }
            format!("{value:.1} {}", DECIMAL_SIZE_UNITS[unit_index])
        }
    }
}

/// Renders `dt` using the current `date_format` preference: relative
/// ("5 minutes ago", "Yesterday") or absolute (`21.08.2026 14:30`).
pub(crate) fn format_timestamp(dt: chrono::DateTime<chrono::Utc>) -> String {
    match CURRENT_DATE_FORMAT.with(std::cell::Cell::get) {
        DateFormatPref::Absolute => dt.format("%d.%m.%Y %H:%M").to_string(),
        DateFormatPref::Relative => relative_time(dt),
    }
}

/// A small, dependency-free relative-time formatter (no new crate — matches
/// this module's "avoid new dependencies when stdlib/an existing dependency
/// already covers the need" constraint; `chrono` is already a dependency).
fn relative_time(dt: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let delta = now.signed_duration_since(dt);
    if delta.num_seconds() < 0 {
        return dt.format("%d.%m.%Y %H:%M").to_string();
    }
    if delta.num_seconds() < 60 {
        return crate::i18n::t("time.just_now").to_string();
    }
    if delta.num_minutes() < 60 {
        return crate::i18n::t_plural(
            "time.minutes_ago",
            delta.num_minutes(),
            &[("n", &delta.num_minutes().to_string())],
        );
    }
    if delta.num_hours() < 24 {
        return crate::i18n::t_plural(
            "time.hours_ago",
            delta.num_hours(),
            &[("n", &delta.num_hours().to_string())],
        );
    }
    if delta.num_days() == 1 {
        return crate::i18n::t("time.yesterday").to_string();
    }
    if delta.num_days() < 7 {
        return crate::i18n::t_plural(
            "time.days_ago",
            delta.num_days(),
            &[("n", &delta.num_days().to_string())],
        );
    }
    dt.format("%d.%m.%Y %H:%M").to_string()
}

/// Faz 65: where a brand-new (`Ctrl+T`) tab starts browsing. `CurrentFolder`
/// is the default — it's what `win.new-tab` already did before this
/// preference existed (Rule #4: never change existing default behavior when
/// merely exposing it as a setting).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub(crate) enum NewTabLocation {
    #[default]
    CurrentFolder,
    Home,
}

/// Faz 65: the default answer to a Copy/Move name-collision prompt.
/// `AlwaysAsk` keeps today's behavior (`conflict::Conflict` dialog on every
/// clash); the other three skip the dialog and apply that decision to every
/// clash in the batch, mirroring the dialog's own "apply to all" choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub(crate) enum ConflictDefaultAction {
    #[default]
    AlwaysAsk,
    AutoRename,
    Overwrite,
    Skip,
}

/// Faz 65: the compression effort passed to the archive writer — `Fast`
/// favors speed, `Maximum` favors ratio. Maps to `flate2`/`xz2`/`zip`/
/// `sevenz-rust`'s own 0-9 (or equivalent) level scales via `level()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub(crate) enum CompressionLevelPref {
    Fast,
    #[default]
    Balanced,
    Maximum,
}

impl CompressionLevelPref {
    pub(crate) const ALL: [CompressionLevelPref; 3] = [
        CompressionLevelPref::Fast,
        CompressionLevelPref::Balanced,
        CompressionLevelPref::Maximum,
    ];

    /// The 0-9-scale level every archive backend Veyra uses (`flate2`,
    /// `xz2`, `zip`, `sevenz-rust`) accepts.
    pub(crate) fn level(self) -> u32 {
        match self {
            CompressionLevelPref::Fast => 1,
            CompressionLevelPref::Balanced => 6,
            CompressionLevelPref::Maximum => 9,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            CompressionLevelPref::Fast => crate::i18n::t("prefs.files.compression_level.fast"),
            CompressionLevelPref::Balanced => {
                crate::i18n::t("prefs.files.compression_level.balanced")
            }
            CompressionLevelPref::Maximum => {
                crate::i18n::t("prefs.files.compression_level.maximum")
            }
        }
    }
}

/// Faz 65: which terminal emulator `terminal::open_terminal` launches.
/// `Auto` keeps Rule #25's existing priority chain (`xdg-terminal-exec` ->
/// `$TERMINAL` -> GIO default -> known-terminal fallback list) untouched;
/// every other variant pins one specific emulator (or, for `Custom`, an
/// arbitrary user-supplied command) ahead of that chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub(crate) enum TerminalPref {
    #[default]
    Auto,
    GnomeTerminal,
    Alacritty,
    Kitty,
    Ghostty,
    WezTerm,
    Custom,
}

impl TerminalPref {
    pub(crate) const ALL: [TerminalPref; 7] = [
        TerminalPref::Auto,
        TerminalPref::GnomeTerminal,
        TerminalPref::Alacritty,
        TerminalPref::Kitty,
        TerminalPref::Ghostty,
        TerminalPref::WezTerm,
        TerminalPref::Custom,
    ];

    /// The executable name this preference pins, or `None` for `Auto`
    /// (use the existing detection chain) and `Custom` (use the free-form
    /// `custom_terminal_command` string instead).
    pub(crate) fn executable(self) -> Option<&'static str> {
        match self {
            TerminalPref::Auto | TerminalPref::Custom => None,
            TerminalPref::GnomeTerminal => Some("gnome-terminal"),
            TerminalPref::Alacritty => Some("alacritty"),
            TerminalPref::Kitty => Some("kitty"),
            TerminalPref::Ghostty => Some("ghostty"),
            TerminalPref::WezTerm => Some("wezterm"),
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            TerminalPref::Auto => crate::i18n::t("prefs.common.auto_system"),
            TerminalPref::GnomeTerminal => "GNOME Terminal",
            TerminalPref::Alacritty => "Alacritty",
            TerminalPref::Kitty => "Kitty",
            TerminalPref::Ghostty => "Ghostty",
            TerminalPref::WezTerm => "WezTerm",
            TerminalPref::Custom => crate::i18n::t("prefs.advanced.terminal.custom"),
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
    pub accent_color: AccentColorPref,
    #[serde(default)]
    pub language: LanguagePref,
    #[serde(default)]
    pub icon_size: IconSizePref,
    /// Faz 65: relative ("5 minutes ago") vs absolute ("21.08.2026 14:30")
    /// timestamp rendering.
    #[serde(default)]
    pub date_format: DateFormatPref,
    /// Faz 65: binary (1024 KiB/MiB) vs decimal (1000 KB/MB) size units.
    #[serde(default)]
    pub size_unit: SizeUnitPref,

    // --- Navigation ---
    #[serde(default)]
    pub click_policy: ClickPolicy,
    #[serde(default)]
    pub open_folders_in_new_tab: bool,
    #[serde(default)]
    pub restore_tabs_on_startup: bool,
    /// Faz 65: `Dosya 2` sorts before `Dosya 10` when enabled (digit runs
    /// compared numerically); plain lexicographic order otherwise. See
    /// `crate::sorting::SortConfig::natural_sort`.
    #[serde(default = "default_true")]
    pub natural_sort: bool,
    /// Faz 65: where a brand-new (`Ctrl+T`) tab starts browsing.
    #[serde(default)]
    pub new_tab_location: NewTabLocation,

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
    /// Faz 65: the default answer to a Copy/Move name-collision prompt.
    #[serde(default)]
    pub default_conflict_action: ConflictDefaultAction,
    /// Faz 65: the format the Compress action's file-name field defaults
    /// to, and the format `win.compress-selection` uses without prompting.
    #[serde(default = "default_archive_format")]
    pub default_archive_format: veyra_filesystem::ArchiveFormat,
    /// Faz 65: the compression effort passed to whichever archive backend
    /// `default_archive_format` selects.
    #[serde(default)]
    pub compression_level: CompressionLevelPref,
    /// Faz 65: whether renaming/creating a name containing a Unicode
    /// bidi-override control character (RLO/LRO/etc — a homograph/extension-
    /// spoofing vector) shows `rename_dialog`'s blocking warning. Rule
    /// #21/#23-adjacent guard; this only lets a user who understands the
    /// risk turn the warning off, never the underlying `has_bidi_override`
    /// detection.
    #[serde(default = "default_true")]
    pub warn_bidi_spoofing: bool,

    // --- Search & Indexing ---
    #[serde(default = "default_true")]
    pub enable_fts_index: bool,
    #[serde(default = "default_max_search_results")]
    pub max_search_results: usize,
    /// Faz 65: how many directory levels `veyra_search::spawn_background_index`
    /// recurses before stopping.
    #[serde(default = "default_search_max_depth")]
    pub search_max_depth: usize,
    /// Faz 65: whether dotfiles/dot-directories are indexed (and therefore
    /// searchable) at all.
    #[serde(default)]
    pub search_include_hidden: bool,

    // --- Preview ---
    #[serde(default = "default_true")]
    pub enable_preview_panel: bool,
    #[serde(default = "default_max_preview_size_kb")]
    pub max_preview_size_kb: usize,
    #[serde(default = "default_true")]
    pub show_folder_count: bool,
    /// Faz 65: whether `Space` opens the Quick Look dialog at all.
    #[serde(default = "default_true")]
    pub enable_quick_look: bool,
    /// Faz 65: a `NNNN │ ` line-number gutter prefixed onto Quick Look's
    /// text card.
    #[serde(default)]
    pub quick_look_line_numbers: bool,
    /// Faz 65: whether Quick Look's audio/video card starts playing
    /// immediately instead of waiting for the user to press play.
    #[serde(default)]
    pub media_autoplay: bool,

    // --- Performance ---
    #[serde(default = "default_stream_chunk_size")]
    pub stream_chunk_size: usize,
    #[serde(default = "default_thumbnail_cache_capacity")]
    pub thumbnail_cache_capacity: usize,
    /// Faz 65: whether `veyra_filesystem::queue`'s Copy/Move loop tries the
    /// Faz 64 `FICLONE` reflink fast path before falling back to a byte
    /// copy. Disabling this always takes the byte-copy path, e.g. for a
    /// filesystem where reflink clones are known to behave oddly.
    #[serde(default = "default_true")]
    pub enable_reflink: bool,

    // --- Privacy ---
    #[serde(default)]
    pub sanitize_log_paths: bool,
    #[serde(default = "default_true")]
    pub store_recent_files: bool,
    /// Faz 53: whether a panic writes a local, redacted crash report to
    /// `$XDG_STATE_HOME/veyra/crashes/`. Never controls network
    /// transmission — Veyra has none (Kural #24) — only whether the report
    /// is captured at all.
    #[serde(default = "default_true")]
    pub save_crash_reports: bool,

    // --- Advanced ---
    /// Faz 39: reveals the right-click "Developer" submenu (path/URI
    /// copying, checksums, editor launch, metadata inspector) and the Git
    /// status headerbar badge.
    #[serde(default)]
    pub developer_mode: bool,
    /// Faz 65: whether a tab showing a Git working tree gets the headerbar
    /// status badge (independent of `developer_mode`, which only gates the
    /// Developer context-menu submenu).
    #[serde(default = "default_true")]
    pub show_git_badges: bool,
    /// Faz 65: which terminal emulator `terminal::resolve_terminal` prefers.
    #[serde(default)]
    pub terminal_pref: TerminalPref,
    /// Faz 65: the shell command `TerminalPref::Custom` launches. Ignored
    /// for every other `terminal_pref` variant.
    #[serde(default)]
    pub custom_terminal_command: String,
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

fn default_search_max_depth() -> usize {
    32
}

fn default_archive_format() -> veyra_filesystem::ArchiveFormat {
    veyra_filesystem::ArchiveFormat::Zip
}

impl Default for VeyraSettings {
    fn default() -> Self {
        VeyraSettings {
            color_scheme: ColorSchemePref::default(),
            accent_color: AccentColorPref::default(),
            language: LanguagePref::default(),
            icon_size: IconSizePref::default(),
            date_format: DateFormatPref::default(),
            size_unit: SizeUnitPref::default(),
            click_policy: ClickPolicy::default(),
            open_folders_in_new_tab: false,
            restore_tabs_on_startup: false,
            natural_sort: true,
            new_tab_location: NewTabLocation::default(),
            show_hidden: false,
            folders_first: true,
            default_view_mode: DefaultViewMode::default(),
            confirm_trash_empty: true,
            confirm_permanent_delete: true,
            default_conflict_action: ConflictDefaultAction::default(),
            default_archive_format: default_archive_format(),
            compression_level: CompressionLevelPref::default(),
            warn_bidi_spoofing: true,
            enable_fts_index: true,
            max_search_results: default_max_search_results(),
            search_max_depth: default_search_max_depth(),
            search_include_hidden: false,
            enable_preview_panel: true,
            max_preview_size_kb: default_max_preview_size_kb(),
            show_folder_count: true,
            enable_quick_look: true,
            quick_look_line_numbers: false,
            media_autoplay: false,
            stream_chunk_size: default_stream_chunk_size(),
            thumbnail_cache_capacity: default_thumbnail_cache_capacity(),
            enable_reflink: true,
            sanitize_log_paths: false,
            store_recent_files: true,
            save_crash_reports: true,
            developer_mode: false,
            show_git_badges: true,
            terminal_pref: TerminalPref::default(),
            custom_terminal_command: String::new(),
        }
    }
}

/// The allowed picker values for the Search / Preview / Performance pages'
/// combo rows (spec-mandated fixed choices rather than free-form numeric
/// entry).
pub(crate) const MAX_PREVIEW_SIZE_CHOICES_KB: [usize; 4] = [128, 256, 512, 1024];
pub(crate) const STREAM_CHUNK_SIZE_CHOICES: [usize; 3] = [250, 500, 1000];
pub(crate) const THUMBNAIL_CACHE_CAPACITY_CHOICES: [usize; 3] = [500, 1000, 2000];
pub(crate) const SEARCH_MAX_DEPTH_CHOICES: [usize; 4] = [8, 16, 32, 64];
pub(crate) const ARCHIVE_FORMAT_CHOICES: [veyra_filesystem::ArchiveFormat; 4] = [
    veyra_filesystem::ArchiveFormat::Zip,
    veyra_filesystem::ArchiveFormat::TarGz,
    veyra_filesystem::ArchiveFormat::TarXz,
    veyra_filesystem::ArchiveFormat::SevenZip,
];

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
    fn accent_color_hex_values_match_spec() {
        assert_eq!(AccentColorPref::System.hex(), None);
        assert_eq!(AccentColorPref::Blue.hex(), Some("#3584e4"));
        assert_eq!(AccentColorPref::Teal.hex(), Some("#2190a4"));
        assert_eq!(AccentColorPref::Green.hex(), Some("#3a944c"));
        assert_eq!(AccentColorPref::Yellow.hex(), Some("#cd9309"));
        assert_eq!(AccentColorPref::Orange.hex(), Some("#e66100"));
        assert_eq!(AccentColorPref::Red.hex(), Some("#c01c28"));
        assert_eq!(AccentColorPref::Purple.hex(), Some("#9141ac"));
        assert_eq!(AccentColorPref::Pink.hex(), Some("#d56199"));
        assert_eq!(AccentColorPref::Slate.hex(), Some("#5e5c64"));
    }

    #[test]
    fn accent_color_all_excludes_system_and_matches_hex_count() {
        assert_eq!(AccentColorPref::ALL.len(), 9);
        assert!(!AccentColorPref::ALL.contains(&AccentColorPref::System));
        for pref in AccentColorPref::ALL {
            assert!(pref.hex().is_some());
        }
    }

    #[test]
    fn accent_color_default_is_system() {
        assert_eq!(AccentColorPref::default(), AccentColorPref::System);
    }

    #[test]
    fn accent_color_system_has_no_css() {
        assert_eq!(AccentColorPref::System.css(), None);
    }

    #[test]
    fn accent_color_css_defines_all_three_variables() {
        let css = AccentColorPref::Blue.css().unwrap();
        assert!(css.contains("@define-color accent_color #3584e4;"));
        assert!(css.contains("@define-color accent_bg_color #3584e4;"));
        assert!(css.contains("@define-color accent_fg_color #ffffff;"));
    }

    #[test]
    fn contrasting_fg_picks_black_on_bright_yellow() {
        assert_eq!(contrasting_fg("#cd9309"), "#000000");
    }

    #[test]
    fn contrasting_fg_picks_white_on_dark_slate() {
        assert_eq!(contrasting_fg("#5e5c64"), "#ffffff");
    }

    #[test]
    fn accent_color_round_trips_through_json() {
        for pref in [AccentColorPref::System]
            .into_iter()
            .chain(AccentColorPref::ALL)
        {
            let json = serde_json::to_string(&pref).unwrap();
            let restored: AccentColorPref = serde_json::from_str(&json).unwrap();
            assert_eq!(restored, pref);
        }
    }

    #[test]
    fn settings_with_accent_color_round_trip_through_json() {
        let settings = VeyraSettings {
            accent_color: AccentColorPref::Purple,
            ..VeyraSettings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        let restored: VeyraSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, settings);
    }

    #[test]
    fn load_from_missing_accent_color_field_defaults_to_system() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, br#"{"show_hidden": true}"#).unwrap();
        let loaded = load_from(&path);
        assert_eq!(loaded.accent_color, AccentColorPref::System);
    }

    #[test]
    fn icon_size_pixels_match_spec() {
        assert_eq!(IconSizePref::Small.pixels(), 48);
        assert_eq!(IconSizePref::Normal.pixels(), 64);
        assert_eq!(IconSizePref::Large.pixels(), 96);
        assert_eq!(IconSizePref::ExtraLarge.pixels(), 128);
    }

    // --- Faz 65: new preferences round-trip and default correctly ---

    #[test]
    fn faz65_defaults_match_spec() {
        let settings = VeyraSettings::default();
        assert_eq!(settings.date_format, DateFormatPref::Relative);
        assert_eq!(settings.size_unit, SizeUnitPref::Binary);
        assert!(settings.natural_sort);
        assert_eq!(settings.new_tab_location, NewTabLocation::CurrentFolder);
        assert_eq!(
            settings.default_conflict_action,
            ConflictDefaultAction::AlwaysAsk
        );
        assert_eq!(
            settings.default_archive_format,
            veyra_filesystem::ArchiveFormat::Zip
        );
        assert_eq!(settings.compression_level, CompressionLevelPref::Balanced);
        assert!(settings.warn_bidi_spoofing);
        assert_eq!(settings.search_max_depth, 32);
        assert!(!settings.search_include_hidden);
        assert!(settings.enable_quick_look);
        assert!(!settings.quick_look_line_numbers);
        assert!(!settings.media_autoplay);
        assert!(settings.enable_reflink);
        assert!(settings.show_git_badges);
        assert_eq!(settings.terminal_pref, TerminalPref::Auto);
        assert_eq!(settings.custom_terminal_command, "");
    }

    #[test]
    fn faz65_settings_round_trip_through_json() {
        let settings = VeyraSettings {
            date_format: DateFormatPref::Absolute,
            size_unit: SizeUnitPref::Decimal,
            natural_sort: false,
            new_tab_location: NewTabLocation::Home,
            default_conflict_action: ConflictDefaultAction::AutoRename,
            default_archive_format: veyra_filesystem::ArchiveFormat::TarXz,
            compression_level: CompressionLevelPref::Maximum,
            warn_bidi_spoofing: false,
            search_max_depth: 8,
            search_include_hidden: true,
            enable_quick_look: false,
            quick_look_line_numbers: true,
            media_autoplay: true,
            enable_reflink: false,
            show_git_badges: false,
            terminal_pref: TerminalPref::Kitty,
            custom_terminal_command: "my-term -e".to_string(),
            ..VeyraSettings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        let restored: VeyraSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, settings);
    }

    #[test]
    fn faz65_missing_fields_fall_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, br#"{"date_format": "Relative"}"#).unwrap();
        let loaded = load_from(&path);
        assert_eq!(loaded, VeyraSettings::default());
    }

    #[test]
    fn compression_level_maps_to_spec_levels() {
        assert_eq!(CompressionLevelPref::Fast.level(), 1);
        assert_eq!(CompressionLevelPref::Balanced.level(), 6);
        assert_eq!(CompressionLevelPref::Maximum.level(), 9);
    }

    #[test]
    fn terminal_pref_executables_match_spec() {
        assert_eq!(TerminalPref::Auto.executable(), None);
        assert_eq!(
            TerminalPref::GnomeTerminal.executable(),
            Some("gnome-terminal")
        );
        assert_eq!(TerminalPref::Alacritty.executable(), Some("alacritty"));
        assert_eq!(TerminalPref::Kitty.executable(), Some("kitty"));
        assert_eq!(TerminalPref::Ghostty.executable(), Some("ghostty"));
        assert_eq!(TerminalPref::WezTerm.executable(), Some("wezterm"));
        assert_eq!(TerminalPref::Custom.executable(), None);
    }
}
