//! Faz 25: the Keyboard Shortcuts help window (`Ctrl+?`, `win.show-
//! shortcuts-help`) — a read-only `AdwDialog` listing every `win.*` action
//! from `shortcuts::catalog()`, grouped by category, with each row's
//! accelerator read live from the `Application`'s own accel table (via
//! `Application::accels_for_action`) so it always matches whatever
//! `shortcuts.json` currently has configured — including a fresh "Reset to
//! Default" (Rule: never a second, driftable copy of the same fact).

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::shortcuts;

/// Shows the Keyboard Shortcuts help window over `window`.
pub(crate) fn show(window: &adw::ApplicationWindow) {
    let app = window.application();

    let dialog = adw::Dialog::builder()
        .title("Keyboard Shortcuts")
        .content_width(480)
        .content_height(560)
        .build();

    let list = gtk4::ListBox::new();
    list.set_selection_mode(gtk4::SelectionMode::None);
    list.add_css_class("boxed-list");

    let scroller = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vexpand(true)
        .child(&list)
        .build();

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    content.append(&scroller);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(12);

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.set_content(Some(&content));
    dialog.set_child(Some(&toolbar_view));

    let mut last_category: Option<&str> = None;
    for entry in shortcuts::catalog() {
        if last_category != Some(entry.category) {
            list.append(&category_label(entry.category));
            last_category = Some(entry.category);
        }
        list.append(&build_row(&entry, app.as_ref()));
    }

    dialog.present(Some(window));
}

fn category_label(category: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(category));
    label.set_xalign(0.0);
    label.set_margin_top(8);
    label.set_margin_bottom(2);
    label.add_css_class("heading");
    label.add_css_class("dim-label");
    label
}

fn build_row(entry: &shortcuts::ShortcutEntry, app: Option<&gtk4::Application>) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(entry.label).build();
    row.set_use_markup(false);
    row.set_activatable(false);

    let accel = app
        .map(|app| app.accels_for_action(entry.action))
        .and_then(|accels| accels.first().map(|accel| shortcuts::format_accel(accel)));

    let shortcut_label = gtk4::Label::new(Some(accel.as_deref().unwrap_or("—")));
    shortcut_label.add_css_class("dim-label");
    shortcut_label.add_css_class("caption");
    row.add_suffix(&shortcut_label);

    row
}
