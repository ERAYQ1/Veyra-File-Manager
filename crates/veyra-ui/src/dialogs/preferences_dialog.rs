//! Faz 34: the `AdwPreferencesDialog` (`Ctrl+,`, `win.show-preferences`)
//! centralizing every user preference `config::VeyraSettings` models.
//! Every row mutates `settings` in place, persists it immediately via
//! `VeyraSettings::save`, and applies whatever live effect that setting has
//! (see `config.rs`'s live-apply table) — there is no separate "Apply"/"OK"
//! step, matching how every other GNOME/Libadwaita preferences window
//! behaves.

use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::config::{
    AccentColorPref, ClickPolicy, ColorSchemePref, CompressionLevelPref, ConflictDefaultAction,
    DateFormatPref, DefaultViewMode, IconSizePref, LanguagePref, NewTabLocation, SharedSettings,
    SizeUnitPref, TerminalPref, ARCHIVE_FORMAT_CHOICES, MAX_PREVIEW_SIZE_CHOICES_KB,
    SEARCH_MAX_DEPTH_CHOICES, STREAM_CHUNK_SIZE_CHOICES, THUMBNAIL_CACHE_CAPACITY_CHOICES,
};
use crate::i18n::{t, t_fmt, t_plural};
use crate::network;
use crate::recent;
use crate::thumbnails::ThumbnailService;

/// Shows the Preferences dialog over `window`.
///
/// `rebuild_search_index` re-spawns the Faz 9 background indexer (used by
/// both the "Rebuild Search Index" button and re-enabling "Enable Fast
/// Search Indexer" after it was off). `refresh_all_tabs` reloads every open
/// tab's listing (needed for "Icon Size" and "Directory Stream Chunk Size"
/// to visibly apply to tabs already open). `preview_widget` is toggled
/// directly for "Enable Preview Panel".
pub(crate) fn show(
    window: &adw::ApplicationWindow,
    settings: SharedSettings,
    thumbnails: Rc<ThumbnailService>,
    rebuild_search_index: Rc<dyn Fn()>,
    refresh_all_tabs: Rc<dyn Fn()>,
    preview_widget: gtk4::Widget,
    tags: crate::tags::SharedTags,
) {
    let dialog = adw::PreferencesDialog::builder()
        .title(t("prefs.dialog.title"))
        .content_width(560)
        .content_height(600)
        .search_enabled(true)
        .build();

    dialog.add(&appearance_page(&settings, &refresh_all_tabs));
    dialog.add(&navigation_page(&settings, &refresh_all_tabs));
    dialog.add(&files_page(&settings));
    dialog.add(&search_page(&settings, &rebuild_search_index));
    dialog.add(&preview_page(&settings, &preview_widget));
    dialog.add(&performance_page(&settings, &thumbnails, &refresh_all_tabs));
    dialog.add(&tags_page(&tags, &refresh_all_tabs));
    dialog.add(&shortcuts_page(window));
    dialog.add(&privacy_page(&settings));
    dialog.add(&advanced_page(
        &dialog,
        &settings,
        &thumbnails,
        &preview_widget,
        &refresh_all_tabs,
    ));

    dialog.present(Some(window));
}

/// Persists `settings` to disk, logging (never panicking — Kural #15) if the
/// write fails; the in-memory value the rest of the app reads is already
/// updated by the caller before this runs, so a save failure only means the
/// change won't survive a restart, not that it's not live right now.
fn persist(settings: &SharedSettings) {
    if let Err(err) = settings.borrow().save() {
        tracing::warn!(error = %err, "failed to save settings.json");
    }
}

fn group(title: &str) -> adw::PreferencesGroup {
    adw::PreferencesGroup::builder().title(title).build()
}

fn switch_row(title: &str, subtitle: &str, active: bool) -> adw::SwitchRow {
    adw::SwitchRow::builder()
        .title(title)
        .subtitle(subtitle)
        .active(active)
        .build()
}

fn combo_row(title: &str, options: &[&str], selected: usize) -> adw::ComboRow {
    let row = adw::ComboRow::builder().title(title).build();
    row.set_model(Some(&gtk4::StringList::new(options)));
    row.set_selected(selected as u32);
    row
}

fn action_row_button(
    title: &str,
    subtitle: &str,
    button_label: &str,
) -> (adw::ActionRow, gtk4::Button) {
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .build();
    let button = gtk4::Button::builder()
        .label(button_label)
        .valign(gtk4::Align::Center)
        .build();
    row.add_suffix(&button);
    (row, button)
}

