use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use gtk4::prelude::*;
use libadwaita as adw;

use veyra_filesystem::VeyraPath;
use veyra_search::SearchIndex;

use crate::search_results;
use crate::sorting::{QuickFilter, SortKey, SortOrder};
use crate::split_view::{focused_tab, PanelId, Panels};
use crate::views::ViewMode;

/// The window-level header bar (Faz 8: back/forward/up/home/refresh and
/// breadcrumbs moved into each panel's own chrome — see `split_view.rs`;
/// only search and the view-mode switcher remain here, since both stay
/// meaningful as single, window-wide controls acting on whichever panel is
/// currently focused).
#[derive(Clone)]
pub(crate) struct HeaderBarHandles {
    pub widget: adw::HeaderBar,
    pub search_toggle: gtk4::ToggleButton,
    pub split_toggle_button: gtk4::ToggleButton,
    pub preview_toggle_button: gtk4::ToggleButton,
    /// One toggle button per view mode, for syncing the active state to
    /// whichever tab is currently active in the focused panel.
    pub view_switcher_buttons: Vec<(ViewMode, gtk4::ToggleButton)>,
}

/// `panels`/`focused` let the search entry and view-mode switcher act on
/// whichever panel currently has focus, since both the search filter and
/// the view stack are per-tab state owned by a specific panel (Faz 7/8).
///
/// `search_index`/`navigate` back the Faz 9 advanced search syntax
/// (`name:`/`type:`/`size:`/`modified:`): when the typed query uses that
/// syntax, it's run against the SQLite+FTS5 index instead of Faz 3's plain
/// per-tab filename filter, and results are shown inline under the entry;
/// clicking one calls `navigate` on the focused panel.
pub(crate) fn build(
    panels: &Panels,
    focused: Rc<RefCell<PanelId>>,
    search_index: Arc<SearchIndex>,
    navigate: Rc<dyn Fn(VeyraPath)>,
    refresh_preview: Rc<dyn Fn()>,
) -> HeaderBarHandles {
    let widget = adw::HeaderBar::new();

    let search_entry = gtk4::SearchEntry::new();
    search_entry.set_hexpand(true);

    let results = search_results::build();

    {
        let panels = panels.clone();
        let focused = focused.clone();
        let search_index = search_index.clone();
        let results = results.clone();
        let navigate = navigate.clone();
        search_entry.connect_search_changed(move |entry| {
            let Some(tab) = focused_tab(&panels, &focused) else {
                return;
            };
            let text = entry.text().to_string();
            *tab.search_query.borrow_mut() = text.clone();
            tab.filter.changed(gtk4::FilterChange::Different);

            let parsed = veyra_search::parse(&text);
            if !parsed.has_filters() {
                results.revealer.set_reveal_child(false);
                return;
            }

            let search_index = search_index.clone();
            let results = results.clone();
            let navigate = navigate.clone();
            crate::fs_async::run_blocking(
                move || {
                    search_index
                        .search(&parsed, chrono::Utc::now())
                        .unwrap_or_default()
                },
                move |found| {
                    search_results::update(&results, &found, navigate.clone());
                    results.revealer.set_reveal_child(true);
                },
            );
        });
    }

    let search_toggle = gtk4::ToggleButton::new();
    search_toggle.set_icon_name("system-search-symbolic");
    search_toggle.set_tooltip_text(Some("Search Focused Panel (Ctrl+F)"));
    search_toggle.update_property(&[gtk4::accessible::Property::Label("Search Directory")]);
    {
        let widget = widget.clone();
        let search_column = search_column(&search_entry, &results);
        search_toggle.connect_toggled(move |toggle| {
            if toggle.is_active() {
                widget.set_title_widget(Some(&search_column));
                search_entry.grab_focus();
            } else {
                search_entry.set_text("");
                results.revealer.set_reveal_child(false);
                widget.set_title_widget(None::<&gtk4::Widget>);
            }
        });
    }
    widget.pack_end(&search_toggle);

    let view_switcher_buttons = view_switcher(&widget, panels, focused.clone(), refresh_preview);

    let sort_filter_button = build_sort_filter_button(panels, focused);
    widget.pack_end(&sort_filter_button);

    let split_toggle_button = gtk4::ToggleButton::new();
    split_toggle_button.set_icon_name("sidebar-show-right-symbolic");
    split_toggle_button.set_tooltip_text(Some("Toggle Split View (F3)"));
    split_toggle_button.update_property(&[gtk4::accessible::Property::Label("Toggle Split View")]);
    split_toggle_button.set_action_name(Some("win.toggle-split-view"));
    widget.pack_start(&split_toggle_button);

    let preview_toggle_button = gtk4::ToggleButton::new();
    preview_toggle_button.set_icon_name("view-preview-symbolic");
    preview_toggle_button.set_tooltip_text(Some("Toggle Preview (F9)"));
    preview_toggle_button.update_property(&[gtk4::accessible::Property::Label("Toggle Preview")]);
    preview_toggle_button.set_action_name(Some("win.toggle-preview"));
    widget.pack_end(&preview_toggle_button);

    HeaderBarHandles {
        widget,
        search_toggle,
        split_toggle_button,
        preview_toggle_button,
        view_switcher_buttons,
    }
}

