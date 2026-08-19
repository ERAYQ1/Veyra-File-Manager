//! Faz 37: Veyra's own minimal, dependency-free localization engine (no
//! `gettext`/`fluent` — `glib` for system-locale detection is already a
//! dependency of `veyra-ui`, and a compile-time-checked key/value table
//! covers everything a desktop file manager's UI needs, so pulling in a
//! whole message-catalog crate isn't warranted, per the "avoid new
//! dependencies when an existing one already covers the need" rule).
//! Exposed as plain `t`/`t_fmt`/`t_plural` functions rather than a `t!`
//! macro: Rust lets a macro and function share one name (separate
//! namespaces), but a same-crate `pub(crate) use` re-export of such a macro
//! collides with the function of the same name, and `crate::i18n::t(key)`
//! already reads just as cleanly at every call site.
//!
//! `Locale` is the *runtime* language Veyra is currently rendering in — a
//! concrete choice, never "system". `crate::config::LanguagePref` is the
//! *persisted preference* (`System`/`En`/`Tr`), which resolves down to a
//! concrete `Locale` via `detect_system_locale()` when it's `System`. Kept
//! as two separate types because "the user asked for the system locale" and
//! "the system locale happens to be Turkish" are different facts — losing
//! that distinction would make a changed system language impossible to
//! detect without also touching `settings.json`.
//!
//! Adding a new language (the spec's `de`/`fr`/`es`/`ru`/`ar`/`zh`/`ja`
//! roadmap) is: add a variant to `Locale`, add its own `const TABLE: &[...]`
//! array below (mirroring `EN`/`TR`), wire both into `Locale::table` and
//! `Locale::code`/`Locale::label`, and add it to `Locale::ALL`. The
//! completeness test then fails loudly on any key the new table is missing
//! rather than silently falling back at runtime.

use std::cell::Cell;

/// A concrete language Veyra can render its UI in. `System` is *not* a
/// variant here — see the module doc comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Locale {
    En,
    Tr,
}

impl Locale {
    pub(crate) const ALL: [Locale; 2] = [Locale::En, Locale::Tr];

    /// This locale's own table, used first; `t`/`t_fmt`/`t_plural` fall back
    /// to `EN` for any key this table doesn't have (Kural #15 — a missing
    /// translation must never panic or blank out the UI).
    fn table(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Locale::En => EN,
            Locale::Tr => TR,
        }
    }

    /// BCP-47-ish language code, used for `glib::language_names()` prefix
    /// matching in `detect_system_locale` and as the persisted
    /// `settings.json` value behind `LanguagePref`.
    pub(crate) fn code(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::Tr => "tr",
        }
    }

    /// The name of this language written in itself, e.g. `"English (US)"`,
    /// `"Türkçe (TR)"` — a language picker always shows every choice this
    /// way (a Turkish speaker shouldn't need to already read English to find
    /// "Türkçe"), so this deliberately isn't itself translated per-locale.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Locale::En => "English (US)",
            Locale::Tr => "Türkçe (TR)",
        }
    }
}

thread_local! {
    /// The locale every `t`/`t_fmt`/`t_plural` call reads. GTK is
    /// single-threaded (Rule #11), so a thread-local `Cell` set once at
    /// startup (`lib.rs`) and on every Preferences "Language" change is
    /// enough — no `Arc`/`Mutex` needed, matching `config::ACCENT_PROVIDER`'s
    /// existing thread-local pattern for other live-appliable preferences.
    static CURRENT_LOCALE: Cell<Locale> = const { Cell::new(Locale::En) };
}

/// Sets the locale every subsequent `t`/`t_fmt`/`t_plural` call in this
/// process uses. Call once at startup with the resolved (non-`System`)
/// locale, and again whenever the user changes the Preferences "Language"
/// row.
pub(crate) fn set_locale(locale: Locale) {
    CURRENT_LOCALE.with(|cell| cell.set(locale));
}

pub(crate) fn current_locale() -> Locale {
    CURRENT_LOCALE.with(|cell| cell.get())
}

/// Reads the user's system language preference the way `glib` itself
/// resolves it (`LANGUAGE`/`LC_ALL`/`LC_MESSAGES`/`LANG`, most-specific
/// first — see `g_get_language_names`), returning the first entry whose
/// two-letter prefix matches a language Veyra ships. Falls back to `En`
/// when nothing matches (including in a `C`/`POSIX` locale, or headless
/// test environments with no locale configured at all) — never panics.
pub(crate) fn detect_system_locale() -> Locale {
    for name in gtk4::glib::language_names() {
        let prefix = name.split(['_', '.', '@']).next().unwrap_or("");
        if let Some(locale) = Locale::ALL.into_iter().find(|l| l.code() == prefix) {
            return locale;
        }
    }
    Locale::En
}

fn lookup(locale: Locale, key: &str) -> Option<&'static str> {
    locale
        .table()
        .iter()
        .find(|(candidate, _)| *candidate == key)
        .map(|(_, value)| *value)
}