fn appearance_page(
    settings: &SharedSettings,
    refresh_all_tabs: &Rc<dyn Fn()>,
) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(t("prefs.page.appearance"))
        .icon_name("preferences-desktop-theme-symbolic")
        .name("appearance")
        .build();
    let g = group(t("prefs.appearance.group.theme_icons"));

    let current = settings.borrow().color_scheme;
    let theme_row = combo_row(
        t("prefs.appearance.theme.title"),
        &[
            t("prefs.common.system_default"),
            t("prefs.appearance.theme.light"),
            t("prefs.appearance.theme.dark"),
        ],
        match current {
            ColorSchemePref::System => 0,
            ColorSchemePref::Light => 1,
            ColorSchemePref::Dark => 2,
        },
    );
    {
        let settings = settings.clone();
        theme_row.connect_selected_notify(move |row| {
            let scheme = match row.selected() {
                1 => ColorSchemePref::Light,
                2 => ColorSchemePref::Dark,
                _ => ColorSchemePref::System,
            };
            settings.borrow_mut().color_scheme = scheme;
            persist(&settings);
            adw::StyleManager::default().set_color_scheme(scheme.to_adw());
        });
    }
    g.add(&theme_row);

    let accent_choices: Vec<AccentColorPref> = std::iter::once(AccentColorPref::System)
        .chain(AccentColorPref::ALL)
        .collect();
    let accent_labels: Vec<&str> = accent_choices.iter().map(|c| c.label()).collect();
    let current = settings.borrow().accent_color;
    let accent_row = combo_row(
        t("prefs.appearance.accent_color.title"),
        &accent_labels,
        accent_choices
            .iter()
            .position(|c| *c == current)
            .unwrap_or(0),
    );
    {
        let settings = settings.clone();
        let accent_choices = accent_choices.clone();
        accent_row.connect_selected_notify(move |row| {
            let accent = accent_choices
                .get(row.selected() as usize)
                .copied()
                .unwrap_or_default();
            settings.borrow_mut().accent_color = accent;
            persist(&settings);
            crate::config::apply_accent_color(accent);
        });
    }
    g.add(&accent_row);

    let current = settings.borrow().icon_size;
    let icon_size_labels: Vec<&str> = IconSizePref::ALL.iter().map(|s| s.label()).collect();
    let icon_size_row = combo_row(
        t("prefs.appearance.icon_size.title"),
        &icon_size_labels,
        IconSizePref::ALL
            .iter()
            .position(|s| *s == current)
            .unwrap_or(1),
    );
    {
        let settings = settings.clone();
        let refresh_all_tabs = refresh_all_tabs.clone();
        icon_size_row.connect_selected_notify(move |row| {
            let size = IconSizePref::ALL
                .get(row.selected() as usize)
                .copied()
                .unwrap_or_default();
            settings.borrow_mut().icon_size = size;
            persist(&settings);
            refresh_all_tabs();
        });
    }
    g.add(&icon_size_row);

    let language_choices = LanguagePref::ALL;
    let language_labels: Vec<&str> = language_choices.iter().map(|l| l.label()).collect();
    let current = settings.borrow().language;
    let language_row = combo_row(
        t("prefs.language.title"),
        &language_labels,
        language_choices
            .iter()
            .position(|l| *l == current)
            .unwrap_or(0),
    );
    language_row.set_subtitle(t("prefs.language.subtitle"));
    {
        let settings = settings.clone();
        language_row.connect_selected_notify(move |row| {
            let language = language_choices
                .get(row.selected() as usize)
                .copied()
                .unwrap_or_default();
            settings.borrow_mut().language = language;
            persist(&settings);
        });
    }
    g.add(&language_row);
    page.add(&g);

    let format_group = group(t("prefs.appearance.group.formatting"));

    let current = settings.borrow().date_format;
    let date_format_row = combo_row(
        t("prefs.appearance.date_format.title"),
        &[
            t("prefs.appearance.date_format.relative"),
            t("prefs.appearance.date_format.absolute"),
        ],
        match current {
            DateFormatPref::Relative => 0,
            DateFormatPref::Absolute => 1,
        },
    );
    {
        let settings = settings.clone();
        let refresh_all_tabs = refresh_all_tabs.clone();
        date_format_row.connect_selected_notify(move |row| {
            let pref = if row.selected() == 1 {
                DateFormatPref::Absolute
            } else {
                DateFormatPref::Relative
            };
            settings.borrow_mut().date_format = pref;
            persist(&settings);
            crate::config::set_date_format(pref);
            refresh_all_tabs();
        });
    }
    format_group.add(&date_format_row);

    let current = settings.borrow().size_unit;
    let size_unit_row = combo_row(
        t("prefs.appearance.size_unit.title"),
        &[
            t("prefs.appearance.size_unit.binary"),
            t("prefs.appearance.size_unit.decimal"),
        ],
        match current {
            SizeUnitPref::Binary => 0,
            SizeUnitPref::Decimal => 1,
        },
    );
    {
        let settings = settings.clone();
        let refresh_all_tabs = refresh_all_tabs.clone();
        size_unit_row.connect_selected_notify(move |row| {
            let pref = if row.selected() == 1 {
                SizeUnitPref::Decimal
            } else {
                SizeUnitPref::Binary
            };
            settings.borrow_mut().size_unit = pref;
            persist(&settings);
            crate::config::set_size_unit(pref);
            refresh_all_tabs();
        });
    }
    format_group.add(&size_unit_row);
    page.add(&format_group);

    page
}

fn navigation_page(
    settings: &SharedSettings,
    refresh_all_tabs: &Rc<dyn Fn()>,
) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(t("prefs.page.navigation"))
        .icon_name("go-next-symbolic")
        .name("navigation")
        .build();

    let g = group(t("prefs.navigation.group.opening_items"));
    let current = settings.borrow().click_policy;
    let click_row = combo_row(
        t("prefs.navigation.click_policy.title"),
        &[
            t("prefs.navigation.click_policy.double"),
            t("prefs.navigation.click_policy.single"),
        ],
        match current {
            ClickPolicy::DoubleClick => 0,
            ClickPolicy::SingleClick => 1,
        },
    );
    {
        let settings = settings.clone();
        click_row.connect_selected_notify(move |row| {
            settings.borrow_mut().click_policy = if row.selected() == 1 {
                ClickPolicy::SingleClick
            } else {
                ClickPolicy::DoubleClick
            };
            persist(&settings);
        });
    }
    g.add(&click_row);

    let open_new_tab_row = switch_row(
        t("prefs.navigation.open_new_tab.title"),
        t("prefs.navigation.open_new_tab.subtitle"),
        settings.borrow().open_folders_in_new_tab,
    );
    {
        let settings = settings.clone();
        open_new_tab_row.connect_active_notify(move |row| {
            settings.borrow_mut().open_folders_in_new_tab = row.is_active();
            persist(&settings);
        });
    }
    g.add(&open_new_tab_row);

    let current = settings.borrow().natural_sort;
    let natural_sort_row = switch_row(
        t("prefs.navigation.natural_sort.title"),
        t("prefs.navigation.natural_sort.subtitle"),
        current,
    );
    {
        let settings = settings.clone();
        let refresh_all_tabs = refresh_all_tabs.clone();
        natural_sort_row.connect_active_notify(move |row| {
            settings.borrow_mut().natural_sort = row.is_active();
            persist(&settings);
            refresh_all_tabs();
        });
    }
    g.add(&natural_sort_row);
    page.add(&g);

    let tabs_group = group(t("prefs.navigation.group.tabs"));

    let current = settings.borrow().new_tab_location;
    let new_tab_location_row = combo_row(
        t("prefs.navigation.new_tab_location.title"),
        &[
            t("prefs.navigation.new_tab_location.current_folder"),
            t("prefs.navigation.new_tab_location.home"),
        ],
        match current {
            NewTabLocation::CurrentFolder => 0,
            NewTabLocation::Home => 1,
        },
    );
    {
        let settings = settings.clone();
        new_tab_location_row.connect_selected_notify(move |row| {
            settings.borrow_mut().new_tab_location = if row.selected() == 1 {
                NewTabLocation::Home
            } else {
                NewTabLocation::CurrentFolder
            };
            persist(&settings);
        });
    }
    tabs_group.add(&new_tab_location_row);

    let restore_row = switch_row(
        t("prefs.navigation.restore_tabs.title"),
        t("prefs.navigation.restore_tabs.subtitle"),
        settings.borrow().restore_tabs_on_startup,
    );
    {
        let settings = settings.clone();
        restore_row.connect_active_notify(move |row| {
            settings.borrow_mut().restore_tabs_on_startup = row.is_active();
            persist(&settings);
        });
    }
    tabs_group.add(&restore_row);
    page.add(&tabs_group);

    page
}