/// Stacks the search entry above its (initially hidden) results list, as a
/// single widget suitable for `AdwHeaderBar::set_title_widget`.
fn search_column(
    search_entry: &gtk4::SearchEntry,
    results: &search_results::SearchResultsHandles,
) -> gtk4::Box {
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    column.append(search_entry);
    column.append(&results.revealer);
    column
}

/// Builds the Icon/Compact/Details toggle group and packs it into `header`.
/// Each toggle acts on the focused panel's active tab.
fn view_switcher(
    header: &adw::HeaderBar,
    panels: &Panels,
    focused: Rc<RefCell<PanelId>>,
    refresh_preview: Rc<dyn Fn()>,
) -> Vec<(ViewMode, gtk4::ToggleButton)> {
    let box_ = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    box_.add_css_class("linked");

    let modes = [
        (ViewMode::Icon, "view-grid-symbolic", "Icon View"),
        (
            ViewMode::Compact,
            "view-continuous-symbolic",
            "Compact View",
        ),
        (ViewMode::Details, "view-list-symbolic", "Details View"),
    ];

    let mut buttons = Vec::with_capacity(modes.len());
    let group_leader: Option<gtk4::ToggleButton> = None;
    let mut group_leader = group_leader;

    for (mode, icon, tooltip) in modes {
        let button = gtk4::ToggleButton::new();
        button.set_icon_name(icon);
        button.set_tooltip_text(Some(tooltip));
        button.update_property(&[gtk4::accessible::Property::Label(tooltip)]);
        if let Some(leader) = &group_leader {
            button.set_group(Some(leader));
        }
        if mode == ViewMode::Icon {
            button.set_active(true);
        }

        let panels = panels.clone();
        let focused = focused.clone();
        let refresh_preview = refresh_preview.clone();
        button.connect_toggled(move |btn| {
            if btn.is_active() {
                if let Some(tab) = focused_tab(&panels, &focused) {
                    tab.view_stack.set_visible_child_name(mode.stack_name());
                }
                refresh_preview();
            }
        });

        box_.append(&button);
        buttons.push((mode, button.clone()));
        if group_leader.is_none() {
            group_leader = Some(button);
        }
    }

    header.pack_end(&box_);
    buttons
}

