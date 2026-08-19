//! Faz 27: File Associations dialog — a searchable list of MIME types with
//! their current default applications. Clicking "Change…" on any type opens
//! a minimal app-picker overlay (with no auto-launch) to set that type's
//! default handler, persisting to XDG mimeapps.list via `open_with::set_default`.
//! The picker is factored into `pick_default_app` so it can be reused from
//! Properties dialog's "Default Application" row for a single file.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::file_associations;
use crate::i18n::t;
use crate::open_with;

/// Shows the File Associations dialog over `window`, listing all registered
/// MIME types with their current defaults and allowing the user to change
/// the default application for each type.
pub(crate) fn show(window: &adw::ApplicationWindow) {
    let dialog = adw::Dialog::builder()
        .title(t("file_assoc.title"))
        .content_width(520)
        .content_height(640)
        .build();

    let header = adw::HeaderBar::new();
    header.set_show_end_title_buttons(false);
    header.set_title_widget(Some(&gtk4::Label::new(Some(t("file_assoc.title")))));

    let close_button = gtk4::Button::from_icon_name("window-close-symbolic");
    close_button.set_tooltip_text(Some(t("file_assoc.close")));
    close_button.update_property(&[gtk4::accessible::Property::Label(t("file_assoc.close"))]);
    header.pack_end(&close_button);
    {
        let dialog = dialog.clone();
        close_button.connect_clicked(move |_| {
            dialog.close();
        });
    }

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    content.set_margin_top(16);
    content.set_margin_bottom(16);
    content.set_margin_start(16);
    content.set_margin_end(16);

    let search_entry = gtk4::SearchEntry::new();
    search_entry.set_placeholder_text(Some(t("file_assoc.search_placeholder")));
    content.append(&search_entry);

    let list = gtk4::ListBox::new();
    list.set_selection_mode(gtk4::SelectionMode::None);
    list.add_css_class("boxed-list");
    let scroller = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vexpand(true)
        .child(&list)
        .build();
    content.append(&scroller);

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&content));
    dialog.set_child(Some(&toolbar_view));

    let file_types = file_associations::list_system_file_types();
    let current: Rc<RefCell<Vec<file_associations::FileTypeEntry>>> =
        Rc::new(RefCell::new(Vec::new()));

    let refilter = {
        let list = list.clone();
        let file_types = file_types.clone();
        let current = current.clone();
        let window = window.clone();
        move |query: &str| {
            populate_list(&list, &file_types, query, &current, &window);
        }
    };
    refilter("");

    {
        let refilter = refilter.clone();
        search_entry.connect_search_changed(move |entry| {
            refilter(&entry.text());
        });
    }

    dialog.present(Some(window));
}

/// Clears `list` and rebuilds it from `file_types` filtered by `query`,
/// recording the exact filtered order into `current` for future row updates.
fn populate_list(
    list: &gtk4::ListBox,
    file_types: &[file_associations::FileTypeEntry],
    query: &str,
    current: &Rc<RefCell<Vec<file_associations::FileTypeEntry>>>,
    window: &adw::ApplicationWindow,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let filtered: Vec<file_associations::FileTypeEntry> = file_types
        .iter()
        .filter(|entry| {
            let default_app_name = entry.default_app.as_ref().map(|app| app.name());
            let default_app_name = default_app_name.as_ref().map(|s| s.as_str());
            file_associations::matches_query(entry, default_app_name, query)
        })
        .cloned()
        .collect();

    for entry in &filtered {
        list.append(&build_file_type_row(entry, window));
    }

    *current.borrow_mut() = filtered;
}

