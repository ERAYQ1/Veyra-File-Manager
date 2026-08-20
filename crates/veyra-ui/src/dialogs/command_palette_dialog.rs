//! Faz 24: the Command Palette overlay (`Ctrl+K` / `Ctrl+Shift+P`) — a
//! Spotlight-style `AdwDialog` listing every `win.*` command in
//! `command_palette::all_commands()`, filtered live by
//! `command_palette::filter_commands`'s scored fuzzy search.
//!
//! Keyboard-first by design (Rule #29): focus starts and stays in the search
//! entry so the user can keep typing while `Up`/`Down` move a programmatic
//! selection cursor (`selected_index`) rather than GTK focus — moving real
//! focus into the list would steal further keystrokes from the entry.
//! `Enter` activates whichever row is currently selected; `Escape` closes
//! the dialog via `AdwDialog`'s own default close shortcut, so it needs no
//! handling here.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::command_palette::{self, CommandItem};
use crate::i18n::t;
use crate::shortcuts;

/// Shows the Command Palette over `window`. Selecting a command (click,
/// Enter, or double-click) closes the palette and activates that command's
/// action directly on `window` — the same `gtk_widget_activate_action`
/// mechanism the real menu items and accelerators use, so there is exactly
/// one code path per command rather than a second copy of its logic.
pub(crate) fn show(window: &adw::ApplicationWindow) {
    let all_commands = Rc::new(command_palette::all_commands());

    let dialog = adw::Dialog::builder()
        .title(t("palette.title"))
        .content_width(560)
        .content_height(480)
        .build();

    let search_entry = gtk4::SearchEntry::new();
    search_entry.set_placeholder_text(Some(t("palette.search_placeholder")));
    search_entry.set_margin_top(12);
    search_entry.set_margin_bottom(4);
    search_entry.set_margin_start(12);
    search_entry.set_margin_end(12);

    let list = gtk4::ListBox::new();
    list.set_selection_mode(gtk4::SelectionMode::Browse);
    list.add_css_class("boxed-list");
    list.set_activate_on_single_click(true);

    let scroller = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vexpand(true)
        .child(&list)
        .build();

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    content.append(&search_entry);
    content.append(&scroller);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.set_content(Some(&content));
    dialog.set_child(Some(&toolbar_view));

    // The filtered/ranked commands currently shown, in on-screen row order —
    // a row's index in the `ListBox` is exactly its index here, mirroring
    // `open_with_dialog`'s `current` bookkeeping.
    let current: Rc<RefCell<Vec<CommandItem>>> = Rc::new(RefCell::new(Vec::new()));
    let selected_index: Rc<Cell<i32>> = Rc::new(Cell::new(-1));

    {
        let current = current.clone();
        list.set_header_func(move |row, _before| {
            let index = row.index();
            if index < 0 {
                row.set_header(None::<&gtk4::Widget>);
                return;
            }
            let items = current.borrow();
            let Some(item) = items.get(index as usize) else {
                row.set_header(None::<&gtk4::Widget>);
                return;
            };
            let is_new_group = index == 0
                || items
                    .get(index as usize - 1)
                    .is_none_or(|previous| previous.category != item.category);
            row.set_header(
                is_new_group
                    .then(|| category_label(item.display_category()))
                    .as_ref(),
            );
        });
    }

    let app = window.application();

    let populate = {
        let list = list.clone();
        let all_commands = all_commands.clone();
        let current = current.clone();
        let selected_index = selected_index.clone();
        let app = app.clone();
        move |query: &str| {
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }

            let filtered: Vec<CommandItem> = command_palette::filter_commands(query, &all_commands)
                .into_iter()
                .cloned()
                .collect();
            for item in &filtered {
                list.append(&build_row(item, app.as_ref()));
            }
            list.invalidate_headers();

            let has_rows = !filtered.is_empty();
            *current.borrow_mut() = filtered;
            selected_index.set(if has_rows { 0 } else { -1 });
            if let Some(row) = list.row_at_index(0) {
                list.select_row(Some(&row));
            }
        }
    };
    populate("");

    {
        let populate = populate.clone();
        search_entry.connect_search_changed(move |entry| {
            populate(&entry.text());
        });
    }

    let activate_selected: Rc<dyn Fn()> = {
        let window = window.clone();
        let dialog = dialog.clone();
        let current = current.clone();
        let selected_index = selected_index.clone();
        Rc::new(move || {
            let index = selected_index.get();
            if index < 0 {
                return;
            }
            let Some(item) = current.borrow().get(index as usize).cloned() else {
                return;
            };
            dialog.close();
            if let Err(err) = gtk4::prelude::WidgetExt::activate_action(
                &window,
                item.action_name,
                item.action_target.as_ref(),
            ) {
                tracing::warn!(
                    action = item.action_name,
                    error = %err,
                    "command palette: failed to activate action"
                );
            }
        })
    };

    let move_selection = {
        let list = list.clone();
        let scroller = scroller.clone();
        let current = current.clone();
        let selected_index = selected_index.clone();
        move |delta: i32| {
            let len = current.borrow().len() as i32;
            if len == 0 {
                return;
            }
            let next = (selected_index.get() + delta).clamp(0, len - 1);
            selected_index.set(next);
            if let Some(row) = list.row_at_index(next) {
                list.select_row(Some(&row));
                scroll_row_into_view(&scroller, &row);
            }
        }
    };

    let key_controller = gtk4::EventControllerKey::new();
    {
        let move_selection = move_selection.clone();
        key_controller.connect_key_pressed(move |_, keyval, _, _| match keyval {
            gtk4::gdk::Key::Down => {
                move_selection(1);
                glib::Propagation::Stop
            }
            gtk4::gdk::Key::Up => {
                move_selection(-1);
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        });
    }
    search_entry.add_controller(key_controller);

    {
        let activate_selected = activate_selected.clone();
        search_entry.connect_activate(move |_| activate_selected());
    }

    {
        let selected_index = selected_index.clone();
        list.connect_row_selected(move |_, row| {
            if let Some(row) = row {
                selected_index.set(row.index());
            }
        });
    }

    list.connect_row_activated(move |_, _| activate_selected());

    dialog.connect_show(move |_| {
        search_entry.grab_focus();
    });

    dialog.present(Some(window));
}

