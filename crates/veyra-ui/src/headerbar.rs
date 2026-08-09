use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;

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
    pub split_toggle_button: gtk4::ToggleButton,
    /// One toggle button per view mode, for syncing the active state to
    /// whichever tab is currently active in the focused panel.
    pub view_switcher_buttons: Vec<(ViewMode, gtk4::ToggleButton)>,
}

/// `panels`/`focused` let the search entry and view-mode switcher act on
/// whichever panel currently has focus, since both the search filter and
/// the view stack are per-tab state owned by a specific panel (Faz 7/8).
pub(crate) fn build(panels: &Panels, focused: Rc<RefCell<PanelId>>) -> HeaderBarHandles {
    let widget = adw::HeaderBar::new();

    let search_entry = gtk4::SearchEntry::new();
    search_entry.set_hexpand(true);
    {
        let panels = panels.clone();
        let focused = focused.clone();
        search_entry.connect_search_changed(move |entry| {
            let Some(tab) = focused_tab(&panels, &focused) else {
                return;
            };
            *tab.search_query.borrow_mut() = entry.text().to_string();
            tab.filter.changed(gtk4::FilterChange::Different);
        });
    }

    let search_toggle = gtk4::ToggleButton::new();
    search_toggle.set_icon_name("system-search-symbolic");
    search_toggle.set_tooltip_text(Some("Search Focused Panel (Ctrl+F)"));
    search_toggle.update_property(&[gtk4::accessible::Property::Label("Search Directory")]);
    {
        let widget = widget.clone();
        let search_entry = search_entry.clone();
        search_toggle.connect_toggled(move |toggle| {
            if toggle.is_active() {
                widget.set_title_widget(Some(&search_entry));
                search_entry.grab_focus();
            } else {
                search_entry.set_text("");
                widget.set_title_widget(None::<&gtk4::Widget>);
            }
        });
    }
    widget.pack_end(&search_toggle);

    let view_switcher_buttons = view_switcher(&widget, panels, focused);

    let split_toggle_button = gtk4::ToggleButton::new();
    split_toggle_button.set_icon_name("sidebar-show-right-symbolic");
    split_toggle_button.set_tooltip_text(Some("Toggle Split View (F3)"));
    split_toggle_button.update_property(&[gtk4::accessible::Property::Label("Toggle Split View")]);
    split_toggle_button.set_action_name(Some("win.toggle-split-view"));
    widget.pack_start(&split_toggle_button);

    HeaderBarHandles {
        widget,
        split_toggle_button,
        view_switcher_buttons,
    }
}

/// Builds the Icon/Compact/Details toggle group and packs it into `header`.
/// Each toggle acts on the focused panel's active tab.
fn view_switcher(
    header: &adw::HeaderBar,
    panels: &Panels,
    focused: Rc<RefCell<PanelId>>,
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
        button.connect_toggled(move |btn| {
            if btn.is_active() {
                if let Some(tab) = focused_tab(&panels, &focused) {
                    tab.view_stack.set_visible_child_name(mode.stack_name());
                }
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
