//! Faz 22: "Open With…" dialog — replaces the deprecated
//! `GtkAppChooserDialog` placeholder in `window.rs`. Two `GtkListBox`
//! sections (Recommended Applications, from `open_with::recommended_apps`;
//! All Applications, from `open_with::all_apps`) both re-filter live against
//! a `GtkSearchEntry`, matching `open_with::matches_query`. Each list keeps
//! its own filtered `Vec<AppInfo>` in `Rc<RefCell<_>>` alongside the
//! `GtkListBox`, since a row's position in that vector after filtering is
//! exactly its `ListBoxRow` index — the cheapest way to recover which
//! `AppInfo` a `row-selected`/`row-activated` signal refers to without
//! reaching for `ObjectExt::set_data`.
//!
//! Selecting a row (single click) arms the "Open" button; activating one
//! (double-click/Enter) opens immediately. Both paths run the same
//! `do_open`, which honors the "Always use this application" checkbox by
//! calling `open_with::set_default` before `open_with::launch`.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{gio, glib};
use libadwaita as adw;
use libadwaita::prelude::*;

use veyra_filesystem::VeyraPath;

use crate::open_with;

/// Shows the Open With dialog for `path`, whose content type is
/// `content_type`. On failure to launch the chosen application, `on_error`
/// is called with a heading/body pair suitable for an `AdwAlertDialog`
/// (Rule #15/#18) — the caller owns how that's presented.
pub(crate) fn show(
    window: &adw::ApplicationWindow,
    path: &VeyraPath,
    content_type: &str,
    on_error: impl Fn(&str, &str) + 'static,
) {
    let file = path.to_gio_file();
    let content_type = content_type.to_string();
    let default_app = open_with::default_app(&content_type);
    let default_id = default_app.as_ref().and_then(gio::AppInfo::id);

    let dialog = adw::Dialog::builder()
        .title("Open With")
        .content_width(440)
        .content_height(560)
        .build();

    let header = adw::HeaderBar::new();
    header.set_show_end_title_buttons(false);
    header.set_title_widget(Some(&gtk4::Label::new(Some("Open With"))));

    let cancel_button = gtk4::Button::with_label("Cancel");
    header.pack_start(&cancel_button);
    {
        let dialog = dialog.clone();
        cancel_button.connect_clicked(move |_| {
            dialog.close();
        });
    }

    let open_button = gtk4::Button::with_label("Open");
    open_button.add_css_class("suggested-action");
    open_button.set_sensitive(false);
    header.pack_end(&open_button);
    dialog.set_default_widget(Some(&open_button));

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    content.set_margin_top(16);
    content.set_margin_bottom(16);
    content.set_margin_start(16);
    content.set_margin_end(16);

    let search_entry = gtk4::SearchEntry::new();
    search_entry.set_placeholder_text(Some("Search Applications…"));
    content.append(&search_entry);

    let recommended_label = gtk4::Label::new(Some("Recommended Applications"));
    recommended_label.set_xalign(0.0);
    recommended_label.add_css_class("heading");
    content.append(&recommended_label);

    let recommended_list = gtk4::ListBox::new();
    recommended_list.set_selection_mode(gtk4::SelectionMode::Single);
    recommended_list.set_activate_on_single_click(false);
    recommended_list.add_css_class("boxed-list");
    content.append(&recommended_list);

    let all_label = gtk4::Label::new(Some("All Applications"));
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

    let default_checkbox = gtk4::CheckButton::with_label(&format!(
        "Always use this application for {content_type} files"
    ));
    content.append(&default_checkbox);

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&content));
    dialog.set_child(Some(&toolbar_view));

    let recommended_apps = open_with::recommended_apps(&content_type);
    let all_apps = open_with::all_apps();

    let recommended_current: Rc<RefCell<Vec<gio::AppInfo>>> = Rc::new(RefCell::new(Vec::new()));
    let all_current: Rc<RefCell<Vec<gio::AppInfo>>> = Rc::new(RefCell::new(Vec::new()));
    let selected: Rc<RefCell<Option<gio::AppInfo>>> = Rc::new(RefCell::new(None));

    let do_open: Rc<dyn Fn(gio::AppInfo)> = {
        let file = file.clone();
        let content_type = content_type.clone();
        let default_checkbox = default_checkbox.clone();
        let window = window.clone();
        let dialog = dialog.clone();
        Rc::new(move |app: gio::AppInfo| {
            if default_checkbox.is_active() {
                if let Err(err) = open_with::set_default(&app, &content_type) {
                    tracing::warn!(error = %err, "failed to set default application for type");
                }
            }
            if let Err(err) = open_with::launch(&app, &file, &window) {
                on_error("Unable to Open File", &err.to_string());
            }
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
        let default_id = default_id.clone();
        let selected = selected.clone();
        let open_button = open_button.clone();
        move |query: &str| {
            populate_list(
                &recommended_list,
                &recommended_apps,
                query,
                default_id.as_ref(),
                &recommended_current,
            );
            populate_list(
                &all_list,
                &all_apps,
                query,
                default_id.as_ref(),
                &all_current,
            );
            *selected.borrow_mut() = None;
            open_button.set_sensitive(false);
        }
    };
    refilter("");

    {
        let refilter = refilter.clone();
        search_entry.connect_search_changed(move |entry| {
            refilter(&entry.text());
        });
    }

    connect_list(
        &recommended_list,
        &all_list,
        &recommended_current,
        &selected,
        &open_button,
        &do_open,
    );
    connect_list(
        &all_list,
        &recommended_list,
        &all_current,
        &selected,
        &open_button,
        &do_open,
    );

    {
        let selected = selected.clone();
        let do_open = do_open.clone();
        open_button.connect_clicked(move |_| {
            if let Some(app) = selected.borrow().clone() {
                do_open(app);
            }
        });
    }

    dialog.present(Some(window));
}

/// Wires a `GtkListBox`'s `row-selected` (arm the Open button, clear the
/// sibling list's selection) and `row-activated` (open immediately) signals.
/// `current` is the filtered `Vec<AppInfo>` `populate_list` last wrote for
/// this exact list, so a row's index in it recovers the `AppInfo`.
fn connect_list(
    list: &gtk4::ListBox,
    sibling: &gtk4::ListBox,
    current: &Rc<RefCell<Vec<gio::AppInfo>>>,
    selected: &Rc<RefCell<Option<gio::AppInfo>>>,
    open_button: &gtk4::Button,
    do_open: &Rc<dyn Fn(gio::AppInfo)>,
) {
    {
        let sibling = sibling.clone();
        let current = current.clone();
        let selected = selected.clone();
        let open_button = open_button.clone();
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
            open_button.set_sensitive(true);
        });
    }
    {
        let current = current.clone();
        let do_open = do_open.clone();
        list.connect_row_activated(move |_, row| {
            let index = row.index();
            if let Some(app) = (index >= 0)
                .then(|| current.borrow().get(index as usize).cloned())
                .flatten()
            {
                do_open(app);
            }
        });
    }
}

/// Clears `list` and rebuilds it from `source` filtered by `query`
/// (`open_with::matches_query` against each app's name/description),
/// recording the exact filtered order into `current` so `connect_list` can
/// map a row index back to an `AppInfo`.
fn populate_list(
    list: &gtk4::ListBox,
    source: &[gio::AppInfo],
    query: &str,
    default_id: Option<&glib::GString>,
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
        let is_default = default_id.is_some_and(|id| app.id().as_deref() == Some(id.as_str()));
        list.append(&build_app_row(app, is_default));
    }

    *current.borrow_mut() = filtered;
}

/// Builds one application row: icon, name, description subtitle (when the
/// desktop file provides one), and a "Default" badge when `is_default`.
fn build_app_row(app: &gio::AppInfo, is_default: bool) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(app.name()).build();

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

    if is_default {
        let badge = gtk4::Label::new(Some("Default"));
        badge.add_css_class("dim-label");
        badge.add_css_class("caption");
        row.add_suffix(&badge);
    }

    row
}