fn files_page(settings: &SharedSettings) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(t("prefs.page.files"))
        .icon_name("folder-symbolic")
        .name("files")
        .build();

    let display_group = group(t("prefs.files.group.display_defaults"));
    display_group.set_description(Some(t("prefs.files.group.display_defaults.description")));

    let hidden_row = switch_row(
        t("prefs.files.show_hidden.title"),
        t("prefs.files.show_hidden.subtitle"),
        settings.borrow().show_hidden,
    );
    {
        let settings = settings.clone();
        hidden_row.connect_active_notify(move |row| {
            settings.borrow_mut().show_hidden = row.is_active();
            persist(&settings);
        });
    }
    display_group.add(&hidden_row);

    let folders_first_row = switch_row(
        t("prefs.files.folders_first.title"),
        t("prefs.files.folders_first.subtitle"),
        settings.borrow().folders_first,
    );
    {
        let settings = settings.clone();
        folders_first_row.connect_active_notify(move |row| {
            settings.borrow_mut().folders_first = row.is_active();
            persist(&settings);
        });
    }
    display_group.add(&folders_first_row);

    let current = settings.borrow().default_view_mode;
    let view_mode_row = combo_row(
        t("prefs.files.default_view_mode.title"),
        &[
            t("prefs.files.default_view_mode.icons"),
            t("prefs.files.default_view_mode.compact"),
            t("prefs.files.default_view_mode.details"),
        ],
        match current {
            DefaultViewMode::Icon => 0,
            DefaultViewMode::Compact => 1,
            DefaultViewMode::Details => 2,
        },
    );
    {
        let settings = settings.clone();
        view_mode_row.connect_selected_notify(move |row| {
            settings.borrow_mut().default_view_mode = match row.selected() {
                1 => DefaultViewMode::Compact,
                2 => DefaultViewMode::Details,
                _ => DefaultViewMode::Icon,
            };
            persist(&settings);
        });
    }
    display_group.add(&view_mode_row);
    page.add(&display_group);

    let confirm_group = group(t("prefs.files.group.confirmations"));
    let confirm_trash_row = switch_row(
        t("prefs.files.confirm_trash.title"),
        "",
        settings.borrow().confirm_trash_empty,
    );
    {
        let settings = settings.clone();
        confirm_trash_row.connect_active_notify(move |row| {
            settings.borrow_mut().confirm_trash_empty = row.is_active();
            persist(&settings);
        });
    }
    confirm_group.add(&confirm_trash_row);

    let confirm_delete_row = switch_row(
        t("prefs.files.confirm_delete.title"),
        "",
        settings.borrow().confirm_permanent_delete,
    );
    {
        let settings = settings.clone();
        confirm_delete_row.connect_active_notify(move |row| {
            settings.borrow_mut().confirm_permanent_delete = row.is_active();
            persist(&settings);
        });
    }
    confirm_group.add(&confirm_delete_row);

    let current = settings.borrow().default_conflict_action;
    let conflict_row = combo_row(
        t("prefs.files.conflict_action.title"),
        &[
            t("prefs.files.conflict_action.always_ask"),
            t("prefs.files.conflict_action.auto_rename"),
            t("prefs.files.conflict_action.overwrite"),
            t("prefs.files.conflict_action.skip"),
        ],
        match current {
            ConflictDefaultAction::AlwaysAsk => 0,
            ConflictDefaultAction::AutoRename => 1,
            ConflictDefaultAction::Overwrite => 2,
            ConflictDefaultAction::Skip => 3,
        },
    );
    {
        let settings = settings.clone();
        conflict_row.connect_selected_notify(move |row| {
            settings.borrow_mut().default_conflict_action = match row.selected() {
                1 => ConflictDefaultAction::AutoRename,
                2 => ConflictDefaultAction::Overwrite,
                3 => ConflictDefaultAction::Skip,
                _ => ConflictDefaultAction::AlwaysAsk,
            };
            persist(&settings);
        });
    }
    confirm_group.add(&conflict_row);

    let bidi_row = switch_row(
        t("prefs.files.warn_bidi.title"),
        t("prefs.files.warn_bidi.subtitle"),
        settings.borrow().warn_bidi_spoofing,
    );
    {
        let settings = settings.clone();
        bidi_row.connect_active_notify(move |row| {
            settings.borrow_mut().warn_bidi_spoofing = row.is_active();
            persist(&settings);
        });
    }
    confirm_group.add(&bidi_row);
    page.add(&confirm_group);

    let archive_group = group(t("prefs.files.group.archive"));

    let current = settings.borrow().default_archive_format;
    let archive_labels: Vec<&str> = ARCHIVE_FORMAT_CHOICES.iter().map(|f| f.label()).collect();
    let archive_row = combo_row(
        t("prefs.files.archive_format.title"),
        &archive_labels,
        ARCHIVE_FORMAT_CHOICES
            .iter()
            .position(|f| *f == current)
            .unwrap_or(0),
    );
    {
        let settings = settings.clone();
        archive_row.connect_selected_notify(move |row| {
            if let Some(format) = ARCHIVE_FORMAT_CHOICES.get(row.selected() as usize) {
                settings.borrow_mut().default_archive_format = *format;
                persist(&settings);
            }
        });
    }
    archive_group.add(&archive_row);

    let current = settings.borrow().compression_level;
    let compression_labels: Vec<&str> = CompressionLevelPref::ALL
        .iter()
        .map(|l| l.label())
        .collect();
    let compression_row = combo_row(
        t("prefs.files.compression_level.title"),
        &compression_labels,
        CompressionLevelPref::ALL
            .iter()
            .position(|l| *l == current)
            .unwrap_or(1),
    );
    {
        let settings = settings.clone();
        compression_row.connect_selected_notify(move |row| {
            if let Some(level) = CompressionLevelPref::ALL.get(row.selected() as usize) {
                settings.borrow_mut().compression_level = *level;
                persist(&settings);
                veyra_filesystem::set_compression_level(level.level());
            }
        });
    }
    archive_group.add(&compression_row);
    page.add(&archive_group);

    page
}