/// Builds one file type row: icon, description + content_type subtitle, and a
/// "Change…" button to set the default application.
fn build_file_type_row(
    entry: &file_associations::FileTypeEntry,
    window: &adw::ApplicationWindow,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(entry.description.clone())
        .subtitle(entry.content_type.clone())
        .title_lines(1)
        .subtitle_lines(1)
        .build();
    row.set_use_markup(false);

    // Prefix: file type icon or generic fallback
    let icon = entry
        .icon
        .as_ref()
        .map(gtk4::Image::from_gicon)
        .unwrap_or_else(|| gtk4::Image::from_icon_name("text-x-generic-symbolic"));
    icon.set_pixel_size(32);
    row.add_prefix(&icon);

    // Suffix: current default app name or "None", plus "Change…" button
    let suffix_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    suffix_box.set_halign(gtk4::Align::End);
    suffix_box.set_valign(gtk4::Align::Center);

    let app_label = gtk4::Label::new(None);
    app_label.add_css_class("dim-label");
    app_label.add_css_class("caption");
    update_app_label(&app_label, &entry.default_app);
    suffix_box.append(&app_label);

    let change_button = gtk4::Button::with_label(t("file_assoc.change"));
    change_button.add_css_class("flat");
    change_button.set_valign(gtk4::Align::Center);

    let content_type = entry.content_type.clone();
    let window = window.clone();
    let app_label = app_label.clone();
    change_button.connect_clicked(move |_| {
        let on_changed = {
            let content_type = content_type.clone();
            let app_label = app_label.clone();
            Rc::new(move |app: Option<gio::AppInfo>| {
                if let Some(app_ref) = app.as_ref() {
                    if let Err(err) = open_with::set_default(app_ref, &content_type) {
                        tracing::warn!(error = %err, "failed to set default application");
                    }
                }
                update_app_label(&app_label, &app);
            })
        };
        pick_default_app(&window, &content_type, on_changed);
    });
    suffix_box.append(&change_button);

    row.add_suffix(&suffix_box);

    row
}

/// Updates the app label to show the app's name or "None" if no default.
fn update_app_label(label: &gtk4::Label, app: &Option<gio::AppInfo>) {
    let text = app
        .as_ref()
        .map(|a| a.name().to_string())
        .unwrap_or_else(|| t("file_assoc.none").to_string());
    label.set_text(&text);
}

/// Opens a minimal app-picker overlay (no auto-launch, only set-default) for
/// `content_type`. When the user selects an app (or cancels), `on_changed` is
/// called with the new default app (or None). This function is factored out
/// so it can be reused from Properties dialog's "Default Application" row.
pub(crate) fn pick_default_app(
    parent: &impl IsA<gtk4::Widget>,
    content_type: &str,
    on_changed: Rc<dyn Fn(Option<gio::AppInfo>)>,
) {
    let content_type = content_type.to_string();

    let dialog = adw::Dialog::builder()
        .title(t("file_assoc.select_application"))
        .content_width(400)
        .content_height(480)
        .build();

    let header = adw::HeaderBar::new();
    header.set_show_end_title_buttons(false);
    header.set_title_widget(Some(&gtk4::Label::new(Some(t(
        "file_assoc.select_application",
    )))));

    let cancel_button = gtk4::Button::with_label(t("open_with.cancel"));
    header.pack_start(&cancel_button);
    {
        let dialog = dialog.clone();
        cancel_button.connect_clicked(move |_| {
            dialog.close();
        });
    }

    let select_button = gtk4::Button::with_label(t("file_assoc.set_default"));
    select_button.add_css_class("suggested-action");
    select_button.set_sensitive(false);
    header.pack_end(&select_button);
    dialog.set_default_widget(Some(&select_button));

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    content.set_margin_top(16);
    content.set_margin_bottom(16);
    content.set_margin_start(16);
    content.set_margin_end(16);

    let search_entry = gtk4::SearchEntry::new();
    search_entry.set_placeholder_text(Some(t("open_with.search_placeholder")));
    content.append(&search_entry);

    let recommended_label = gtk4::Label::new(Some(t("open_with.recommended")));
    recommended_label.set_xalign(0.0);
    recommended_label.add_css_class("heading");
    content.append(&recommended_label);

    let recommended_list = gtk4::ListBox::new();
    recommended_list.set_selection_mode(gtk4::SelectionMode::Single);
    recommended_list.set_activate_on_single_click(false);
    recommended_list.add_css_class("boxed-list");
    content.append(&recommended_list);

    let all_label = gtk4::Label::new(Some(t("open_with.all_applications")));
    all_label.set_xalign(0.0);
    all_label.add_css_class("heading");
    all_label.add_css_class("dim-label");
    content.append(&all_label);

    let all_list = gtk4::ListBox::new();
    all_list.set_selection_mode(gtk4::SelectionMode::Single);
    all_list.set_activate_on_single_click(false);
    all_list.add_css_class("boxed-list");
    let all_scroller = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .min_content_height(160)
        .vexpand(true)
        .child(&all_list)
        .build();
    content.append(&all_scroller);

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&content));
    dialog.set_child(Some(&toolbar_view));

    let recommended_apps = open_with::recommended_apps(&content_type);
    let all_apps = open_with::all_apps();

    let recommended_current: Rc<RefCell<Vec<gio::AppInfo>>> = Rc::new(RefCell::new(Vec::new()));
    let all_current: Rc<RefCell<Vec<gio::AppInfo>>> = Rc::new(RefCell::new(Vec::new()));
    let selected: Rc<RefCell<Option<gio::AppInfo>>> = Rc::new(RefCell::new(None));

    let do_set_default: Rc<dyn Fn(Option<gio::AppInfo>)> = {
        let dialog = dialog.clone();
        Rc::new(move |app: Option<gio::AppInfo>| {
            on_changed(app);
            dialog.close();
        })
    };

    let refilter = {
        let recommended_list = recommended_list.clone();
        let all_list = all_list.clone();
        let recommended_apps = recommended_apps.clone();
        let all_apps = all_apps.clone();
        let recommended_current = recommended_current.clone();
        let all_current = all_current.clone();
        let selected = selected.clone();
        let select_button = select_button.clone();
        move |query: &str| {
            populate_app_list(
                &recommended_list,
                &recommended_apps,
                query,
                &recommended_current,
            );
            populate_app_list(&all_list, &all_apps, query, &all_current);
            *selected.borrow_mut() = None;
            select_button.set_sensitive(false);
        }
    };
    refilter("");

    {
        let refilter = refilter.clone();
        search_entry.connect_search_changed(move |entry| {
            refilter(&entry.text());
        });
    }

    connect_app_list(
        &recommended_list,
        &all_list,
        &recommended_current,
        &selected,
        &select_button,
        &do_set_default,
    );
    connect_app_list(
        &all_list,
        &recommended_list,
        &all_current,
        &selected,
        &select_button,
        &do_set_default,
    );

    {
        let selected = selected.clone();
        let do_set_default = do_set_default.clone();
        select_button.connect_clicked(move |_| {
            if let Some(app) = selected.borrow().clone() {
                do_set_default(Some(app));
            }
        });
    }

    dialog.present(Some(parent));
}