/// Builds the Faz 13 Sort & Filter menu button: sort criterion, direction,
/// folders-first, and quick file-type filter, all acting on the focused
/// panel's active tab and reflected back live from Details column-header
/// clicks (`TabPage::resort`).
fn build_sort_filter_button(panels: &Panels, focused: Rc<RefCell<PanelId>>) -> gtk4::MenuButton {
    let button = gtk4::MenuButton::new();
    button.set_icon_name("view-sort-ascending-symbolic");
    button.set_tooltip_text(Some("Sort & Filter"));
    button.update_property(&[gtk4::accessible::Property::Label("Sort and Filter Options")]);

    // Guards the toggle handlers below while the popover's widget states are
    // being programmatically synced from the focused tab's state on open,
    // so that sync doesn't itself mutate the tab it's reading from.
    let syncing = Rc::new(Cell::new(false));

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    content.set_margin_top(8);
    content.set_margin_bottom(8);
    content.set_margin_start(8);
    content.set_margin_end(8);
    content.set_width_request(220);

    content.append(&section_label("Sort By"));
    let mut sort_key_buttons = Vec::with_capacity(SortKey::ALL.len());
    let mut group_leader: Option<gtk4::CheckButton> = None;
    for key in SortKey::ALL {
        let check = gtk4::CheckButton::with_label(key.label());
        if let Some(leader) = &group_leader {
            check.set_group(Some(leader));
        }
        {
            let panels = panels.clone();
            let focused = focused.clone();
            let syncing = syncing.clone();
            check.connect_toggled(move |btn| {
                if syncing.get() || !btn.is_active() {
                    return;
                }
                if let Some(tab) = focused_tab(&panels, &focused) {
                    tab.sort_config.borrow_mut().key = key;
                    tab.resort();
                }
            });
        }
        content.append(&check);
        if group_leader.is_none() {
            group_leader = Some(check.clone());
        }
        sort_key_buttons.push((key, check));
    }

    content.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
    content.append(&section_label("Direction"));
    let ascending_check = gtk4::CheckButton::with_label("Ascending");
    let descending_check = gtk4::CheckButton::with_label("Descending");
    descending_check.set_group(Some(&ascending_check));
    for (order, check) in [
        (SortOrder::Ascending, &ascending_check),
        (SortOrder::Descending, &descending_check),
    ] {
        let panels = panels.clone();
        let focused = focused.clone();
        let syncing = syncing.clone();
        check.connect_toggled(move |btn| {
            if syncing.get() || !btn.is_active() {
                return;
            }
            if let Some(tab) = focused_tab(&panels, &focused) {
                tab.sort_config.borrow_mut().order = order;
                tab.resort();
            }
        });
        content.append(check);
    }

    content.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
    let folders_first_check = gtk4::CheckButton::with_label("Folders First");
    {
        let panels = panels.clone();
        let focused = focused.clone();
        let syncing = syncing.clone();
        folders_first_check.connect_toggled(move |btn| {
            if syncing.get() {
                return;
            }
            if let Some(tab) = focused_tab(&panels, &focused) {
                tab.sort_config.borrow_mut().folders_first = btn.is_active();
                tab.resort();
            }
        });
    }
    content.append(&folders_first_check);

    content.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
    content.append(&section_label("Filter By"));
    let mut quick_filter_buttons = Vec::with_capacity(QuickFilter::ALL.len());
    let mut group_leader: Option<gtk4::CheckButton> = None;
    for quick_filter in QuickFilter::ALL {
        let check = gtk4::CheckButton::with_label(quick_filter.label());
        if let Some(leader) = &group_leader {
            check.set_group(Some(leader));
        }
        {
            let panels = panels.clone();
            let focused = focused.clone();
            let syncing = syncing.clone();
            check.connect_toggled(move |btn| {
                if syncing.get() || !btn.is_active() {
                    return;
                }
                if let Some(tab) = focused_tab(&panels, &focused) {
                    *tab.quick_filter.borrow_mut() = quick_filter;
                    tab.refresh_filter();
                }
            });
        }
        content.append(&check);
        if group_leader.is_none() {
            group_leader = Some(check.clone());
        }
        quick_filter_buttons.push((quick_filter, check));
    }

    let popover = gtk4::Popover::new();
    popover.set_child(Some(&content));
    {
        let panels = panels.clone();
        let focused = focused.clone();
        popover.connect_show(move |_| {
            let Some(tab) = focused_tab(&panels, &focused) else {
                return;
            };
            let config = *tab.sort_config.borrow();
            let active_filter = *tab.quick_filter.borrow();

            syncing.set(true);
            for (key, check) in &sort_key_buttons {
                check.set_active(*key == config.key);
            }
            ascending_check.set_active(config.order == SortOrder::Ascending);
            descending_check.set_active(config.order == SortOrder::Descending);
            folders_first_check.set_active(config.folders_first);
            for (quick_filter, check) in &quick_filter_buttons {
                check.set_active(*quick_filter == active_filter);
            }
            syncing.set(false);
        });
    }
    button.set_popover(Some(&popover));

    button
}

fn section_label(text: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_margin_top(4);
    label.add_css_class("heading");
    label
}