fn search_page(
    settings: &SharedSettings,
    rebuild_search_index: &Rc<dyn Fn()>,
) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(t("prefs.page.search"))
        .icon_name("system-search-symbolic")
        .name("search")
        .build();
    let g = group(t("prefs.search.group.indexing"));

    let enable_row = switch_row(
        t("prefs.search.enable.title"),
        t("prefs.search.enable.subtitle"),
        settings.borrow().enable_fts_index,
    );
    {
        let settings = settings.clone();
        let rebuild_search_index = rebuild_search_index.clone();
        enable_row.connect_active_notify(move |row| {
            let enabled = row.is_active();
            settings.borrow_mut().enable_fts_index = enabled;
            persist(&settings);
            if enabled {
                rebuild_search_index();
            }
        });
    }
    g.add(&enable_row);

    let max_results_row = adw::SpinRow::with_range(50.0, 5000.0, 50.0);
    max_results_row.set_title(t("prefs.search.max_results.title"));
    max_results_row.set_value(settings.borrow().max_search_results as f64);
    {
        let settings = settings.clone();
        max_results_row.connect_value_notify(move |row| {
            settings.borrow_mut().max_search_results = row.value() as usize;
            persist(&settings);
        });
    }
    g.add(&max_results_row);

    let current = settings.borrow().search_max_depth;
    let depth_labels: Vec<String> = SEARCH_MAX_DEPTH_CHOICES
        .iter()
        .map(|n| n.to_string())
        .collect();
    let depth_labels_ref: Vec<&str> = depth_labels.iter().map(String::as_str).collect();
    let depth_row = combo_row(
        t("prefs.search.max_depth.title"),
        &depth_labels_ref,
        SEARCH_MAX_DEPTH_CHOICES
            .iter()
            .position(|n| *n == current)
            .unwrap_or(2),
    );
    {
        let settings = settings.clone();
        let rebuild_search_index = rebuild_search_index.clone();
        depth_row.connect_selected_notify(move |row| {
            if let Some(n) = SEARCH_MAX_DEPTH_CHOICES.get(row.selected() as usize) {
                settings.borrow_mut().search_max_depth = *n;
                persist(&settings);
                rebuild_search_index();
            }
        });
    }
    g.add(&depth_row);

    let include_hidden_row = switch_row(
        t("prefs.search.include_hidden.title"),
        t("prefs.search.include_hidden.subtitle"),
        settings.borrow().search_include_hidden,
    );
    {
        let settings = settings.clone();
        let rebuild_search_index = rebuild_search_index.clone();
        include_hidden_row.connect_active_notify(move |row| {
            settings.borrow_mut().search_include_hidden = row.is_active();
            persist(&settings);
            rebuild_search_index();
        });
    }
    g.add(&include_hidden_row);

    let (rebuild_row, rebuild_button) = action_row_button(
        t("prefs.search.rebuild.title"),
        t("prefs.search.rebuild.subtitle"),
        t("prefs.search.rebuild.button"),
    );
    {
        let rebuild_search_index = rebuild_search_index.clone();
        rebuild_button.connect_clicked(move |_| rebuild_search_index());
    }
    g.add(&rebuild_row);

    page.add(&g);
    page
}

fn preview_page(settings: &SharedSettings, preview_widget: &gtk4::Widget) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(t("prefs.page.preview"))
        .icon_name("view-reveal-symbolic")
        .name("preview")
        .build();
    let g = group(t("prefs.preview.group.panel"));

    let enable_row = switch_row(
        t("prefs.preview.enable.title"),
        "",
        settings.borrow().enable_preview_panel,
    );
    {
        let settings = settings.clone();
        let preview_widget = preview_widget.clone();
        enable_row.connect_active_notify(move |row| {
            let enabled = row.is_active();
            settings.borrow_mut().enable_preview_panel = enabled;
            persist(&settings);
            preview_widget.set_visible(enabled);
        });
    }
    g.add(&enable_row);

    let current = settings.borrow().max_preview_size_kb;
    let size_labels: Vec<String> = MAX_PREVIEW_SIZE_CHOICES_KB
        .iter()
        .map(|kb| format_kb(*kb))
        .collect();
    let size_labels_ref: Vec<&str> = size_labels.iter().map(String::as_str).collect();
    let size_row = combo_row(
        t("prefs.preview.size_limit.title"),
        &size_labels_ref,
        MAX_PREVIEW_SIZE_CHOICES_KB
            .iter()
            .position(|kb| *kb == current)
            .unwrap_or(1),
    );
    {
        let settings = settings.clone();
        size_row.connect_selected_notify(move |row| {
            if let Some(kb) = MAX_PREVIEW_SIZE_CHOICES_KB.get(row.selected() as usize) {
                settings.borrow_mut().max_preview_size_kb = *kb;
                persist(&settings);
            }
        });
    }
    g.add(&size_row);

    let folder_count_row = switch_row(
        t("prefs.preview.folder_count.title"),
        "",
        settings.borrow().show_folder_count,
    );
    {
        let settings = settings.clone();
        folder_count_row.connect_active_notify(move |row| {
            settings.borrow_mut().show_folder_count = row.is_active();
            persist(&settings);
        });
    }
    g.add(&folder_count_row);
    page.add(&g);

    let ql_group = group(t("prefs.preview.group.quick_look"));

    let quick_look_row = switch_row(
        t("prefs.preview.quick_look.title"),
        t("prefs.preview.quick_look.subtitle"),
        settings.borrow().enable_quick_look,
    );
    {
        let settings = settings.clone();
        quick_look_row.connect_active_notify(move |row| {
            settings.borrow_mut().enable_quick_look = row.is_active();
            persist(&settings);
        });
    }
    ql_group.add(&quick_look_row);

    let line_numbers_row = switch_row(
        t("prefs.preview.quick_look_line_numbers.title"),
        "",
        settings.borrow().quick_look_line_numbers,
    );
    {
        let settings = settings.clone();
        line_numbers_row.connect_active_notify(move |row| {
            settings.borrow_mut().quick_look_line_numbers = row.is_active();
            persist(&settings);
        });
    }
    ql_group.add(&line_numbers_row);

    let autoplay_row = switch_row(
        t("prefs.preview.media_autoplay.title"),
        "",
        settings.borrow().media_autoplay,
    );
    {
        let settings = settings.clone();
        autoplay_row.connect_active_notify(move |row| {
            settings.borrow_mut().media_autoplay = row.is_active();
            persist(&settings);
        });
    }
    ql_group.add(&autoplay_row);
    page.add(&ql_group);

    page
}

