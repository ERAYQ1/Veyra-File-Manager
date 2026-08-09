//! Faz 8: two independent panels (Left/Right) side by side in a resizable
//! `GtkPaned`. Each panel owns a complete Faz 3-7 navigation chrome (back/
//! forward/up/home/refresh, breadcrumbs, status bar) plus its own
//! `AdwTabView` — so the two panels are fully independent: separate
//! location, history, tabs, and view state, mirroring Dolphin/Krusader-style
//! dual-pane file managers. Only the window-level search entry and
//! view-mode switcher (in the header bar) act on whichever panel currently
//! has focus — see `window::focus_panel`.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;

use crate::tab_page::{active_tab, TabPage, TabRegistry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PanelId {
    Left,
    Right,
}

impl PanelId {
    /// The panel on the opposite side — the destination for "Copy/Move to
    /// Other Panel" when `self` is the focused (source) panel.
    pub(crate) fn other(self) -> Self {
        match self {
            PanelId::Left => PanelId::Right,
            PanelId::Right => PanelId::Left,
        }
    }
}

/// The navigation chrome a single panel owns outright: nav buttons,
/// breadcrumbs/address entry, and the item-count/free-space status row.
/// Unlike Faz 7's window-wide chrome, every field here belongs to exactly
/// one panel, so panels never need to coordinate about who gets to write to
/// them — a panel's own back/forward/refresh always act on that panel,
/// regardless of which panel currently has focus.
#[derive(Clone)]
pub(crate) struct Chrome {
    pub back_button: gtk4::Button,
    pub forward_button: gtk4::Button,
    pub up_button: gtk4::Button,
    pub home_button: gtk4::Button,
    pub refresh_button: gtk4::Button,
    pub breadcrumbs_box: gtk4::Box,
    pub title_stack: gtk4::Stack,
    pub address_entry: gtk4::Entry,
    pub status_left: gtk4::Label,
    pub status_right: gtk4::Label,
}

/// One independent panel: its own navigation chrome, its own `AdwTabView`
/// (and therefore its own tabs, each carrying the Faz 7 per-tab isolated
/// state), and the outer `frame` used for the active-panel highlight
/// (`install_panel_css`) and click-to-focus detection.
#[derive(Clone)]
pub(crate) struct Panel {
    pub id: PanelId,
    pub tab_view: adw::TabView,
    pub registry: TabRegistry,
    pub chrome: Chrome,
    pub frame: gtk4::Box,
}

impl Panel {
    pub fn set_highlighted(&self, active: bool) {
        if active {
            self.frame.add_css_class("veyra-active-panel");
        } else {
            self.frame.remove_css_class("veyra-active-panel");
        }
    }
}

#[derive(Clone)]
pub(crate) struct Panels {
    pub left: Panel,
    pub right: Panel,
}

impl Panels {
    pub fn get(&self, id: PanelId) -> &Panel {
        match id {
            PanelId::Left => &self.left,
            PanelId::Right => &self.right,
        }
    }
}

/// The `TabPage` behind whichever panel is currently focused, if it has any
/// tabs open yet (the right panel starts with zero tabs until Faz 8's split
/// view is activated for the first time).
pub(crate) fn focused_tab(panels: &Panels, focused: &Rc<RefCell<PanelId>>) -> Option<TabPage> {
    let panel = panels.get(*focused.borrow());
    active_tab(&panel.tab_view, &panel.registry)
}

/// Builds one panel's full navigation chrome + tab strip: back/forward/up/
/// home/refresh buttons, a breadcrumbs row that swaps into an editable
/// address entry on click (Esc/focus-out reverts), an `AdwTabBar` +
/// `AdwTabView`, and a status row (item count / free space). Mirrors the
/// single-panel header bar Faz 3-7 built, just scoped to one panel instead
/// of the whole window.
pub(crate) fn build_panel(id: PanelId) -> Panel {
    let back_button = nav_button("go-previous-symbolic", "Go Back");
    let forward_button = nav_button("go-next-symbolic", "Go Forward");
    let up_button = nav_button("go-up-symbolic", "Go Up");
    let home_button = nav_button("go-home-symbolic", "Go Home");
    let refresh_button = nav_button("view-refresh-symbolic", "Refresh (F5)");

    let nav_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    nav_box.add_css_class("linked");
    nav_box.set_margin_start(4);
    nav_box.set_margin_top(4);
    nav_box.set_margin_bottom(4);
    nav_box.append(&back_button);
    nav_box.append(&forward_button);
    nav_box.append(&up_button);
    nav_box.append(&home_button);
    nav_box.append(&refresh_button);

    let breadcrumbs_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    breadcrumbs_box.set_halign(gtk4::Align::Start);
    breadcrumbs_box.set_hexpand(true);
    breadcrumbs_box.set_margin_start(8);

    let address_entry = gtk4::Entry::new();
    address_entry.set_hexpand(true);
    address_entry.set_margin_start(4);
    address_entry.set_margin_end(4);
    address_entry.set_tooltip_text(Some("Enter Location (Enter to go, Esc to cancel)"));
    address_entry.update_property(&[gtk4::accessible::Property::Label("Address")]);

    let title_stack = gtk4::Stack::new();
    title_stack.set_hexpand(true);
    title_stack.add_named(&breadcrumbs_box, Some("breadcrumbs"));
    title_stack.add_named(&address_entry, Some("address"));
    title_stack.set_visible_child_name("breadcrumbs");

    let breadcrumbs_click = gtk4::GestureClick::new();
    {
        let title_stack = title_stack.clone();
        let address_entry = address_entry.clone();
        breadcrumbs_click.connect_released(move |gesture, _, _, _| {
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            title_stack.set_visible_child_name("address");
            address_entry.grab_focus();
            address_entry.select_region(0, -1);
        });
    }
    breadcrumbs_box.add_controller(breadcrumbs_click);

    let address_key = gtk4::EventControllerKey::new();
    {
        let title_stack = title_stack.clone();
        let breadcrumbs_box = breadcrumbs_box.clone();
        address_key.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == gtk4::gdk::Key::Escape {
                title_stack.set_visible_child_name("breadcrumbs");
                breadcrumbs_box.grab_focus();
                gtk4::glib::Propagation::Stop
            } else {
                gtk4::glib::Propagation::Proceed
            }
        });
    }
    address_entry.add_controller(address_key);

    let address_focus = gtk4::EventControllerFocus::new();
    {
        let title_stack = title_stack.clone();
        address_focus.connect_leave(move |_| {
            title_stack.set_visible_child_name("breadcrumbs");
        });
    }
    address_entry.add_controller(address_focus);

    let toolbar_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    toolbar_row.append(&nav_box);
    toolbar_row.append(&title_stack);

    let tab_view = adw::TabView::new();
    let tab_bar = adw::TabBar::new();
    tab_bar.set_view(Some(&tab_view));
    tab_view.set_vexpand(true);

    let status = crate::statusbar::build();

    let frame = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    frame.add_css_class("veyra-panel");
    frame.append(&toolbar_row);
    frame.append(&tab_bar);
    frame.append(&tab_view);
    frame.append(&status.widget);

    let chrome = Chrome {
        back_button,
        forward_button,
        up_button,
        home_button,
        refresh_button,
        breadcrumbs_box,
        title_stack,
        address_entry,
        status_left: status.left_label,
        status_right: status.right_label,
    };

    Panel {
        id,
        tab_view,
        registry: TabRegistry::default(),
        chrome,
        frame,
    }
}

fn nav_button(icon_name: &str, tooltip: &str) -> gtk4::Button {
    let button = gtk4::Button::from_icon_name(icon_name);
    button.set_tooltip_text(Some(tooltip));
    button.update_property(&[gtk4::accessible::Property::Label(tooltip)]);
    button
}

/// Loads the CSS that draws the active-panel highlight border and installs
/// it for `display`. `.veyra-panel` always reserves the same 2px
/// transparent border, so toggling `.veyra-active-panel` on/off never
/// shifts layout.
pub(crate) fn install_panel_css(display: &gtk4::gdk::Display) {
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(
        ".veyra-panel { border: 2px solid transparent; }\n\
         .veyra-panel.veyra-active-panel { border: 2px solid @accent_color; border-radius: 6px; }",
    );
    gtk4::style_context_add_provider_for_display(
        display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn other_panel_swaps_left_and_right() {
        assert_eq!(PanelId::Left.other(), PanelId::Right);
        assert_eq!(PanelId::Right.other(), PanelId::Left);
    }

    #[test]
    fn other_panel_is_involutive() {
        assert_eq!(PanelId::Left.other().other(), PanelId::Left);
        assert_eq!(PanelId::Right.other().other(), PanelId::Right);
    }
}
