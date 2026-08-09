use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;

use crate::views::ViewMode;

/// Handles the window needs after the header bar is built: the navigation
/// buttons (their sensitivity is updated on every navigation) and the
/// breadcrumbs container (rebuilt on every navigation).
#[derive(Clone)]
pub(crate) struct HeaderBarHandles {
    pub widget: adw::HeaderBar,
    pub back_button: gtk4::Button,
    pub forward_button: gtk4::Button,
    pub up_button: gtk4::Button,
    pub home_button: gtk4::Button,
    pub refresh_button: gtk4::Button,
    pub breadcrumbs_box: gtk4::Box,
    /// Swaps between the breadcrumbs row and the editable address entry.
    pub title_stack: gtk4::Stack,
    pub address_entry: gtk4::Entry,
}

pub(crate) fn build(
    view_stack: &gtk4::Stack,
    search_query: Rc<RefCell<String>>,
    filter: gtk4::CustomFilter,
) -> HeaderBarHandles {
    let widget = adw::HeaderBar::new();

    let back_button = nav_button("go-previous-symbolic", "Go Back");
    let forward_button = nav_button("go-next-symbolic", "Go Forward");
    let up_button = nav_button("go-up-symbolic", "Go Up");
    let home_button = nav_button("go-home-symbolic", "Go Home");
    let refresh_button = nav_button("view-refresh-symbolic", "Refresh (F5)");

    let nav_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    nav_box.add_css_class("linked");
    nav_box.append(&back_button);
    nav_box.append(&forward_button);
    nav_box.append(&up_button);
    nav_box.append(&home_button);
    widget.pack_start(&nav_box);
    widget.pack_start(&refresh_button);

    let breadcrumbs_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    breadcrumbs_box.set_halign(gtk4::Align::Center);

    let address_entry = gtk4::Entry::new();
    address_entry.set_hexpand(true);
    address_entry.set_tooltip_text(Some("Enter Location (Enter to go, Esc to cancel)"));
    address_entry.update_property(&[gtk4::accessible::Property::Label("Address")]);

    let title_stack = gtk4::Stack::new();
    title_stack.set_hexpand(true);
    title_stack.add_named(&breadcrumbs_box, Some("breadcrumbs"));
    title_stack.add_named(&address_entry, Some("address"));
    title_stack.set_visible_child_name("breadcrumbs");
    widget.set_title_widget(Some(&title_stack));

    // Clicking empty space in the breadcrumbs row (not on a segment button,
    // since GestureClick's default TARGET phase only sees events aimed at
    // this widget itself) switches to address-entry mode.
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

    // Esc cancels address entry and returns to the breadcrumbs view.
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

    // Losing focus without pressing Enter also reverts to breadcrumbs mode.
    let address_focus = gtk4::EventControllerFocus::new();
    {
        let title_stack = title_stack.clone();
        address_focus.connect_leave(move |_| {
            title_stack.set_visible_child_name("breadcrumbs");
        });
    }
    address_entry.add_controller(address_focus);

    let search_entry = gtk4::SearchEntry::new();
    search_entry.set_hexpand(true);
    search_entry.connect_search_changed(move |entry| {
        *search_query.borrow_mut() = entry.text().to_string();
        filter.changed(gtk4::FilterChange::Different);
    });

    let search_toggle = gtk4::ToggleButton::new();
    search_toggle.set_icon_name("system-search-symbolic");
    search_toggle.set_tooltip_text(Some("Search Directory (Ctrl+F)"));
    search_toggle.update_property(&[gtk4::accessible::Property::Label("Search Directory")]);
    {
        let widget = widget.clone();
        let breadcrumbs_box = breadcrumbs_box.clone();
        let search_entry = search_entry.clone();
        search_toggle.connect_toggled(move |toggle| {
            if toggle.is_active() {
                widget.set_title_widget(Some(&search_entry));
                search_entry.grab_focus();
            } else {
                search_entry.set_text("");
                widget.set_title_widget(Some(&breadcrumbs_box));
            }
        });
    }
    widget.pack_end(&search_toggle);

    widget.pack_end(&view_switcher(view_stack));

    HeaderBarHandles {
        widget,
        back_button,
        forward_button,
        up_button,
        home_button,
        refresh_button,
        breadcrumbs_box,
        title_stack,
        address_entry,
    }
}

fn nav_button(icon_name: &str, tooltip: &str) -> gtk4::Button {
    let button = gtk4::Button::from_icon_name(icon_name);
    button.set_tooltip_text(Some(tooltip));
    button.update_property(&[gtk4::accessible::Property::Label(tooltip)]);
    button
}

fn view_switcher(view_stack: &gtk4::Stack) -> gtk4::Box {
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

        let view_stack = view_stack.clone();
        button.connect_toggled(move |btn| {
            if btn.is_active() {
                view_stack.set_visible_child_name(mode.stack_name());
            }
        });

        box_.append(&button);
        if group_leader.is_none() {
            group_leader = Some(button);
        }
    }

    box_
}