/// Scrolls `scroller` just far enough that `row` is fully within its
/// viewport — used to keep the programmatically-moved keyboard selection
/// visible without ever moving real widget focus off the search entry.
fn scroll_row_into_view(scroller: &gtk4::ScrolledWindow, row: &gtk4::ListBoxRow) {
    let Some(bounds) = row.compute_bounds(scroller) else {
        return;
    };
    let adjustment = scroller.vadjustment();
    adjustment.clamp_page(bounds.y() as f64, (bounds.y() + bounds.height()) as f64);
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

/// Builds one command row: action icon + title on the left, category and
/// (when the command has one) a keyboard-shortcut badge on the right — read
/// live from `app`'s accel table (`Faz 25 ShortcutMap`) rather than a copy
/// baked into `CommandItem`, so a customized or reset shortcut is reflected
/// here immediately.
fn build_row(item: &CommandItem, app: Option<&gtk4::Application>) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(item.display_title())
        .build();
    row.set_use_markup(false);
    row.set_activatable(true);
    row.set_widget_name(item.id);

    let icon = gtk4::Image::from_icon_name(item.icon_name);
    row.add_prefix(&icon);

    let detailed_action_name = match item.action_target.as_ref().and_then(glib::Variant::str) {
        Some(target) => format!("{}::{target}", item.action_name),
        None => item.action_name.to_string(),
    };
    let accel = app
        .map(|app| app.accels_for_action(&detailed_action_name))
        .and_then(|accels| accels.first().map(|accel| shortcuts::format_accel(accel)));

    if let Some(accel) = accel {
        let shortcut_label = gtk4::Label::new(Some(&accel));
        shortcut_label.add_css_class("dim-label");
        shortcut_label.add_css_class("caption");
        row.add_suffix(&shortcut_label);
    }

    row
}