fn performance_page(
    settings: &SharedSettings,
    thumbnails: &Rc<ThumbnailService>,
    refresh_all_tabs: &Rc<dyn Fn()>,
) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(t("prefs.page.performance"))
        .icon_name("speedometer-symbolic")
        .name("performance")
        .build();
    let g = group(t("prefs.performance.group.directory_loading"));

    let current = settings.borrow().stream_chunk_size;
    let chunk_labels: Vec<String> = STREAM_CHUNK_SIZE_CHOICES
        .iter()
        .map(|n| {
            t_plural(
                "prefs.performance.chunk_size.unit",
                *n as i64,
                &[("n", &n.to_string())],
            )
        })
        .collect();
    let chunk_labels_ref: Vec<&str> = chunk_labels.iter().map(String::as_str).collect();
    let chunk_row = combo_row(
        t("prefs.performance.chunk_size.title"),
        &chunk_labels_ref,
        STREAM_CHUNK_SIZE_CHOICES
            .iter()
            .position(|n| *n == current)
            .unwrap_or(1),
    );
    {
        let settings = settings.clone();
        let refresh_all_tabs = refresh_all_tabs.clone();
        chunk_row.connect_selected_notify(move |row| {
            if let Some(n) = STREAM_CHUNK_SIZE_CHOICES.get(row.selected() as usize) {
                settings.borrow_mut().stream_chunk_size = *n;
                persist(&settings);
                refresh_all_tabs();
            }
        });
    }
    g.add(&chunk_row);

    let current = settings.borrow().thumbnail_cache_capacity;
    let cache_labels: Vec<String> = THUMBNAIL_CACHE_CAPACITY_CHOICES
        .iter()
        .map(|n| {
            t_plural(
                "prefs.performance.cache_capacity.unit",
                *n as i64,
                &[("n", &n.to_string())],
            )
        })
        .collect();
    let cache_labels_ref: Vec<&str> = cache_labels.iter().map(String::as_str).collect();
    let cache_row = combo_row(
        t("prefs.performance.cache_capacity.title"),
        &cache_labels_ref,
        THUMBNAIL_CACHE_CAPACITY_CHOICES
            .iter()
            .position(|n| *n == current)
            .unwrap_or(1),
    );
    {
        let settings = settings.clone();
        let thumbnails = thumbnails.clone();
        cache_row.connect_selected_notify(move |row| {
            if let Some(n) = THUMBNAIL_CACHE_CAPACITY_CHOICES.get(row.selected() as usize) {
                settings.borrow_mut().thumbnail_cache_capacity = *n;
                persist(&settings);
                thumbnails.resize_l1(*n);
            }
        });
    }
    g.add(&cache_row);
    page.add(&g);

    let reflink_group = group(t("prefs.performance.group.reflink"));
    let reflink_row = switch_row(
        t("prefs.performance.reflink.title"),
        t("prefs.performance.reflink.subtitle"),
        settings.borrow().enable_reflink,
    );
    {
        let settings = settings.clone();
        reflink_row.connect_active_notify(move |row| {
            let enabled = row.is_active();
            settings.borrow_mut().enable_reflink = enabled;
            persist(&settings);
            veyra_filesystem::set_reflink_enabled(enabled);
        });
    }
    reflink_group.add(&reflink_row);
    page.add(&reflink_group);

    let disk_group = group(t("prefs.performance.group.disk_cache"));
    let (clear_thumbnails_row, clear_thumbnails_button) = action_row_button(
        t("prefs.performance.disk_cache.title"),
        &t_fmt(
            "prefs.performance.disk_cache.subtitle",
            &[(
                "size",
                &veyra_filesystem::format_size(thumbnails.l2_cache_size_bytes()),
            )],
        ),
        t("prefs.privacy.clear.button"),
    );
    {
        let thumbnails = thumbnails.clone();
        let clear_thumbnails_row = clear_thumbnails_row.clone();
        clear_thumbnails_button.connect_clicked(move |_| {
            thumbnails.clear_l2_cache();
            clear_thumbnails_row.set_subtitle(&t_fmt(
                "prefs.performance.disk_cache.subtitle",
                &[("size", &veyra_filesystem::format_size(0))],
            ));
        });
    }
    disk_group.add(&clear_thumbnails_row);
    page.add(&disk_group);

    page
}

