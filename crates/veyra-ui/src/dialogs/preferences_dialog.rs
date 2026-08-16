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
    ClickPolicy, ColorSchemePref, DefaultViewMode, IconSizePref, SharedSettings,
    MAX_PREVIEW_SIZE_CHOICES_KB, STREAM_CHUNK_SIZE_CHOICES, THUMBNAIL_CACHE_CAPACITY_CHOICES,
};
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
) {
    let dialog = adw::PreferencesDialog::builder()
        .title("Preferences")
        .content_width(560)
        .content_height(600)
        .search_enabled(true)
        .build();

    dialog.add(&appearance_page(&settings, &refresh_all_tabs));
    dialog.add(&navigation_page(&settings));
    dialog.add(&files_page(&settings));
    dialog.add(&search_page(&settings, &rebuild_search_index));
    dialog.add(&preview_page(&settings, &preview_widget));
    dialog.add(&performance_page(&settings, &thumbnails, &refresh_all_tabs));
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
        .title("Appearance")
        .icon_name("preferences-desktop-theme-symbolic")
        .name("appearance")
        .build();
    let g = group("Theme & Icons");

    let current = settings.borrow().color_scheme;
    let theme_row = combo_row(
        "Theme",
        &["System Default", "Light", "Dark"],
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

    let current = settings.borrow().icon_size;
    let icon_size_labels: Vec<&str> = IconSizePref::ALL.iter().map(|s| s.label()).collect();
    let icon_size_row = combo_row(
        "Icon Size",
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

    page.add(&g);
    page
}

fn navigation_page(settings: &SharedSettings) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Navigation")
        .icon_name("go-next-symbolic")
        .name("navigation")
        .build();

    let g = group("Opening Items");
    let current = settings.borrow().click_policy;
    let click_row = combo_row(
        "Click Policy",
        &["Double-Click to Open", "Single-Click to Open"],
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
        "Open Folders in New Tab",
        "Middle-click and sidebar navigation already do this; applies here too on your next launch",
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
    page.add(&g);

    let tabs_group = group("Tabs");
    let restore_row = switch_row(
        "Restore Previous Tabs on Startup",
        "Reopen every tab that was open when Veyra last closed",
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
        .title("Files")
        .icon_name("folder-symbolic")
        .name("files")
        .build();

    let display_group = group("Display Defaults");
    display_group.set_description(Some(
        "Applies to newly opened tabs; open tabs keep their own setting.",
    ));

    let hidden_row = switch_row(
        "Show Hidden Files",
        "New tabs start with hidden files visible",
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
        "Folders First",
        "New tabs list folders before files",
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
        "Default View Mode",
        &["Icons", "Compact", "Details"],
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

    let confirm_group = group("Confirmations");
    let confirm_trash_row = switch_row(
        "Confirm Before Emptying Trash",
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
        "Confirm Before Permanently Deleting",
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
    page.add(&confirm_group);

    page
}

fn search_page(
    settings: &SharedSettings,
    rebuild_search_index: &Rc<dyn Fn()>,
) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Search")
        .icon_name("system-search-symbolic")
        .name("search")
        .build();
    let g = group("Indexing");

    let enable_row = switch_row(
        "Enable Fast Search Indexer",
        "SQLite + FTS5 index backing name:/type:/size:/modified: searches",
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
    max_results_row.set_title("Max Search Results");
    max_results_row.set_value(settings.borrow().max_search_results as f64);
    {
        let settings = settings.clone();
        max_results_row.connect_value_notify(move |row| {
            settings.borrow_mut().max_search_results = row.value() as usize;
            persist(&settings);
        });
    }
    g.add(&max_results_row);

    let (rebuild_row, rebuild_button) = action_row_button(
        "Rebuild Search Index",
        "Re-scans your home directory from scratch",
        "Rebuild",
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
        .title("Preview")
        .icon_name("view-reveal-symbolic")
        .name("preview")
        .build();
    let g = group("Preview Panel");

    let enable_row = switch_row(
        "Enable Preview Panel",
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
        "Preview Size Limit",
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
        "Show Folder Item Count",
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
    page
}

fn performance_page(
    settings: &SharedSettings,
    thumbnails: &Rc<ThumbnailService>,
    refresh_all_tabs: &Rc<dyn Fn()>,
) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Performance")
        .icon_name("speedometer-symbolic")
        .name("performance")
        .build();
    let g = group("Directory Loading");

    let current = settings.borrow().stream_chunk_size;
    let chunk_labels: Vec<String> = STREAM_CHUNK_SIZE_CHOICES
        .iter()
        .map(|n| format!("{n} files"))
        .collect();
    let chunk_labels_ref: Vec<&str> = chunk_labels.iter().map(String::as_str).collect();
    let chunk_row = combo_row(
        "Directory Stream Chunk Size",
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
        .map(|n| format!("{n} thumbnails"))
        .collect();
    let cache_labels_ref: Vec<&str> = cache_labels.iter().map(String::as_str).collect();
    let cache_row = combo_row(
        "Thumbnail Cache Capacity",
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
    page
}

fn shortcuts_page(window: &adw::ApplicationWindow) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Shortcuts")
        .icon_name("input-keyboard-symbolic")
        .name("shortcuts")
        .build();
    let g = group("Keyboard Shortcuts");

    let (view_row, view_button) = action_row_button(
        "View All Shortcuts",
        "Every action and its current key binding",
        "View",
    );
    {
        let window = window.clone();
        view_button.connect_clicked(move |_| {
            crate::dialogs::shortcuts_help_dialog::show(&window);
        });
    }
    g.add(&view_row);

    let (reset_row, reset_button) = action_row_button(
        "Reset Shortcuts to Default",
        "Discards any customization in shortcuts.json",
        "Reset",
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
        .title("Privacy")
        .icon_name("security-high-symbolic")
        .name("privacy")
        .build();

    let history_group = group("History");
    let (clear_files_row, clear_files_button) = action_row_button(
        "Clear Recent Files History",
        "Empties the XDG recently-used registry",
        "Clear",
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
        "Clear Recent Servers",
        "Empties the \"Connect to Server\" address history",
        "Clear",
    );
    clear_servers_button.connect_clicked(move |_| {
        network::clear_history();
    });
    history_group.add(&clear_servers_row);

    let remember_row = switch_row(
        "Remember Recently Used Files",
        "Off also disables the Recent Files history above",
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

    let logging_group = group("Logging");
    let sanitize_row = switch_row(
        "Sanitize File Paths in Logs",
        "Log only file names instead of full paths",
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
    page.add(&logging_group);

    let telemetry_group = group("Telemetry");
    let telemetry_row = adw::ActionRow::builder()
        .title("Zero Telemetry")
        .subtitle("Veyra never sends usage data or file contents anywhere (Kural #24)")
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
        .title("Advanced")
        .icon_name("applications-engineering-symbolic")
        .name("advanced")
        .build();
    let g = group("Reset");

    let (reset_row, reset_button) = action_row_button(
        "Reset All Settings to Default",
        "Restores every page on this dialog to its default value",
        "Reset All",
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
                .heading("Reset all settings to default?")
                .body("Every Preferences page reverts to its default value. This cannot be undone.")
                .build();
            confirm.add_responses(&[("cancel", "Cancel"), ("reset", "Reset All")]);
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
                thumbnails.resize_l1(defaults.thumbnail_cache_capacity);
                preview_widget.set_visible(defaults.enable_preview_panel);
                veyra_core::security::set_sanitize_log_paths(defaults.sanitize_log_paths);
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

fn format_kb(kb: usize) -> String {
    if kb >= 1024 {
        format!("{} MB", kb / 1024)
    } else {
        format!("{kb} KB")
    }
}
