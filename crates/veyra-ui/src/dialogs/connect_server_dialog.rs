//! Faz 21: "Connect to Server" dialog — a server-address entry with a
//! scheme quick-picker (`network::NetworkProtocol`) and a list of
//! previously-connected servers (`network::load_history`). Connecting runs
//! through `network::mount_remote_location`, which is fully async (Rule
//! #11/#12) and hands credential prompts to GTK's native mount-operation
//! dialog rather than anything built here.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use veyra_filesystem::VeyraPath;

use crate::i18n::{t, t_fmt};
use crate::network::{self, NetworkProtocol};

/// Shows the dialog parented to `window`. On a successful connection,
/// `navigate` is called with the mounted location and the dialog closes.
pub(crate) fn show(window: &adw::ApplicationWindow, navigate: Rc<dyn Fn(VeyraPath)>) {
    let dialog = adw::Dialog::builder()
        .title(t("connect.title"))
        .content_width(440)
        .content_height(560)
        .build();

    let header = adw::HeaderBar::new();
    header.set_show_end_title_buttons(false);
    header.set_title_widget(Some(&gtk4::Label::new(Some(t("connect.title")))));

    let cancel_button = gtk4::Button::with_label(t("connect.cancel"));
    header.pack_start(&cancel_button);
    {
        let dialog = dialog.clone();
        cancel_button.connect_clicked(move |_| {
            dialog.close();
        });
    }

    let connect_button = gtk4::Button::with_label(t("connect.connect"));
    connect_button.add_css_class("suggested-action");
    connect_button.set_sensitive(false);
    header.pack_end(&connect_button);
    dialog.set_default_widget(Some(&connect_button));

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_top(16);
    content.set_margin_bottom(16);
    content.set_margin_start(16);
    content.set_margin_end(16);

    let address_label = gtk4::Label::new(Some(t("connect.address_label")));
    address_label.set_xalign(0.0);
    address_label.add_css_class("heading");
    content.append(&address_label);

    let address_entry = gtk4::Entry::new();
    address_entry.set_placeholder_text(Some(t("connect.address_placeholder")));
    address_entry.set_activates_default(true);
    content.append(&address_entry);

    let protocol_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    protocol_box.add_css_class("linked");
    content.append(&protocol_box);

    let error_label = gtk4::Label::new(None);
    error_label.set_xalign(0.0);
    error_label.add_css_class("error");
    error_label.set_wrap(true);
    error_label.set_visible(false);
    content.append(&error_label);

    let status_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let spinner = gtk4::Spinner::new();
    status_box.append(&spinner);
    status_box.append(&gtk4::Label::new(Some(t("connect.connecting"))));
    status_box.set_visible(false);
    content.append(&status_box);

    let selected_protocol = Rc::new(Cell::new(NetworkProtocol::Sftp));
    let mut group_leader: Option<gtk4::ToggleButton> = None;
    for protocol in NetworkProtocol::ALL {
        let button = gtk4::ToggleButton::with_label(protocol.chip_label());
        if let Some(leader) = &group_leader {
            button.set_group(Some(leader));
        }
        if protocol == NetworkProtocol::Sftp {
            button.set_active(true);
        }
        {
            let selected_protocol = selected_protocol.clone();
            let address_entry = address_entry.clone();
            let connect_button = connect_button.clone();
            let error_label = error_label.clone();
            button.connect_toggled(move |toggle| {
                if !toggle.is_active() {
                    return;
                }
                selected_protocol.set(protocol);
                let current = address_entry.text();
                let is_blank_or_bare_scheme = current.is_empty()
                    || NetworkProtocol::ALL
                        .iter()
                        .any(|p| current == format!("{}://", p.scheme()));
                if is_blank_or_bare_scheme {
                    address_entry.set_text(&format!("{}://", protocol.scheme()));
                    address_entry.set_position(-1);
                }
                error_label.set_visible(false);
                connect_button.set_sensitive(
                    network::parse_server_address(&address_entry.text(), selected_protocol.get())
                        .is_ok(),
                );
            });
        }
        protocol_box.append(&button);
        if group_leader.is_none() {
            group_leader = Some(button);
        }
    }

    {
        let connect_button = connect_button.clone();
        let error_label = error_label.clone();
        let selected_protocol = selected_protocol.clone();
        address_entry.connect_changed(move |entry| {
            error_label.set_visible(false);
            connect_button.set_sensitive(
                network::parse_server_address(&entry.text(), selected_protocol.get()).is_ok(),
            );
        });
    }

    content.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
    let recent_label = gtk4::Label::new(Some(t("connect.recent_servers")));
    recent_label.set_xalign(0.0);
    recent_label.add_css_class("heading");
    recent_label.add_css_class("dim-label");
    content.append(&recent_label);

    let recent_list = gtk4::ListBox::new();
    recent_list.set_selection_mode(gtk4::SelectionMode::None);
    recent_list.add_css_class("boxed-list");
    let history = network::load_history();
    recent_label.set_visible(!history.is_empty());
    for uri in &history {
        recent_list.append(&recent_row(
            uri,
            &address_entry,
            &connect_button,
            &recent_list,
        ));
    }
    let recent_scroller = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .min_content_height(120)
        .vexpand(true)
        .child(&recent_list)
        .build();
    recent_scroller.set_visible(!history.is_empty());
    content.append(&recent_scroller);

    {
        let connect_button_inner = connect_button.clone();
        let error_label = error_label.clone();
        let status_box = status_box.clone();
        let spinner = spinner.clone();
        let cancel_button = cancel_button.clone();
        let address_entry = address_entry.clone();
        let selected_protocol = selected_protocol.clone();
        let window = window.clone();
        let dialog = dialog.clone();
        connect_button.connect_clicked(move |_| {
            let connect_button = &connect_button_inner;
            let parsed =
                match network::parse_server_address(&address_entry.text(), selected_protocol.get())
                {
                    Ok(parsed) => parsed,
                    Err(err) => {
                        error_label.set_text(&err.to_string());
                        error_label.set_visible(true);
                        return;
                    }
                };

            error_label.set_visible(false);
            address_entry.set_sensitive(false);
            connect_button.set_sensitive(false);
            cancel_button.set_sensitive(false);
            spinner.set_spinning(true);
            status_box.set_visible(true);

            let navigate = navigate.clone();
            let dialog_success = dialog.clone();

            let address_entry_err = address_entry.clone();
            let connect_button_err = connect_button.clone();
            let cancel_button_err = cancel_button.clone();
            let spinner_err = spinner.clone();
            let status_box_err = status_box.clone();
            let error_label_err = error_label.clone();

            network::mount_remote_location(
                &window,
                parsed.uri,
                move |path| {
                    navigate(path);
                    dialog_success.close();
                },
                move |message| {
                    spinner_err.set_spinning(false);
                    status_box_err.set_visible(false);
                    address_entry_err.set_sensitive(true);
                    connect_button_err.set_sensitive(true);
                    cancel_button_err.set_sensitive(true);
                    error_label_err.set_text(&message);
                    error_label_err.set_visible(true);
                },
            );
        });
    }

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&content));
    dialog.set_child(Some(&toolbar_view));

    dialog.present(Some(window));
}