/// Faz 63: the "Tags" Preferences page — one `AdwEntryRow` per standard
/// color to rename it (empty entry = "use the default localized name"),
/// plus a "Tag Maintenance" group for resetting names or wiping every tag
/// assignment. Every write goes straight to `veyra_filesystem::tags`
/// (there's no separate settings object for this — see `tags.rs`'s module
/// doc comment for why names/assignments are independent maps in the same
/// file) and `refresh_all_tabs` is called afterwards so already-open tabs'
/// row tooltips pick up the new name immediately; the sidebar's Tags
/// section refreshes on its own via `crate::tags::watch`, and the context
/// menu is naturally live since it's rebuilt from scratch on every
/// right-click.
fn tags_page(
    tags: &crate::tags::SharedTags,
    refresh_all_tabs: &Rc<dyn Fn()>,
) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(t("prefs.page.tags"))
        .icon_name("tag-symbolic")
        .name("tags")
        .build();

    let names_group = adw::PreferencesGroup::builder()
        .title(t("prefs.tags.custom_names_group"))
        .description(t("prefs.tags.custom_names_subtitle"))
        .build();

    let entry_rows: Rc<Vec<adw::EntryRow>> = Rc::new(
        veyra_filesystem::TagColor::ALL
            .iter()
            .map(|&color| {
                let entry = adw::EntryRow::builder()
                    .title(crate::tags::default_label(color))
                    .show_apply_button(true)
                    .build();
                if let Some(custom) = veyra_filesystem::get_custom_tag_name(color) {
                    entry.set_text(&custom);
                }
                let dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
                dot.add_css_class("veyra-tag-dot");
                dot.add_css_class(&crate::tags::css_class(color));
                dot.set_valign(gtk4::Align::Center);
                entry.add_prefix(&dot);
                {
                    let refresh_all_tabs = refresh_all_tabs.clone();
                    entry.connect_apply(move |row| {
                        if let Err(err) = veyra_filesystem::set_custom_tag_name(color, &row.text())
                        {
                            tracing::warn!(error = %err, "failed to save custom tag name");
                        }
                        refresh_all_tabs();
                    });
                }
                names_group.add(&entry);
                entry
            })
            .collect(),
    );
    page.add(&names_group);

    let maintenance_group = group(t("prefs.tags.maintenance_group"));

    let (reset_row, reset_button) = action_row_button(
        t("prefs.tags.reset_names"),
        t("prefs.tags.reset_names_subtitle"),
        t("prefs.tags.reset_names_button"),
    );
    {
        let entry_rows = entry_rows.clone();
        let refresh_all_tabs = refresh_all_tabs.clone();
        reset_button.connect_clicked(move |_| {
            if let Err(err) = veyra_filesystem::reset_custom_tag_names() {
                tracing::warn!(error = %err, "failed to reset custom tag names");
            }
            for row in entry_rows.iter() {
                row.set_text("");
            }
            refresh_all_tabs();
        });
    }
    maintenance_group.add(&reset_row);

    let (clear_all_row, clear_all_button) = action_row_button(
        t("prefs.tags.clear_all"),
        t("prefs.tags.clear_all_subtitle"),
        t("prefs.privacy.clear.button"),
    );
    clear_all_button.add_css_class("destructive-action");
    {
        let tags = tags.clone();
        let refresh_all_tabs = refresh_all_tabs.clone();
        clear_all_button.connect_clicked(move |button| {
            let tags = tags.clone();
            let refresh_all_tabs = refresh_all_tabs.clone();
            let confirm = adw::AlertDialog::builder()
                .heading(t("prefs.tags.clear_all_confirm_heading"))
                .body(t("prefs.tags.clear_all_confirm_body"))
                .build();
            confirm.add_responses(&[
                ("cancel", t("prefs.tags.clear_all_cancel")),
                ("clear", t("prefs.privacy.clear.button")),
            ]);
            confirm.set_response_appearance("clear", adw::ResponseAppearance::Destructive);
            confirm.set_default_response(Some("cancel"));
            confirm.set_close_response("cancel");
            confirm.choose(button, gtk4::gio::Cancellable::NONE, move |response| {
                if response != "clear" {
                    return;
                }
                if let Err(err) = veyra_filesystem::clear_all_tags() {
                    tracing::warn!(error = %err, "failed to clear all tags");
                }
                crate::tags::reload(&tags);
                refresh_all_tabs();
            });
        });
    }
    maintenance_group.add(&clear_all_row);

    let (clear_unused_row, clear_unused_button) = action_row_button(
        t("prefs.tags.clear_unused"),
        t("prefs.tags.clear_unused_subtitle"),
        t("prefs.tags.clear_unused_button"),
    );
    {
        let tags = tags.clone();
        let refresh_all_tabs = refresh_all_tabs.clone();
        let clear_unused_row = clear_unused_row.clone();
        clear_unused_button.connect_clicked(move |_| match veyra_filesystem::clear_unused_tags() {
            Ok(removed) => {
                clear_unused_row.set_subtitle(&t_fmt(
                    "prefs.tags.clear_unused_result",
                    &[("count", &removed.to_string())],
                ));
                crate::tags::reload(&tags);
                refresh_all_tabs();
            }
            Err(err) => {
                tracing::warn!(error = %err, "failed to clear unused tags");
            }
        });
    }
    maintenance_group.add(&clear_unused_row);
    page.add(&maintenance_group);

    page
}

fn shortcuts_page(window: &adw::ApplicationWindow) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(t("prefs.page.shortcuts"))
        .icon_name("input-keyboard-symbolic")
        .name("shortcuts")
        .build();
    let g = group(t("prefs.shortcuts.group.keyboard_shortcuts"));

    let (view_row, view_button) = action_row_button(
        t("prefs.shortcuts.view_all.title"),
        t("prefs.shortcuts.view_all.subtitle"),
        t("prefs.shortcuts.view_all.button"),
    );
    {
        let window = window.clone();
        view_button.connect_clicked(move |_| {
            crate::dialogs::shortcuts_help_dialog::show(&window);
        });
    }
    g.add(&view_row);

    let (reset_row, reset_button) = action_row_button(
        t("prefs.shortcuts.reset.title"),
        t("prefs.shortcuts.reset.subtitle"),
        t("prefs.shortcuts.reset.button"),
    );
    {
        let window = window.clone();
        reset_button.connect_clicked(move |_| {
            gtk4::gio::prelude::ActionGroupExt::activate_action(
                &window,
                "win.reset-shortcuts",
                None,
            );
        });
    }
    g.add(&reset_row);

    page.add(&g);
    page
}