/// Wires a `GtkListBox`'s `row-selected` (arm the Set Default button, clear
/// the sibling list's selection) and `row-activated` (set default immediately)
/// signals for the app picker.
fn connect_app_list(
    list: &gtk4::ListBox,
    sibling: &gtk4::ListBox,
    current: &Rc<RefCell<Vec<gio::AppInfo>>>,
    selected: &Rc<RefCell<Option<gio::AppInfo>>>,
    select_button: &gtk4::Button,
    do_set_default: &Rc<dyn Fn(Option<gio::AppInfo>)>,
) {
    {
        let sibling = sibling.clone();
        let current = current.clone();
        let selected = selected.clone();
        let select_button = select_button.clone();
        list.connect_row_selected(move |_, row| {
            let Some(row) = row else { return };
            let index = row.index();
            let Some(app) = (index >= 0)
                .then(|| current.borrow().get(index as usize).cloned())
                .flatten()
            else {
                return;
            };
            sibling.unselect_all();
            *selected.borrow_mut() = Some(app);
            select_button.set_sensitive(true);
        });
    }
    {
        let current = current.clone();
        let do_set_default = do_set_default.clone();
        list.connect_row_activated(move |_, row| {
            let index = row.index();
            if let Some(app) = (index >= 0)
                .then(|| current.borrow().get(index as usize).cloned())
                .flatten()
            {
                do_set_default(Some(app));
            }
        });
    }
}

/// Clears `list` and rebuilds it from `source` filtered by `query`.
fn populate_app_list(
    list: &gtk4::ListBox,
    source: &[gio::AppInfo],
    query: &str,
    current: &Rc<RefCell<Vec<gio::AppInfo>>>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let filtered: Vec<gio::AppInfo> = source
        .iter()
        .filter(|app| {
            let description = app.description().unwrap_or_default();
            open_with::matches_query(&app.name(), &description, query)
        })
        .cloned()
        .collect();

    for app in &filtered {
        list.append(&build_app_row(app));
    }

    *current.borrow_mut() = filtered;
}

/// Builds one application row: icon, name, description subtitle.
fn build_app_row(app: &gio::AppInfo) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(app.name()).build();
    row.set_use_markup(false);
    row.set_title_lines(1);
    row.set_subtitle_lines(2);

    if let Some(description) = app.description() {
        if !description.is_empty() {
            row.set_subtitle(&description);
        }
    }

    let icon = app
        .icon()
        .map(|icon| gtk4::Image::from_gicon(&icon))
        .unwrap_or_else(|| gtk4::Image::from_icon_name("application-x-executable-symbolic"));
    icon.set_pixel_size(32);
    row.add_prefix(&icon);

    row
}
