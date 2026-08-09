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
    pub breadcrumbs_box: gtk4::Box,
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

    let nav_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    nav_box.add_css_class("linked");
    nav_box.append(&back_button);
    nav_box.append(&forward_button);
    nav_box.append(&up_button);
    widget.pack_start(&nav_box);

    let breadcrumbs_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    breadcrumbs_box.set_halign(gtk4::Align::Center);
    widget.set_title_widget(Some(&breadcrumbs_box));

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
        breadcrumbs_box,
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