fn privacy_page(settings: &SharedSettings) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(t("prefs.page.privacy"))
        .icon_name("security-high-symbolic")
        .name("privacy")
        .build();

    let history_group = group(t("prefs.privacy.group.history"));
    let (clear_files_row, clear_files_button) = action_row_button(
        t("prefs.privacy.clear_files.title"),
        t("prefs.privacy.clear_files.subtitle"),
        t("prefs.privacy.clear.button"),
    );
    {
        clear_files_button.connect_clicked(move |button| {
            crate::dialogs::clear_recent_confirm::show(button, move || {
                recent::clear_history();
            });
        });
    }
    history_group.add(&clear_files_row);

    let (clear_servers_row, clear_servers_button) = action_row_button(
        t("prefs.privacy.clear_servers.title"),
        t("prefs.privacy.clear_servers.subtitle"),
        t("prefs.privacy.clear.button"),
    );
    clear_servers_button.connect_clicked(move |_| {
        network::clear_history();
    });
    history_group.add(&clear_servers_row);

    let remember_row = switch_row(
        t("prefs.privacy.remember_files.title"),
        t("prefs.privacy.remember_files.subtitle"),
        settings.borrow().store_recent_files,
    );
    {
        let settings = settings.clone();
        remember_row.connect_active_notify(move |row| {
            settings.borrow_mut().store_recent_files = row.is_active();
            persist(&settings);
        });
    }
    history_group.add(&remember_row);
    page.add(&history_group);

    let logging_group = group(t("prefs.privacy.group.logging"));
    let sanitize_row = switch_row(
        t("prefs.privacy.sanitize_logs.title"),
        t("prefs.privacy.sanitize_logs.subtitle"),
        settings.borrow().sanitize_log_paths,
    );
    {
        let settings = settings.clone();
        sanitize_row.connect_active_notify(move |row| {
            let enabled = row.is_active();
            settings.borrow_mut().sanitize_log_paths = enabled;
            persist(&settings);
            veyra_core::security::set_sanitize_log_paths(enabled);
        });
    }
    logging_group.add(&sanitize_row);

    let (open_log_dir_row, open_log_dir_button) = action_row_button(
        t("prefs.privacy.open_log_dir.title"),
        t("prefs.privacy.open_log_dir.subtitle"),
        t("prefs.privacy.open_log_dir.button"),
    );
    open_log_dir_button.connect_clicked(|button| {
        let Some(log_dir) = veyra_core::XdgDirs::resolve("veyra")
            .ok()
            .and_then(|dirs| dirs.log_file().parent().map(|p| p.to_path_buf()))
        else {
            return;
        };
        let file = gtk4::gio::File::for_path(&log_dir);
        gtk4::FileLauncher::new(Some(&file)).launch(
            button.root().and_downcast_ref::<gtk4::Window>(),
            gtk4::gio::Cancellable::NONE,
            |result| {
                if let Err(err) = result {
                    tracing::warn!(error = %err, "failed to open log directory");
                }
            },
        );
    });
    logging_group.add(&open_log_dir_row);
    page.add(&logging_group);

    let crash_group = group(t("prefs.privacy.group.crash_reports"));
    let save_crash_reports_row = switch_row(
        t("prefs.privacy.save_crash_reports.title"),
        t("prefs.privacy.save_crash_reports.subtitle"),
        settings.borrow().save_crash_reports,
    );
    {
        let settings = settings.clone();
        save_crash_reports_row.connect_active_notify(move |row| {
            let enabled = row.is_active();
            settings.borrow_mut().save_crash_reports = enabled;
            persist(&settings);
            veyra_core::crash_report::set_crash_reports_enabled(enabled);
        });
    }
    crash_group.add(&save_crash_reports_row);

    let (clear_crash_reports_row, clear_crash_reports_button) = action_row_button(
        t("prefs.privacy.clear_crash_reports.title"),
        t("prefs.privacy.clear_crash_reports.subtitle"),
        t("prefs.privacy.clear.button"),
    );
    clear_crash_reports_button.connect_clicked(|_| {
        let Some(state_dir) = veyra_core::XdgDirs::resolve("veyra")
            .ok()
            .map(|dirs| dirs.crashes_dir())
        else {
            return;
        };
        if let Err(err) = veyra_core::crash_report::clear_all(&state_dir) {
            tracing::warn!(error = %err, "failed to clear crash reports");
        }
    });
    crash_group.add(&clear_crash_reports_row);
    page.add(&crash_group);

    let telemetry_group = group(t("prefs.privacy.group.telemetry"));
    let telemetry_row = adw::ActionRow::builder()
        .title(t("prefs.privacy.telemetry.title"))
        .subtitle(t("prefs.privacy.telemetry.subtitle"))
        .build();
    telemetry_row.add_prefix(&gtk4::Image::from_icon_name("emblem-ok-symbolic"));
    telemetry_group.add(&telemetry_row);
    page.add(&telemetry_group);

    page
}