/// Translates `key` in the current locale. Falls back to `En`, then to the
/// raw key itself, if a table is missing an entry — a missing translation
/// is never fatal (Kural #15), it's just visibly wrong (the literal key
/// string), which is easy to spot and fix.
pub(crate) fn t(key: &'static str) -> &'static str {
    lookup(current_locale(), key)
        .or_else(|| lookup(Locale::En, key))
        .unwrap_or(key)
}

/// `t(key)` with `{name}` placeholders in the translated string replaced by
/// `params`' matching values, e.g. `t_fmt("status.free_space", &[("size",
/// "12.3 GB")])` -> `"12.3 GB free"` / `"12.3 GB boş"`.
pub(crate) fn t_fmt(key: &'static str, params: &[(&str, &str)]) -> String {
    apply_params(t(key), params)
}

fn apply_params(template: &str, params: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (name, value) in params {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

/// CLDR-style plural category for `n` in `locale`. Both shipped locales use
/// the same two-category (`one`/`other`) rule today, but this is where a
/// future language needing more categories (Arabic has six) would branch —
/// kept as its own function rather than inlined into `t_plural` so adding
/// one is a single new match arm, not a rewrite.
fn plural_category(_locale: Locale, n: i64) -> &'static str {
    if n == 1 {
        "one"
    } else {
        "other"
    }
}

/// Plural-aware translation: looks up `"{key}.one"` or `"{key}.other"`
/// (picked via `plural_category`) rather than `key` itself, then applies
/// `params` the same way `t_fmt` does. Callers conventionally also pass the
/// count itself as a `"count"` param, e.g. `t_plural("status.items_count",
/// n, &[("count", &n.to_string())])` -> `"1 item"` / `"5 items"`.
pub(crate) fn t_plural(key: &str, n: i64, params: &[(&str, &str)]) -> String {
    let category = plural_category(current_locale(), n);
    let full_key = format!("{key}.{category}");
    let template = lookup(current_locale(), &full_key)
        .or_else(|| lookup(Locale::En, &full_key))
        .unwrap_or(key);
    apply_params(template, params)
}

// ---------------------------------------------------------------------
// Catalog
//
// One flat key/value table per language. Keys are dotted
// `<area>.<element>[.<field>]`, e.g. `prefs.appearance.theme.title`, so a
// grep for `prefs.appearance.` finds every string on that one Preferences
// page. Plural keys are two entries, `<key>.one` and `<key>.other` (see
// `t_plural`), never a bare `<key>`.
// ---------------------------------------------------------------------

const EN: &[(&str, &str)] = &[
    // --- Navigation (panel toolbar buttons, split_view.rs) ---
    ("nav.back", "Go Back"),
    ("nav.back.tooltip", "Go Back (Alt+Left)"),
    ("nav.forward", "Go Forward"),
    ("nav.forward.tooltip", "Go Forward (Alt+Right)"),
    ("nav.up", "Go Up"),
    ("nav.up.tooltip", "Go Up (Alt+Up)"),
    ("nav.home", "Go Home"),
    ("nav.refresh", "Refresh"),
    ("nav.refresh.tooltip", "Refresh (F5)"),
    (
        "nav.address.tooltip",
        "Enter Location (Enter to go, Esc to cancel)",
    ),
    ("nav.address.accessible_label", "Address"),
    // --- Header bar (headerbar.rs) ---
    ("headerbar.search.tooltip", "Search Focused Panel (Ctrl+F)"),
    ("headerbar.search.accessible_label", "Search Directory"),
    ("headerbar.split.tooltip", "Toggle Split View (F3)"),
    ("headerbar.split.accessible_label", "Toggle Split View"),
    ("headerbar.preview.tooltip", "Toggle Preview (F9)"),
    ("headerbar.preview.accessible_label", "Toggle Preview"),
    ("view.icon", "Icon View"),
    ("view.compact", "Compact View"),
    ("view.details", "Details View"),
    ("sort.button.tooltip", "Sort & Filter"),
    ("sort.button.accessible_label", "Sort and Filter Options"),
    ("sort.section.sort_by", "Sort By"),
    ("sort.section.direction", "Direction"),
    ("sort.ascending", "Ascending"),
    ("sort.descending", "Descending"),
    ("sort.folders_first", "Folders First"),
    ("sort.section.filter_by", "Filter By"),
    // --- Context menu (context_menu.rs) ---
    ("menu.open", "Open"),
    ("menu.open_with", "Open With"),
    ("menu.open_with_other", "Other Application…"),
    ("menu.open_in_new_tab", "Open in New Tab"),
    ("menu.open_in_new_window", "Open in New Window"),
    ("menu.add_to_bookmarks", "Add to Bookmarks"),
    ("menu.analyze_disk", "Analyze Disk Usage…"),
    ("menu.copy.one", "Copy"),
    ("menu.copy.other", "Copy ({count} items)"),
    ("menu.cut.one", "Cut"),
    ("menu.cut.other", "Cut ({count} items)"),
    ("menu.copy_to_other_panel.one", "Copy to Other Panel"),
    (
        "menu.copy_to_other_panel.other",
        "Copy to Other Panel ({count} items)",
    ),
    ("menu.move_to_other_panel.one", "Move to Other Panel"),
    (
        "menu.move_to_other_panel.other",
        "Move to Other Panel ({count} items)",
    ),
    ("menu.rename", "Rename"),
    ("menu.move_to_trash.one", "Move to Trash"),
    ("menu.move_to_trash.other", "Move {count} items to Trash"),
    ("menu.delete_permanently.one", "Delete Permanently"),
    (
        "menu.delete_permanently.other",
        "Delete {count} items Permanently",
    ),
    ("menu.compress.one", "Compress…"),
    ("menu.compress.other", "Compress {count} items…"),
    ("menu.extract_here", "Extract Here"),
    ("menu.extract_to", "Extract to…"),
    ("menu.open_terminal_here", "Open Terminal Here"),
    ("menu.open_terminal_as_root", "Open in Terminal as Root"),
    ("menu.copy_path", "Copy Path"),
    ("menu.copy_location", "Copy Location"),
    // --- Developer Mode submenu (Faz 39, context_menu.rs) ---
    ("menu.developer", "Developer"),
    ("menu.copy_absolute_path", "Copy Absolute Path"),
    ("menu.copy_uri", "Copy URI"),
    ("menu.copy_relative_path", "Copy Relative Path"),
    ("menu.open_in_editor", "Open in Editor"),
    ("menu.calculate_checksums", "Calculate Checksums…"),
    ("menu.developer_metadata", "Developer Metadata Inspector"),
    ("menu.properties", "Properties"),
    ("menu.new_folder", "New Folder"),
    ("menu.new_document", "New Document"),
    ("menu.paste", "Paste"),
    ("menu.restore", "Restore"),
    ("menu.empty_trash", "Empty Trash"),
    // --- Operation toasts (widgets/progress_toast.rs) ---
    ("toast.copying", "Copying"),
    ("toast.moving", "Moving"),
    ("toast.moving_to_trash", "Moving to Trash"),
    ("toast.deleting", "Deleting"),
    ("toast.compressing", "Compressing"),
    ("toast.extracting", "Extracting"),
    ("toast.pause", "Pause"),
    ("toast.resume", "Resume"),
    ("toast.cancel", "Cancel"),
    // --- Status bar / item counts (window.rs) ---
    ("status.items_count.one", "1 item"),
    ("status.items_count.other", "{count} items"),
    ("status.free_space", "{size} free"),
    ("status.loading", "Loading…"),
    ("status.cancelled", "Cancelled"),
    ("status.archive_failed", "Archive failed"),
    ("panel.files_page_title", "Files"),
    ("panel.sidebar_page_title", "Sidebar"),
    // --- Preferences: Language row (dialogs/preferences_dialog.rs) ---
    ("prefs.dialog.title", "Preferences"),
    ("prefs.language.title", "Language"),
    (
        "prefs.language.subtitle",
        "Applies the next time Veyra starts",
    ),
    ("prefs.common.system_default", "System Default"),
    // --- Preferences: page titles ---
    ("prefs.page.appearance", "Appearance"),
    ("prefs.page.navigation", "Navigation"),
    ("prefs.page.files", "Files"),
    ("prefs.page.search", "Search"),
    ("prefs.page.preview", "Preview"),
    ("prefs.page.performance", "Performance"),
    ("prefs.page.shortcuts", "Shortcuts"),
    ("prefs.page.privacy", "Privacy"),
    ("prefs.page.advanced", "Advanced"),
    // --- Preferences: Appearance ---
    ("prefs.appearance.group.theme_icons", "Theme & Icons"),
    ("prefs.appearance.theme.title", "Theme"),
    ("prefs.appearance.theme.light", "Light"),
    ("prefs.appearance.theme.dark", "Dark"),
    ("prefs.appearance.accent_color.title", "Accent Color"),
    ("prefs.appearance.accent_color.blue", "Blue"),
    ("prefs.appearance.accent_color.teal", "Teal"),
    ("prefs.appearance.accent_color.green", "Green"),
    ("prefs.appearance.accent_color.yellow", "Yellow"),
    ("prefs.appearance.accent_color.orange", "Orange"),
    ("prefs.appearance.accent_color.red", "Red"),
    ("prefs.appearance.accent_color.purple", "Purple"),
    ("prefs.appearance.accent_color.pink", "Pink"),
    ("prefs.appearance.accent_color.slate", "Slate"),
    ("prefs.appearance.icon_size.title", "Icon Size"),
    ("prefs.appearance.icon_size.small", "Small (48px)"),
    ("prefs.appearance.icon_size.normal", "Medium (64px)"),
    ("prefs.appearance.icon_size.large", "Large (96px)"),
    (
        "prefs.appearance.icon_size.extra_large",
        "Extra Large (128px)",
    ),
    // --- Preferences: Navigation ---
    ("prefs.navigation.group.opening_items", "Opening Items"),
    ("prefs.navigation.click_policy.title", "Click Policy"),
    (
        "prefs.navigation.click_policy.double",
        "Double-Click to Open",
    ),
    (
        "prefs.navigation.click_policy.single",
        "Single-Click to Open",
    ),
    (
        "prefs.navigation.open_new_tab.title",
        "Open Folders in New Tab",
    ),
    (
        "prefs.navigation.open_new_tab.subtitle",
        "Middle-click and sidebar navigation already do this; applies here too on your next launch",
    ),
    ("prefs.navigation.group.tabs", "Tabs"),
    (
        "prefs.navigation.restore_tabs.title",
        "Restore Previous Tabs on Startup",
    ),
    (
        "prefs.navigation.restore_tabs.subtitle",
        "Reopen every tab that was open when Veyra last closed",
    ),
    // --- Preferences: Files ---
    ("prefs.files.group.display_defaults", "Display Defaults"),
    (
        "prefs.files.group.display_defaults.description",
        "Applies to newly opened tabs; open tabs keep their own setting.",
    ),
    ("prefs.files.show_hidden.title", "Show Hidden Files"),
    (
        "prefs.files.show_hidden.subtitle",
        "New tabs start with hidden files visible",
    ),
    ("prefs.files.folders_first.title", "Folders First"),
    (
        "prefs.files.folders_first.subtitle",
        "New tabs list folders before files",
    ),
    ("prefs.files.default_view_mode.title", "Default View Mode"),
    ("prefs.files.default_view_mode.icons", "Icons"),
    ("prefs.files.default_view_mode.compact", "Compact"),
    ("prefs.files.default_view_mode.details", "Details"),
    ("prefs.files.group.confirmations", "Confirmations"),
    (
        "prefs.files.confirm_trash.title",
        "Confirm Before Emptying Trash",
    ),
    (
        "prefs.files.confirm_delete.title",
        "Confirm Before Permanently Deleting",
    ),
    // --- Preferences: Search ---
    ("prefs.search.group.indexing", "Indexing"),
    ("prefs.search.enable.title", "Enable Fast Search Indexer"),
    (
        "prefs.search.enable.subtitle",
        "SQLite + FTS5 index backing name:/type:/size:/modified: searches",
    ),
    ("prefs.search.max_results.title", "Max Search Results"),
    ("prefs.search.rebuild.title", "Rebuild Search Index"),
    (
        "prefs.search.rebuild.subtitle",
        "Re-scans your home directory from scratch",
    ),
    ("prefs.search.rebuild.button", "Rebuild"),
    // --- Preferences: Preview ---
    ("prefs.preview.group.panel", "Preview Panel"),
    ("prefs.preview.enable.title", "Enable Preview Panel"),
    ("prefs.preview.size_limit.title", "Preview Size Limit"),
    ("prefs.preview.folder_count.title", "Show Folder Item Count"),
    // --- Preferences: Performance ---
    (
        "prefs.performance.group.directory_loading",
        "Directory Loading",
    ),
    (
        "prefs.performance.chunk_size.title",
        "Directory Stream Chunk Size",
    ),
    ("prefs.performance.chunk_size.unit.one", "{n} file"),
    ("prefs.performance.chunk_size.unit.other", "{n} files"),
    (
        "prefs.performance.cache_capacity.title",
        "Thumbnail Cache Capacity",
    ),
    ("prefs.performance.cache_capacity.unit.one", "{n} thumbnail"),
    (
        "prefs.performance.cache_capacity.unit.other",
        "{n} thumbnails",
    ),
    // --- Preferences: Shortcuts ---
    (
        "prefs.shortcuts.group.keyboard_shortcuts",
        "Keyboard Shortcuts",
    ),
    ("prefs.shortcuts.view_all.title", "View All Shortcuts"),
    (
        "prefs.shortcuts.view_all.subtitle",
        "Every action and its current key binding",
    ),
    ("prefs.shortcuts.view_all.button", "View"),
    ("prefs.shortcuts.reset.title", "Reset Shortcuts to Default"),
    (
        "prefs.shortcuts.reset.subtitle",
        "Discards any customization in shortcuts.json",
    ),
    ("prefs.shortcuts.reset.button", "Reset"),
    // --- Preferences: Privacy ---
    ("prefs.privacy.group.history", "History"),
    (
        "prefs.privacy.clear_files.title",
        "Clear Recent Files History",
    ),
    (
        "prefs.privacy.clear_files.subtitle",
        "Empties the XDG recently-used registry",
    ),
    ("prefs.privacy.clear_servers.title", "Clear Recent Servers"),
    (
        "prefs.privacy.clear_servers.subtitle",
        "Empties the \"Connect to Server\" address history",
    ),
    ("prefs.privacy.clear.button", "Clear"),
    (
        "prefs.privacy.remember_files.title",
        "Remember Recently Used Files",
    ),
    (
        "prefs.privacy.remember_files.subtitle",
        "Off also disables the Recent Files history above",
    ),
    ("prefs.privacy.group.logging", "Logging"),
    (
        "prefs.privacy.sanitize_logs.title",
        "Sanitize File Paths in Logs",
    ),
    (
        "prefs.privacy.sanitize_logs.subtitle",
        "Log only file names instead of full paths",
    ),
    ("prefs.privacy.group.telemetry", "Telemetry"),
    ("prefs.privacy.telemetry.title", "Zero Telemetry"),
    (
        "prefs.privacy.telemetry.subtitle",
        "Veyra never sends usage data or file contents anywhere (Kural #24)",
    ),
    // --- Preferences: Advanced ---
    ("prefs.advanced.group.reset", "Reset"),
    (
        "prefs.advanced.reset_all.title",
        "Reset All Settings to Default",
    ),
    (
        "prefs.advanced.reset_all.subtitle",
        "Restores every page on this dialog to its default value",
    ),
    ("prefs.advanced.reset_all.button", "Reset All"),
    (
        "prefs.advanced.reset_all.confirm_heading",
        "Reset all settings to default?",
    ),
    (
        "prefs.advanced.reset_all.confirm_body",
        "Every Preferences page reverts to its default value. This cannot be undone.",
    ),
    ("prefs.advanced.reset_all.cancel", "Cancel"),
    ("prefs.advanced.group.developer", "Developer"),
    ("prefs.advanced.developer_mode.title", "Developer Mode"),
    (
        "prefs.advanced.developer_mode.subtitle",
        "Adds path/URI copying, checksums, editor launch, and a Git status badge",
    ),
    // --- Developer Mode dialogs (Faz 39) ---
    ("dev.checksum.title", "Checksums"),
    ("dev.checksum.close", "Close"),
    ("dev.checksum.computing", "Computing…"),
    ("dev.checksum.copy", "Copy"),
    // --- Checksums & Verification (Faz 41) ---
    ("dev.checksum.calculate", "Calculate Checksums"),
    (
        "dev.checksum.verify_placeholder",
        "Paste expected checksum to verify…",
    ),
    ("dev.checksum.matches", "✓ Matches {algorithm}"),
    ("dev.checksum.mismatch", "✗ Checksum mismatch"),
    ("dev.metadata.title", "Developer Metadata"),
    ("dev.metadata.close", "Close"),
    ("dev.metadata.inode", "Inode"),
    ("dev.metadata.device_id", "Device ID"),
    ("dev.metadata.permissions", "Permissions"),
    ("dev.metadata.mime_type", "MIME Type"),
    ("dev.metadata.hard_links", "Hard Links"),
    ("dev.metadata.unknown", "Unknown"),
    // --- Duplicate Files (Faz 42) ---
    ("dup.tab.title", "Duplicate Files"),
    ("dup.loading", "Comparing file contents…"),
    ("dup.empty", "No duplicate files found."),
    ("dup.summary.groups.one", "{count} duplicate group"),
    ("dup.summary.groups.other", "{count} duplicate groups"),
    ("dup.summary.files.one", "{count} file"),
    ("dup.summary.files.other", "{count} files"),
    ("dup.summary.wasted", "{size} wasted"),
    ("dup.select_keep_newest", "Select All Copies (Keep Newest)"),
    ("dup.select_keep_oldest", "Select All Copies (Keep Oldest)"),
    ("dup.clear_selection", "Clear Selection"),
    ("dup.reveal_in_folder", "Reveal in Folder"),
    ("dup.group.copies.one", "{count} copy • {size} each"),
    ("dup.group.copies.other", "{count} copies • {size} each"),
    ("dup.group.total", "{size} total"),
    ("dup.selection_status.one", "{count} file selected • {size} to free"),
    ("dup.selection_status.other", "{count} files selected • {size} to free"),
    ("dup.selection_status.empty", "No files selected"),
    ("dup.move_to_trash", "Move Selected to Trash"),
    ("dup.confirm.title.one", "Move {count} File to Trash?"),
    ("dup.confirm.title.other", "Move {count} Files to Trash?"),
    (
        "dup.confirm.body",
        "{count} files ({size}) will be moved to Trash. You can undo this with Ctrl+Z.",
    ),
    ("dup.confirm.cancel", "Cancel"),
    ("dup.confirm.confirm", "Move to Trash"),
    ("dup.block_all_copies.title", "Can't Delete Every Copy"),
    (
        "dup.block_all_copies.body",
        "At least one copy must be kept in every duplicate group. Unselect at least one file in: {names}",
    ),
    ("dup.block_all_copies.close", "OK"),
    ("dup.trash_error.title", "Some Files Couldn't Be Moved"),
];

const TR: &[(&str, &str)] = &[
    // --- Navigation (panel toolbar buttons, split_view.rs) ---
    ("nav.back", "Geri"),
    ("nav.back.tooltip", "Geri (Alt+Sol)"),
    ("nav.forward", "İleri"),
    ("nav.forward.tooltip", "İleri (Alt+Sağ)"),
    ("nav.up", "Yukarı"),
    ("nav.up.tooltip", "Yukarı (Alt+Yukarı)"),
    ("nav.home", "Ana Dizin"),
    ("nav.refresh", "Yenile"),
    ("nav.refresh.tooltip", "Yenile (F5)"),
    ("nav.address.tooltip", "Konum Girin (Git için Enter, İptal için Esc)"),
    ("nav.address.accessible_label", "Adres"),
    // --- Header bar (headerbar.rs) ---
    ("headerbar.search.tooltip", "Odaktaki Paneli Ara (Ctrl+F)"),
    ("headerbar.search.accessible_label", "Dizinde Ara"),
    ("headerbar.split.tooltip", "Bölünmüş Görünümü Aç/Kapat (F3)"),
    ("headerbar.split.accessible_label", "Bölünmüş Görünümü Aç/Kapat"),
    ("headerbar.preview.tooltip", "Önizlemeyi Aç/Kapat (F9)"),
    ("headerbar.preview.accessible_label", "Önizlemeyi Aç/Kapat"),
    ("view.icon", "Simge Görünümü"),
    ("view.compact", "Kompakt Görünüm"),
    ("view.details", "Ayrıntılar Görünümü"),
    ("sort.button.tooltip", "Sırala ve Süz"),
    ("sort.button.accessible_label", "Sıralama ve Süzme Seçenekleri"),
    ("sort.section.sort_by", "Sırala"),
    ("sort.section.direction", "Yön"),
    ("sort.ascending", "Artan"),
    ("sort.descending", "Azalan"),
    ("sort.folders_first", "Önce Klasörler"),
    ("sort.section.filter_by", "Süz"),
    // --- Context menu (context_menu.rs) ---
    ("menu.open", "Aç"),
    ("menu.open_with", "Birlikte Aç"),
    ("menu.open_with_other", "Başka Uygulama…"),
    ("menu.open_in_new_tab", "Yeni Sekmede Aç"),
    ("menu.open_in_new_window", "Yeni Pencerede Aç"),
    ("menu.add_to_bookmarks", "Yer İmlerine Ekle"),
    ("menu.analyze_disk", "Disk Kullanımını Analiz Et…"),
    ("menu.copy.one", "Kopyala"),
    ("menu.copy.other", "Kopyala ({count} öge)"),
    ("menu.cut.one", "Kes"),
    ("menu.cut.other", "Kes ({count} öge)"),
    ("menu.copy_to_other_panel.one", "Diğer Panele Kopyala"),
    ("menu.copy_to_other_panel.other", "Diğer Panele Kopyala ({count} öge)"),
    ("menu.move_to_other_panel.one", "Diğer Panele Taşı"),
    ("menu.move_to_other_panel.other", "Diğer Panele Taşı ({count} öge)"),
    ("menu.rename", "Yeniden Adlandır"),
    ("menu.move_to_trash.one", "Çöpe At"),
    ("menu.move_to_trash.other", "{count} ögeyi Çöpe At"),
    ("menu.delete_permanently.one", "Kalıcı Olarak Sil"),
    ("menu.delete_permanently.other", "{count} ögeyi Kalıcı Olarak Sil"),
    ("menu.compress.one", "Sıkıştır…"),
    ("menu.compress.other", "{count} ögeyi Sıkıştır…"),
    ("menu.extract_here", "Buraya Ayıkla"),
    ("menu.extract_to", "Şuraya Ayıkla…"),
    ("menu.open_terminal_here", "Burada Terminal Aç"),
    ("menu.open_terminal_as_root", "Yönetici Olarak Terminalde Aç"),
    ("menu.copy_path", "Yolu Kopyala"),
    ("menu.copy_location", "Konumu Kopyala"),
    // --- Geliştirici Modu alt menüsü (Faz 39, context_menu.rs) ---
    ("menu.developer", "Geliştirici"),
    ("menu.copy_absolute_path", "Tam Yolu Kopyala"),
    ("menu.copy_uri", "URI Kopyala"),
    ("menu.copy_relative_path", "Göreceli Yolu Kopyala"),
    ("menu.open_in_editor", "Düzenleyicide Aç"),
    ("menu.calculate_checksums", "Sağlama Toplamlarını Hesapla…"),
    ("menu.developer_metadata", "Geliştirici Meta Verisi İnceleyici"),
    ("menu.properties", "Özellikler"),
    ("menu.new_folder", "Yeni Klasör"),
    ("menu.new_document", "Yeni Belge"),
    ("menu.paste", "Yapıştır"),
    ("menu.restore", "Geri Yükle"),
    ("menu.empty_trash", "Çöpü Boşalt"),
    // --- Operation toasts (widgets/progress_toast.rs) ---
    ("toast.copying", "Kopyalanıyor"),
    ("toast.moving", "Taşınıyor"),
    ("toast.moving_to_trash", "Çöpe Taşınıyor"),
    ("toast.deleting", "Siliniyor"),
    ("toast.compressing", "Sıkıştırılıyor"),
    ("toast.extracting", "Ayıklanıyor"),
    ("toast.pause", "Duraklat"),
    ("toast.resume", "Devam Ettir"),
    ("toast.cancel", "İptal"),
    // --- Status bar / item counts (window.rs) ---
    ("status.items_count.one", "1 öge"),
    ("status.items_count.other", "{count} öge"),
    ("status.free_space", "{size} boş"),
    ("status.loading", "Yükleniyor…"),
    ("status.cancelled", "İptal edildi"),
    ("status.archive_failed", "Arşivleme başarısız"),
    ("panel.files_page_title", "Dosyalar"),
    ("panel.sidebar_page_title", "Kenar Çubuğu"),
    // --- Preferences: Language row (dialogs/preferences_dialog.rs) ---
    ("prefs.dialog.title", "Tercihler"),
    ("prefs.language.title", "Dil"),
    ("prefs.language.subtitle", "Veyra bir sonraki başlatmada uygulanır"),
    ("prefs.common.system_default", "Sistem Varsayılanı"),
    // --- Preferences: page titles ---
    ("prefs.page.appearance", "Görünüm"),
    ("prefs.page.navigation", "Gezinme"),
    ("prefs.page.files", "Dosyalar"),
    ("prefs.page.search", "Arama"),
    ("prefs.page.preview", "Önizleme"),
    ("prefs.page.performance", "Performans"),
    ("prefs.page.shortcuts", "Kısayollar"),
    ("prefs.page.privacy", "Gizlilik"),
    ("prefs.page.advanced", "Gelişmiş"),
    // --- Preferences: Appearance ---
    ("prefs.appearance.group.theme_icons", "Tema ve Simgeler"),
    ("prefs.appearance.theme.title", "Tema"),
    ("prefs.appearance.theme.light", "Açık"),
    ("prefs.appearance.theme.dark", "Koyu"),
    ("prefs.appearance.accent_color.title", "Vurgu Rengi"),
    ("prefs.appearance.accent_color.blue", "Mavi"),
    ("prefs.appearance.accent_color.teal", "Deniz Mavisi"),
    ("prefs.appearance.accent_color.green", "Yeşil"),
    ("prefs.appearance.accent_color.yellow", "Sarı"),
    ("prefs.appearance.accent_color.orange", "Turuncu"),
    ("prefs.appearance.accent_color.red", "Kırmızı"),
    ("prefs.appearance.accent_color.purple", "Mor"),
    ("prefs.appearance.accent_color.pink", "Pembe"),
    ("prefs.appearance.accent_color.slate", "Arduvaz Grisi"),
    ("prefs.appearance.icon_size.title", "Simge Boyutu"),
    ("prefs.appearance.icon_size.small", "Küçük (48px)"),
    ("prefs.appearance.icon_size.normal", "Orta (64px)"),
    ("prefs.appearance.icon_size.large", "Büyük (96px)"),
    ("prefs.appearance.icon_size.extra_large", "Çok Büyük (128px)"),
    // --- Preferences: Navigation ---
    ("prefs.navigation.group.opening_items", "Ögeleri Açma"),
    ("prefs.navigation.click_policy.title", "Tıklama Davranışı"),
    ("prefs.navigation.click_policy.double", "Açmak İçin Çift Tıkla"),
    ("prefs.navigation.click_policy.single", "Açmak İçin Tek Tıkla"),
    ("prefs.navigation.open_new_tab.title", "Klasörleri Yeni Sekmede Aç"),
    (
        "prefs.navigation.open_new_tab.subtitle",
        "Orta tık ve kenar çubuğu gezinmesi bunu zaten yapıyor; burası da bir sonraki başlatmada geçerli olur",
    ),
    ("prefs.navigation.group.tabs", "Sekmeler"),
    ("prefs.navigation.restore_tabs.title", "Başlangıçta Önceki Sekmeleri Geri Yükle"),
    (
        "prefs.navigation.restore_tabs.subtitle",
        "Veyra son kapandığında açık olan her sekmeyi yeniden açar",
    ),
    // --- Preferences: Files ---
    ("prefs.files.group.display_defaults", "Görünüm Varsayılanları"),
    (
        "prefs.files.group.display_defaults.description",
        "Yeni açılan sekmelere uygulanır; açık sekmeler kendi ayarını korur.",
    ),
    ("prefs.files.show_hidden.title", "Gizli Dosyaları Göster"),
    ("prefs.files.show_hidden.subtitle", "Yeni sekmeler gizli dosyalar görünür şekilde başlar"),
    ("prefs.files.folders_first.title", "Önce Klasörler"),
    ("prefs.files.folders_first.subtitle", "Yeni sekmeler klasörleri dosyalardan önce listeler"),
    ("prefs.files.default_view_mode.title", "Varsayılan Görünüm Modu"),
    ("prefs.files.default_view_mode.icons", "Simgeler"),
    ("prefs.files.default_view_mode.compact", "Kompakt"),
    ("prefs.files.default_view_mode.details", "Ayrıntılar"),
    ("prefs.files.group.confirmations", "Onaylar"),
    ("prefs.files.confirm_trash.title", "Çöpü Boşaltmadan Önce Onay İste"),
    ("prefs.files.confirm_delete.title", "Kalıcı Silmeden Önce Onay İste"),
    // --- Preferences: Search ---
    ("prefs.search.group.indexing", "İndeksleme"),
    ("prefs.search.enable.title", "Hızlı Arama İndeksleyicisini Etkinleştir"),
    (
        "prefs.search.enable.subtitle",
        "name:/type:/size:/modified: aramalarını destekleyen SQLite + FTS5 indeksi",
    ),
    ("prefs.search.max_results.title", "En Fazla Arama Sonucu"),
    ("prefs.search.rebuild.title", "Arama İndeksini Yeniden Oluştur"),
    ("prefs.search.rebuild.subtitle", "Ev dizininizi baştan yeniden tarar"),
    ("prefs.search.rebuild.button", "Yeniden Oluştur"),
    // --- Preferences: Preview ---
    ("prefs.preview.group.panel", "Önizleme Paneli"),
    ("prefs.preview.enable.title", "Önizleme Panelini Etkinleştir"),
    ("prefs.preview.size_limit.title", "Önizleme Boyut Sınırı"),
    ("prefs.preview.folder_count.title", "Klasör Öge Sayısını Göster"),
    // --- Preferences: Performance ---
    ("prefs.performance.group.directory_loading", "Dizin Yükleme"),
    ("prefs.performance.chunk_size.title", "Dizin Akış Öbek Boyutu"),
    ("prefs.performance.chunk_size.unit.one", "{n} dosya"),
    ("prefs.performance.chunk_size.unit.other", "{n} dosya"),
    ("prefs.performance.cache_capacity.title", "Küçük Resim Önbellek Kapasitesi"),
    ("prefs.performance.cache_capacity.unit.one", "{n} küçük resim"),
    ("prefs.performance.cache_capacity.unit.other", "{n} küçük resim"),
    // --- Preferences: Shortcuts ---
    ("prefs.shortcuts.group.keyboard_shortcuts", "Klavye Kısayolları"),
    ("prefs.shortcuts.view_all.title", "Tüm Kısayolları Görüntüle"),
    ("prefs.shortcuts.view_all.subtitle", "Her eylem ve geçerli tuş ataması"),
    ("prefs.shortcuts.view_all.button", "Görüntüle"),
    ("prefs.shortcuts.reset.title", "Kısayolları Varsayılana Sıfırla"),
    ("prefs.shortcuts.reset.subtitle", "shortcuts.json içindeki tüm özelleştirmeleri siler"),
    ("prefs.shortcuts.reset.button", "Sıfırla"),
    // --- Preferences: Privacy ---
    ("prefs.privacy.group.history", "Geçmiş"),
    ("prefs.privacy.clear_files.title", "Son Dosyalar Geçmişini Temizle"),
    ("prefs.privacy.clear_files.subtitle", "XDG son kullanılanlar kaydını boşaltır"),
    ("prefs.privacy.clear_servers.title", "Son Sunucuları Temizle"),
    (
        "prefs.privacy.clear_servers.subtitle",
        "\"Sunucuya Bağlan\" adres geçmişini boşaltır",
    ),
    ("prefs.privacy.clear.button", "Temizle"),
    ("prefs.privacy.remember_files.title", "Son Kullanılan Dosyaları Hatırla"),
    (
        "prefs.privacy.remember_files.subtitle",
        "Kapalıyken yukarıdaki Son Dosyalar geçmişi de devre dışı kalır",
    ),
    ("prefs.privacy.group.logging", "Günlükleme"),
    ("prefs.privacy.sanitize_logs.title", "Günlüklerde Dosya Yollarını Gizle"),
    (
        "prefs.privacy.sanitize_logs.subtitle",
        "Tam yol yerine yalnızca dosya adını günlükle",
    ),
    ("prefs.privacy.group.telemetry", "Telemetri"),
    ("prefs.privacy.telemetry.title", "Sıfır Telemetri"),
    (
        "prefs.privacy.telemetry.subtitle",
        "Veyra hiçbir zaman kullanım verisi veya dosya içeriği göndermez (Kural #24)",
    ),
    // --- Preferences: Advanced ---
    ("prefs.advanced.group.reset", "Sıfırla"),
    ("prefs.advanced.reset_all.title", "Tüm Ayarları Varsayılana Sıfırla"),
    (
        "prefs.advanced.reset_all.subtitle",
        "Bu penceredeki her sayfayı varsayılan değerine döndürür",
    ),
    ("prefs.advanced.reset_all.button", "Tümünü Sıfırla"),
    ("prefs.advanced.reset_all.confirm_heading", "Tüm ayarlar varsayılana sıfırlansın mı?"),
    (
        "prefs.advanced.reset_all.confirm_body",
        "Her Ayarlar sayfası varsayılan değerine döner. Bu işlem geri alınamaz.",
    ),
    ("prefs.advanced.reset_all.cancel", "İptal"),
    ("prefs.advanced.group.developer", "Geliştirici"),
    ("prefs.advanced.developer_mode.title", "Geliştirici Modu"),
    (
        "prefs.advanced.developer_mode.subtitle",
        "Yol/URI kopyalama, sağlama toplamları, düzenleyici başlatma ve Git durum rozeti ekler",
    ),
    // --- Geliştirici Modu diyalogları (Faz 39) ---
    ("dev.checksum.title", "Sağlama Toplamları"),
    ("dev.checksum.close", "Kapat"),
    ("dev.checksum.computing", "Hesaplanıyor…"),
    ("dev.checksum.copy", "Kopyala"),
    // --- Sağlama Toplamları & Doğrulama (Faz 41) ---
    ("dev.checksum.calculate", "Sağlama Toplamlarını Hesapla"),
    (
        "dev.checksum.verify_placeholder",
        "Doğrulamak için beklenen sağlama toplamını yapıştırın…",
    ),
    ("dev.checksum.matches", "✓ {algorithm} İle Eşleşiyor"),
    ("dev.checksum.mismatch", "✗ Sağlama toplamı uyuşmuyor"),
    ("dev.metadata.title", "Geliştirici Meta Verisi"),
    ("dev.metadata.close", "Kapat"),
    ("dev.metadata.inode", "Inode"),
    ("dev.metadata.device_id", "Aygıt Kimliği"),
    ("dev.metadata.permissions", "İzinler"),
    ("dev.metadata.mime_type", "MIME Türü"),
    ("dev.metadata.hard_links", "Sabit Bağlantılar"),
    ("dev.metadata.unknown", "Bilinmiyor"),
    // --- Çift Dosyalar (Faz 42) ---
    ("dup.tab.title", "Çift Dosyalar"),
    ("dup.loading", "Dosya içerikleri karşılaştırılıyor…"),
    ("dup.empty", "Çift dosya bulunamadı."),
    ("dup.summary.groups.one", "{count} çift grubu"),
    ("dup.summary.groups.other", "{count} çift grubu"),
    ("dup.summary.files.one", "{count} dosya"),
    ("dup.summary.files.other", "{count} dosya"),
    ("dup.summary.wasted", "{size} boşa harcanmış"),
    ("dup.select_keep_newest", "Tüm Kopyaları Seç (En Yeniyi Koru)"),
    ("dup.select_keep_oldest", "Tüm Kopyaları Seç (En Eskiyi Koru)"),
    ("dup.clear_selection", "Seçimi Temizle"),
    ("dup.reveal_in_folder", "Klasörde Göster"),
    ("dup.group.copies.one", "{count} kopya • her biri {size}"),
    ("dup.group.copies.other", "{count} kopya • her biri {size}"),
    ("dup.group.total", "toplam {size}"),
    ("dup.selection_status.one", "{count} dosya seçildi • {size} kazanılacak"),
    ("dup.selection_status.other", "{count} dosya seçildi • {size} kazanılacak"),
    ("dup.selection_status.empty", "Seçili dosya yok"),
    ("dup.move_to_trash", "Seçilenleri Çöpe Taşı"),
    ("dup.confirm.title.one", "{count} Dosya Çöpe Taşınsın mı?"),
    ("dup.confirm.title.other", "{count} Dosya Çöpe Taşınsın mı?"),
    (
        "dup.confirm.body",
        "{count} dosya ({size}) Çöp Kutusuna taşınacak. Ctrl+Z ile geri alabilirsiniz.",
    ),
    ("dup.confirm.cancel", "İptal"),
    ("dup.confirm.confirm", "Çöpe Taşı"),
    ("dup.block_all_copies.title", "Tüm Kopyalar Silinemez"),
    (
        "dup.block_all_copies.body",
        "Her çift grubunda en az bir kopya kalmalıdır. Şu gruplarda en az bir dosyanın seçimini kaldırın: {names}",
    ),
    ("dup.block_all_copies.close", "Tamam"),
    ("dup.trash_error.title", "Bazı Dosyalar Taşınamadı"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_en_key_has_a_tr_counterpart() {
        for (key, _) in EN {
            assert!(
                TR.iter().any(|(candidate, _)| candidate == key),
                "tr catalog is missing key {key:?}"
            );
        }
    }

    #[test]
    fn every_tr_key_exists_in_en() {
        // Catches typo'd keys added only to `TR` that would silently never
        // be reached (`t`/`t_plural` only ever look a key up by its `EN`
        // spelling plus locale-table fallback, so a `TR`-only key is dead).
        for (key, _) in TR {
            assert!(
                EN.iter().any(|(candidate, _)| candidate == key),
                "en catalog is missing key {key:?}"
            );
        }
    }

    #[test]
    fn no_catalog_entry_is_empty() {
        for (key, value) in EN.iter().chain(TR) {
            assert!(!key.is_empty());
            assert!(!value.is_empty(), "empty translation for key {key:?}");
        }
    }

    #[test]
    fn no_duplicate_keys_within_a_table() {
        for table in [EN, TR] {
            let mut seen = std::collections::HashSet::new();
            for (key, _) in table {
                assert!(seen.insert(*key), "duplicate key {key:?} in table");
            }
        }
    }

    #[test]
    fn plural_keys_have_both_one_and_other_forms() {
        for (key, _) in EN {
            if let Some(stem) = key.strip_suffix(".one") {
                let other = format!("{stem}.other");
                assert!(
                    EN.iter().any(|(candidate, _)| *candidate == other),
                    "{key:?} has no matching {other:?}"
                );
            }
        }
    }

    #[test]
    fn t_falls_back_to_english_for_a_key_missing_in_the_current_locale() {
        set_locale(Locale::Tr);
        // Every real key exists in both tables (enforced by the parity
        // tests above), so a synthetic missing key is used here instead to
        // exercise the fallback path itself.
        assert_eq!(t("does.not.exist.anywhere"), "does.not.exist.anywhere");
        set_locale(Locale::En);
    }

    #[test]
    fn t_returns_the_locale_specific_translation() {
        set_locale(Locale::En);
        assert_eq!(t("menu.rename"), "Rename");
        set_locale(Locale::Tr);
        assert_eq!(t("menu.rename"), "Yeniden Adlandır");
        set_locale(Locale::En);
    }

    #[test]
    fn t_fmt_substitutes_named_placeholders() {
        set_locale(Locale::En);
        assert_eq!(
            t_fmt("status.free_space", &[("size", "12.3 GB")]),
            "12.3 GB free"
        );
    }

    #[test]
    fn t_plural_picks_singular_for_exactly_one() {
        set_locale(Locale::En);
        assert_eq!(
            t_plural("status.items_count", 1, &[("count", "1")]),
            "1 item"
        );
    }

    #[test]
    fn t_plural_picks_plural_for_zero_and_many() {
        set_locale(Locale::En);
        assert_eq!(
            t_plural("status.items_count", 0, &[("count", "0")]),
            "0 items"
        );
        assert_eq!(
            t_plural("status.items_count", 5, &[("count", "5")]),
            "5 items"
        );
    }

    #[test]
    fn t_plural_in_turkish_uses_the_turkish_table() {
        set_locale(Locale::Tr);
        assert_eq!(
            t_plural("menu.delete_permanently", 3, &[("count", "3")]),
            "3 ögeyi Kalıcı Olarak Sil"
        );
        set_locale(Locale::En);
    }

    #[test]
    fn locale_default_is_english() {
        assert_eq!(current_locale(), Locale::En);
    }

    #[test]
    fn locale_code_round_trips_through_detection_matching() {
        assert_eq!(Locale::En.code(), "en");
        assert_eq!(Locale::Tr.code(), "tr");
    }
}