/// One row of the recent-servers list: clicking it fills the address entry
/// (letting the user tweak the port/path before connecting), and its
/// trailing button removes the entry from history.
fn recent_row(
    uri: &str,
    address_entry: &gtk4::Entry,
    connect_button: &gtk4::Button,
    recent_list: &gtk4::ListBox,
) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::new();

    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    content.set_margin_top(4);
    content.set_margin_bottom(4);
    content.set_margin_start(8);
    content.set_margin_end(8);

    let icon_name = network::detect_protocol(uri)
        .map(NetworkProtocol::icon_name)
        .unwrap_or("network-server-symbolic");
    content.append(&gtk4::Image::from_icon_name(icon_name));

    let label = gtk4::Label::new(Some(uri));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    content.append(&label);

    let remove_button = gtk4::Button::from_icon_name("edit-delete-symbolic");
    remove_button.add_css_class("flat");
    remove_button.set_tooltip_text(Some(t("connect.remove_from_history")));
    remove_button.update_property(&[gtk4::accessible::Property::Label(&t_fmt(
        "connect.remove_uri_from_history",
        &[("uri", uri)],
    ))]);
    {
        let uri = uri.to_string();
        let row = row.clone();
        let recent_list = recent_list.clone();
        remove_button.connect_clicked(move |_| {
            network::remove_history_entry(&uri);
            recent_list.remove(&row);
        });
    }
    content.append(&remove_button);

    row.set_child(Some(&content));

    let gesture = gtk4::GestureClick::new();
    {
        let uri = uri.to_string();
        let address_entry = address_entry.clone();
        let connect_button = connect_button.clone();
        gesture.connect_released(move |_, _, _, _| {
            address_entry.set_text(&uri);
            address_entry.set_position(-1);
            connect_button.set_sensitive(true);
        });
    }
    row.add_controller(gesture);

    row
}