fn advanced_page(
    dialog: &adw::PreferencesDialog,
    settings: &SharedSettings,
    thumbnails: &Rc<ThumbnailService>,
    preview_widget: &gtk4::Widget,
    refresh_all_tabs: &Rc<dyn Fn()>,
) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(t("prefs.page.advanced"))
        .icon_name("applications-engineering-symbolic")
        .name("advanced")
        .build();

    let developer_group = group(t("prefs.advanced.group.developer"));
    let developer_mode_row = switch_row(
        t("prefs.advanced.developer_mode.title"),
        t("prefs.advanced.developer_mode.subtitle"),
        settings.borrow().developer_mode,
    );
    {
        let settings = settings.clone();
        developer_mode_row.connect_active_notify(move |row| {
            settings.borrow_mut().developer_mode = row.is_active();
            persist(&settings);
        });
    }
    developer_group.add(&developer_mode_row);

    let git_badges_row = switch_row(
        t("prefs.advanced.git_badges.title"),
        t("prefs.advanced.git_badges.subtitle"),
        settings.borrow().show_git_badges,
    );
    {
        let settings = settings.clone();
        let refresh_all_tabs = refresh_all_tabs.clone();
        git_badges_row.connect_active_notify(move |row| {
            settings.borrow_mut().show_git_badges = row.is_active();
            persist(&settings);
            refresh_all_tabs();
        });
    }
    developer_group.add(&git_badges_row);
    page.add(&developer_group);

    page.add(&terminal_group(settings));
    page.add(&system_integration_group());

    let g = group(t("prefs.advanced.group.reset"));

    let (reset_row, reset_button) = action_row_button(
        t("prefs.advanced.reset_all.title"),
        t("prefs.advanced.reset_all.subtitle"),
        t("prefs.advanced.reset_all.button"),
    );
    reset_button.add_css_class("destructive-action");
    {
        let dialog = dialog.clone();
        let settings = settings.clone();
        let thumbnails = thumbnails.clone();
        let preview_widget = preview_widget.clone();
        let refresh_all_tabs = refresh_all_tabs.clone();
        reset_button.connect_clicked(move |button| {
            let dialog = dialog.clone();
            let settings = settings.clone();
            let thumbnails = thumbnails.clone();
            let preview_widget = preview_widget.clone();
            let refresh_all_tabs = refresh_all_tabs.clone();
            let confirm = adw::AlertDialog::builder()
                .heading(t("prefs.advanced.reset_all.confirm_heading"))
                .body(t("prefs.advanced.reset_all.confirm_body"))
                .build();
            confirm.add_responses(&[
                ("cancel", t("prefs.advanced.reset_all.cancel")),
                ("reset", t("prefs.advanced.reset_all.button")),
            ]);
            confirm.set_response_appearance("reset", adw::ResponseAppearance::Destructive);
            confirm.set_default_response(Some("cancel"));
            confirm.set_close_response("cancel");
            confirm.choose(button, gtk4::gio::Cancellable::NONE, move |response| {
                if response != "reset" {
                    return;
                }
                let defaults = crate::config::VeyraSettings::default();
                *settings.borrow_mut() = defaults.clone();
                persist(&settings);
                adw::StyleManager::default().set_color_scheme(defaults.color_scheme.to_adw());
                crate::config::apply_accent_color(defaults.accent_color);
                crate::config::set_date_format(defaults.date_format);
                crate::config::set_size_unit(defaults.size_unit);
                thumbnails.resize_l1(defaults.thumbnail_cache_capacity);
                preview_widget.set_visible(defaults.enable_preview_panel);
                veyra_core::security::set_sanitize_log_paths(defaults.sanitize_log_paths);
                veyra_core::crash_report::set_crash_reports_enabled(defaults.save_crash_reports);
                crate::terminal::set_terminal_pref(
                    defaults.terminal_pref,
                    &defaults.custom_terminal_command,
                );
                veyra_filesystem::set_reflink_enabled(defaults.enable_reflink);
                veyra_filesystem::set_compression_level(defaults.compression_level.level());
                refresh_all_tabs();
                // Simplest correct way to reflect every reset value across
                // every page's widgets is to close and let the next
                // `Ctrl+,` rebuild the dialog from the (now-default)
                // settings, rather than manually re-syncing ~15 rows here.
                dialog.close();
            });
        });
    }
    g.add(&reset_row);
    page.add(&g);
    page
}

/// Faz 65: the Advanced page's terminal emulator picker — `TerminalPref`
/// pins one specific emulator ahead of `terminal::resolve_terminal`'s
/// existing `xdg-terminal-exec`/`$TERMINAL`/GIO-default/known-list chain
/// (Rule #25 stays intact: even `Custom` is a user-supplied override, never
/// a hardcoded terminal). The custom-command entry only shows when
/// `Custom` is selected.
fn terminal_group(settings: &SharedSettings) -> adw::PreferencesGroup {
    let g = group(t("prefs.advanced.group.terminal"));

    let current = settings.borrow().terminal_pref;
    let labels: Vec<&str> = TerminalPref::ALL.iter().map(|p| p.label()).collect();
    let terminal_row = combo_row(
        t("prefs.advanced.terminal.title"),
        &labels,
        TerminalPref::ALL
            .iter()
            .position(|p| *p == current)
            .unwrap_or(0),
    );

    let custom_row = adw::EntryRow::builder()
        .title(t("prefs.advanced.terminal.custom_command"))
        .show_apply_button(true)
        .build();
    custom_row.set_text(&settings.borrow().custom_terminal_command);
    custom_row.set_visible(current == TerminalPref::Custom);
    {
        let settings = settings.clone();
        custom_row.connect_apply(move |row| {
            let command = row.text().to_string();
            settings.borrow_mut().custom_terminal_command = command.clone();
            persist(&settings);
            crate::terminal::set_terminal_pref(settings.borrow().terminal_pref, &command);
        });
    }

    {
        let settings = settings.clone();
        let custom_row = custom_row.clone();
        terminal_row.connect_selected_notify(move |row| {
            if let Some(pref) = TerminalPref::ALL.get(row.selected() as usize) {
                settings.borrow_mut().terminal_pref = *pref;
                persist(&settings);
                custom_row.set_visible(*pref == TerminalPref::Custom);
                crate::terminal::set_terminal_pref(
                    *pref,
                    &settings.borrow().custom_terminal_command,
                );
            }
        });
    }

    g.add(&terminal_row);
    g.add(&custom_row);
    g
}

/// Faz 44: the Advanced page's "Default File Manager" row — status subtitle
/// and a "Set as Default" button, wired to `gio::AppInfo::default_for_type`/
/// `set_as_default_for_type` via `crate::system_integration`. Its own group
/// (not folded into "Developer") since it's ordinary-user-facing, unlike
/// everything else on this page.
fn system_integration_group() -> adw::PreferencesGroup {
    let g = group(t("prefs.advanced.group.system_integration"));

    let row = adw::ActionRow::builder()
        .title(t("prefs.advanced.default_file_manager.title"))
        .subtitle(default_file_manager_subtitle())
        .build();
    let button = gtk4::Button::builder()
        .label(t("prefs.advanced.default_file_manager.button"))
        .valign(gtk4::Align::Center)
        .sensitive(!crate::system_integration::is_default_file_manager())
        .build();
    {
        let row = row.clone();
        let button = button.clone();
        button.connect_clicked(move |button| {
            match crate::system_integration::set_as_default_file_manager() {
                Ok(()) => {
                    row.set_subtitle(default_file_manager_subtitle());
                    button.set_sensitive(!crate::system_integration::is_default_file_manager());
                }
                Err(err) => {
                    tracing::warn!(error = %err, "failed to set Veyra as default file manager");
                    row.set_subtitle(t("prefs.advanced.default_file_manager.error"));
                }
            }
        });
    }
    row.add_suffix(&button);
    g.add(&row);
    g
}

fn default_file_manager_subtitle() -> &'static str {
    if crate::system_integration::is_default_file_manager() {
        t("prefs.advanced.default_file_manager.subtitle_default")
    } else {
        t("prefs.advanced.default_file_manager.subtitle_not_default")
    }
}

fn format_kb(kb: usize) -> String {
    if kb >= 1024 {
        format!("{} MB", kb / 1024)
    } else {
        format!("{kb} KB")
    }
}
