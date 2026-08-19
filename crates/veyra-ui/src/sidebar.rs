use std::rc::Rc;

use gtk4::glib::UserDirectory;
use gtk4::prelude::*;
use gtk4::{gdk, gio, glib};
use libadwaita as adw;
use libadwaita::prelude::*;

use veyra_filesystem::VeyraPath;

use crate::bookmarks::{self, Bookmark};
use crate::devices::{self, DeviceEntry};
use crate::dialogs;
use crate::dnd::{self, DropExecutor};
use crate::fs_async;
use crate::i18n::t;
use crate::network;
use crate::thumbnails::ThumbnailService;
use crate::undo::SharedUndoStack;

/// Builds the Places + Bookmarks + Devices sidebar. Places are the standard
/// XDG user directories (resolved via GLib so localized folder names are
/// respected) plus the virtual `trash://` and `recent://` GIO locations.
/// Bookmarks (Faz 16) are the user's `~/.config/gtk-3.0/bookmarks` entries,
/// kept live via a `GFileMonitor` so edits from Veyra or any other
/// application stay in sync. Devices are populated from `GVolumeMonitor` and
/// kept live as drives are mounted/unmounted.
pub(crate) fn build(
    window: &adw::ApplicationWindow,
    navigate: Rc<dyn Fn(VeyraPath)>,
    open_in_new_tab: Rc<dyn Fn(VeyraPath)>,
    thumbnails: Rc<ThumbnailService>,
    dnd_execute: DropExecutor,
    refresh_all: Rc<dyn Fn()>,
    undo_stack: SharedUndoStack,
) -> gtk4::Widget {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(6);
    root.set_margin_end(6);

    root.append(&section_heading(t("sidebar.places")));
    for (label, icon, path, kind) in places_entries() {
        root.append(&row(label, icon, path, kind, &navigate));
    }
    root.append(&storage_dashboard_row());

    let bookmarks_section = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    bookmarks_section.append(&section_heading(t("sidebar.bookmarks")));
    let bookmarks_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    bookmarks_section.append(&bookmarks_box);
    root.append(&bookmarks_section);

    let refresh_bookmarks: Rc<dyn Fn()> = {
        let bookmarks_box = bookmarks_box.clone();
        let window = window.clone();
        let navigate = navigate.clone();
        let open_in_new_tab = open_in_new_tab.clone();
        let dnd_execute = dnd_execute.clone();
        Rc::new(move || {
            refresh_bookmarks_box(
                &bookmarks_box,
                &window,
                &navigate,
                &open_in_new_tab,
                &dnd_execute,
            )
        })
    };
    refresh_bookmarks();

    // Live sync: reacts to bookmarks-file edits from Veyra itself (a
    // mutation below simply causes this to re-render the already up-to-date
    // list) as well as from any other application. `gio::FileMonitor` stops
    // watching once its last strong reference drops, so a clone is kept
    // alive for as long as `bookmarks_section` exists by capturing it (and
    // touching it, so it's actually captured) in the drop-target closure
    // below, which itself lives as long as the controller attached to that
    // widget.
    let monitor = bookmarks::watch({
        let refresh_bookmarks = refresh_bookmarks.clone();
        move || refresh_bookmarks()
    });

    // Faz 16: drop zone spanning the whole Bookmarks section — dragging any
    // folder from a view onto it bookmarks the folder. Non-directory drops
    // are silently ignored rather than added. The directory check and the
    // actual write both run off the GTK main thread (Rule #11/#12) since the
    // dropped `gio::File`s could just as well be on a slow network mount as
    // on local disk.
    let drop_target = gtk4::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
    {
        let refresh_bookmarks = refresh_bookmarks.clone();
        let monitor_keepalive = monitor.clone();
        drop_target.connect_drop(move |_, value, _, _| {
            let _ = &monitor_keepalive;
            let Ok(file_list) = value.get::<gdk::FileList>() else {
                return false;
            };
            let files: Vec<gio::File> = file_list.files().into_iter().collect();
            if files.is_empty() {
                return false;
            }

            let refresh_bookmarks = refresh_bookmarks.clone();
            fs_async::run_blocking(
                move || {
                    let mut added = false;
                    for file in files {
                        let is_dir = file
                            .query_file_type(gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE)
                            == gio::FileType::Directory;
                        if !is_dir {
                            continue;
                        }
                        let path = VeyraPath::from_gio_file(&file);
                        match bookmarks::add(&path, None) {
                            Ok(()) => added = true,
                            Err(err) => tracing::warn!(
                                error = %err,
                                "failed to add dropped folder to bookmarks"
                            ),
                        }
                    }
                    added
                },
                move |added| {
                    if added {
                        refresh_bookmarks();
                    }
                },
            );
            true
        });
    }
    bookmarks_section.add_controller(drop_target);

    root.append(&section_heading(t("sidebar.devices")));
    let devices_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    root.append(&devices_box);

    let monitor = gio::VolumeMonitor::get();
    let refresh_devices: Rc<dyn Fn()> = {
        let devices_box = devices_box.clone();
        let window = window.clone();
        let navigate = navigate.clone();
        let open_in_new_tab = open_in_new_tab.clone();
        let thumbnails = thumbnails.clone();
        let monitor = monitor.clone();
        let refresh_all = refresh_all.clone();
        let undo_stack = undo_stack.clone();
        Rc::new(move || {
            refresh_devices_box(
                &devices_box,
                &monitor,
                &window,
                &navigate,
                &open_in_new_tab,
                &thumbnails,
                &refresh_all,
                &undo_stack,
            )
        })
    };
    refresh_devices();

    // Faz 21: Network section — the "Network" (`network:///` local-network
    // browse) root always shown, live-refreshed active SFTP/SMB/FTP/WebDAV
    // mounts below it, and a "+ Connect to Server…" action at the bottom.
    root.append(&section_heading(t("sidebar.network")));
    root.append(&row(
        t("sidebar.network"),
        "network-workgroup-symbolic",
        VeyraPath::from_uri("network:///"),
        t("sidebar.network_location"),
        &navigate,
    ));
    let network_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    root.append(&network_box);

    let refresh_network: Rc<dyn Fn()> = {
        let network_box = network_box.clone();
        let window = window.clone();
        let navigate = navigate.clone();
        let open_in_new_tab = open_in_new_tab.clone();
        let thumbnails = thumbnails.clone();
        let monitor = monitor.clone();
        let refresh_all = refresh_all.clone();
        let undo_stack = undo_stack.clone();
        Rc::new(move || {
            refresh_network_box(
                &network_box,
                &monitor,
                &window,
                &navigate,
                &open_in_new_tab,
                &thumbnails,
                &refresh_all,
                &undo_stack,
            )
        })
    };
    refresh_network();

    let connect_button = gtk4::Button::builder().css_classes(["flat"]).build();
    connect_button.set_action_name(Some("win.connect-to-server"));
    connect_button.update_property(&[gtk4::accessible::Property::Label(t(
        "sidebar.connect_to_server",
    ))]);
    let connect_content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    connect_content.append(&gtk4::Image::from_icon_name("list-add-symbolic"));
    let connect_label = gtk4::Label::new(Some(t("sidebar.connect_to_server_ellipsis")));
    connect_label.set_xalign(0.0);
    connect_content.append(&connect_label);
    connect_button.set_child(Some(&connect_content));
    root.append(&connect_button);

    // Faz 17/21: live hotplug — any of these seven `GVolumeMonitor` signals
    // means the Devices list, a row's mount/eject affordances, or the
    // Network section's active mounts may be stale, so all of them
    // re-render both sections from scratch.
    let refresh_all: Rc<dyn Fn()> = {
        let refresh_devices = refresh_devices.clone();
        let refresh_network = refresh_network.clone();
        Rc::new(move || {
            refresh_devices();
            refresh_network();
        })
    };
    {
        let refresh_all = refresh_all.clone();
        monitor.connect_mount_added(move |_, _| refresh_all());
    }
    {
        let refresh_all = refresh_all.clone();
        monitor.connect_mount_removed(move |_, _| refresh_all());
    }
    {
        let refresh_all = refresh_all.clone();
        monitor.connect_mount_changed(move |_, _| refresh_all());
    }
    {
        let refresh_all = refresh_all.clone();
        monitor.connect_volume_added(move |_, _| refresh_all());
    }
    {
        let refresh_all = refresh_all.clone();
        monitor.connect_volume_removed(move |_, _| refresh_all());
    }
    {
        let refresh_all = refresh_all.clone();
        monitor.connect_drive_connected(move |_, _| refresh_all());
    }
    monitor.connect_drive_disconnected(move |_, _| refresh_all());

    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&root)
        .build();
    scrolled.upcast()
}

/// The Faz 43 "Storage" row: unlike every other Places entry, it doesn't
/// navigate — it fires `win.show-storage-dashboard` directly (same pattern
/// as the Network section's "Connect to Server…" row below), opening the
/// Smart Storage Dashboard instead.
fn storage_dashboard_row() -> gtk4::Widget {
    let button = gtk4::Button::builder().css_classes(["flat"]).build();
    button.set_action_name(Some("win.show-storage-dashboard"));
    button.set_accessible_role(gtk4::AccessibleRole::Button);
    button.update_property(&[gtk4::accessible::Property::Label(t(
        "storage.sidebar.label",
    ))]);

    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    content.append(&gtk4::Image::from_icon_name("drive-harddisk-symbolic"));
    let text = gtk4::Label::new(Some(t("storage.sidebar.label")));
    text.set_xalign(0.0);
    text.set_hexpand(true);
    text.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    content.append(&text);
    button.set_child(Some(&content));

    button.upcast()
}

fn places_entries() -> Vec<(&'static str, &'static str, VeyraPath, &'static str)> {
    let mut entries = vec![(
        t("sidebar.home"),
        "user-home-symbolic",
        VeyraPath::from_local(glib::home_dir()),
        t("sidebar.home_folder"),
    )];

    let user_dirs: [(&str, &str, UserDirectory); 5] = [
        (
            t("sidebar.desktop"),
            "user-desktop-symbolic",
            UserDirectory::Desktop,
        ),
        (
            t("sidebar.documents"),
            "folder-documents-symbolic",
            UserDirectory::Documents,
        ),
        (
            t("sidebar.downloads"),
            "folder-download-symbolic",
            UserDirectory::Downloads,
        ),
        (
            t("sidebar.music"),
            "folder-music-symbolic",
            UserDirectory::Music,
        ),
        (
            t("sidebar.pictures"),
            "folder-pictures-symbolic",
            UserDirectory::Pictures,
        ),
    ];
    for (label, icon, dir) in user_dirs {
        if let Some(path) = glib::user_special_dir(dir) {
            entries.push((
                label,
                icon,
                VeyraPath::from_local(path),
                t("sidebar.folder"),
            ));
        }
    }
    if let Some(path) = glib::user_special_dir(UserDirectory::Videos) {
        entries.push((
            t("sidebar.videos"),
            "folder-videos-symbolic",
            VeyraPath::from_local(path),
            t("sidebar.folder"),
        ));
    }

    entries.push((
        t("sidebar.recent"),
        "document-open-recent-symbolic",
        VeyraPath::from_uri("recent:///"),
        t("sidebar.recent_files"),
    ));
    entries.push((
        t("sidebar.trash"),
        "user-trash-symbolic",
        VeyraPath::from_uri("trash:///"),
        t("sidebar.trash"),
    ));

    entries
}

/// Rebuilds `container`'s children from `devices::scan` — root filesystem,
/// every active mount, and every not-yet-mounted volume (USB stick plugged
/// in but unopened, optical disc not yet accessed). Called once at sidebar
/// build time and again on every `GVolumeMonitor` hotplug signal.
#[allow(clippy::too_many_arguments)]
fn refresh_devices_box(
    container: &gtk4::Box,
    monitor: &gio::VolumeMonitor,
    window: &adw::ApplicationWindow,
    navigate: &Rc<dyn Fn(VeyraPath)>,
    open_in_new_tab: &Rc<dyn Fn(VeyraPath)>,
    thumbnails: &Rc<ThumbnailService>,
    refresh_all: &Rc<dyn Fn()>,
    undo_stack: &SharedUndoStack,
) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    let refresh_self: Rc<dyn Fn()> = {
        let container = container.clone();
        let monitor = monitor.clone();
        let window = window.clone();
        let navigate = navigate.clone();
        let open_in_new_tab = open_in_new_tab.clone();
        let thumbnails = thumbnails.clone();
        let refresh_all = refresh_all.clone();
        let undo_stack = undo_stack.clone();
        Rc::new(move || {
            refresh_devices_box(
                &container,
                &monitor,
                &window,
                &navigate,
                &open_in_new_tab,
                &thumbnails,
                &refresh_all,
                &undo_stack,
            )
        })
    };

    for entry in devices::scan(monitor) {
        container.append(&device_row(
            entry,
            window,
            navigate,
            open_in_new_tab,
            thumbnails,
            &refresh_self,
            refresh_all,
            undo_stack,
        ));
    }
}

/// Rebuilds `container`'s children from `network::scan_mounts` — the
/// currently active SFTP/SMB/FTP/WebDAV mounts. Reuses `device_row` since a
/// network row needs the exact same click-to-navigate, right-click
/// Unmount/Open in New Tab/Properties menu, and inline eject affordances a
/// Devices row already has.
#[allow(clippy::too_many_arguments)]
fn refresh_network_box(
    container: &gtk4::Box,
    monitor: &gio::VolumeMonitor,
    window: &adw::ApplicationWindow,
    navigate: &Rc<dyn Fn(VeyraPath)>,
    open_in_new_tab: &Rc<dyn Fn(VeyraPath)>,
    thumbnails: &Rc<ThumbnailService>,
    refresh_all: &Rc<dyn Fn()>,
    undo_stack: &SharedUndoStack,
) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    let refresh_self: Rc<dyn Fn()> = {
        let container = container.clone();
        let monitor = monitor.clone();
        let window = window.clone();
        let navigate = navigate.clone();
        let open_in_new_tab = open_in_new_tab.clone();
        let thumbnails = thumbnails.clone();
        let refresh_all = refresh_all.clone();
        let undo_stack = undo_stack.clone();
        Rc::new(move || {
            refresh_network_box(
                &container,
                &monitor,
                &window,
                &navigate,
                &open_in_new_tab,
                &thumbnails,
                &refresh_all,
                &undo_stack,
            )
        })
    };

    for entry in network::scan_mounts(monitor) {
        container.append(&device_row(
            entry,
            window,
            navigate,
            open_in_new_tab,
            thumbnails,
            &refresh_self,
            refresh_all,
            undo_stack,
        ));
    }
}

/// Builds one Devices row: icon, name + live usage subtitle (queried async
/// per Rule #11/#12 — never blocks the row from appearing), an inline
/// Unmount/Eject button when applicable, and a right-click context menu
/// (Open in New Tab / Mount / Unmount / Safe Removal / Properties, each
/// enabled only when the underlying `gio` object supports it).
#[allow(clippy::too_many_arguments)]
fn device_row(
    entry: DeviceEntry,
    window: &adw::ApplicationWindow,
    navigate: &Rc<dyn Fn(VeyraPath)>,
    open_in_new_tab: &Rc<dyn Fn(VeyraPath)>,
    thumbnails: &Rc<ThumbnailService>,
    on_changed: &Rc<dyn Fn()>,
    refresh_all: &Rc<dyn Fn()>,
    undo_stack: &SharedUndoStack,
) -> gtk4::Widget {
    let entry = Rc::new(entry);

    let button = gtk4::Button::builder().css_classes(["flat"]).build();
    button.set_accessible_role(gtk4::AccessibleRole::Button);
    let device_state = if entry.path.is_some() {
        "mounted"
    } else {
        "not mounted"
    };
    button.update_property(&[gtk4::accessible::Property::Label(&format!(
        "{}, {device_state}",
        entry.label
    ))]);

    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let icon = gtk4::Image::from_icon_name(entry.icon_name);
    content.append(&icon);

    let text_box = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
    text_box.set_hexpand(true);
    text_box.set_valign(gtk4::Align::Center);
    let name_label = gtk4::Label::new(Some(&entry.label));
    name_label.set_xalign(0.0);
    name_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    text_box.append(&name_label);

    let subtitle_label = gtk4::Label::new(Some(if entry.path.is_some() {
        "Calculating…"
    } else {
        "Not mounted"
    }));
    subtitle_label.set_xalign(0.0);
    subtitle_label.add_css_class("caption");
    subtitle_label.add_css_class("dim-label");
    subtitle_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    text_box.append(&subtitle_label);

    let progress = gtk4::ProgressBar::new();
    progress.set_visible(false);
    text_box.append(&progress);

    content.append(&text_box);

    if entry.can_eject() || entry.can_unmount() {
        let eject_button = gtk4::Button::from_icon_name("media-eject-symbolic");
        eject_button.add_css_class("flat");
        eject_button.set_valign(gtk4::Align::Center);
        eject_button.set_tooltip_text(Some(t("sidebar.unmount_eject")));
        {
            let window = window.clone();
            let entry = entry.clone();
            let on_changed = on_changed.clone();
            eject_button.connect_clicked(move |_| {
                request_eject_or_unmount(&window, &entry, on_changed.clone());
            });
        }
        content.append(&eject_button);
    }

    button.set_child(Some(&content));

    {
        let window = window.clone();
        let navigate = navigate.clone();
        let entry = entry.clone();
        let on_changed = on_changed.clone();
        button.connect_clicked(move |_| {
            if let Some(path) = entry.path.clone() {
                navigate(path);
            } else if let Some(volume) = entry.volume.clone() {
                request_mount(&window, volume, navigate.clone(), on_changed.clone());
            }
        });
    }

    let actions = gio::SimpleActionGroup::new();

    let action_open_tab = gio::SimpleAction::new("open-tab", None);
    action_open_tab.set_enabled(entry.path.is_some());
    {
        let open_in_new_tab = open_in_new_tab.clone();
        let entry = entry.clone();
        action_open_tab.connect_activate(move |_, _| {
            if let Some(path) = entry.path.clone() {
                open_in_new_tab(path);
            }
        });
    }
    actions.add_action(&action_open_tab);

    let action_mount = gio::SimpleAction::new("mount", None);
    action_mount.set_enabled(entry.can_mount());
    {
        let window = window.clone();
        let navigate = navigate.clone();
        let entry = entry.clone();
        let on_changed = on_changed.clone();
        action_mount.connect_activate(move |_, _| {
            if let Some(volume) = entry.volume.clone() {
                request_mount(&window, volume, navigate.clone(), on_changed.clone());
            }
        });
    }
    actions.add_action(&action_mount);

    let action_unmount = gio::SimpleAction::new("unmount", None);
    action_unmount.set_enabled(entry.can_unmount());
    {
        let window = window.clone();
        let entry = entry.clone();
        let on_changed = on_changed.clone();
        action_unmount.connect_activate(move |_, _| {
            request_eject_or_unmount(&window, &entry, on_changed.clone());
        });
    }
    actions.add_action(&action_unmount);

    let action_eject = gio::SimpleAction::new("eject", None);
    action_eject.set_enabled(entry.can_eject());
    {
        let window = window.clone();
        let entry = entry.clone();
        let on_changed = on_changed.clone();
        action_eject.connect_activate(move |_, _| {
            request_eject_or_unmount(&window, &entry, on_changed.clone());
        });
    }
    actions.add_action(&action_eject);

    let action_analyze = gio::SimpleAction::new("analyze", None);
    action_analyze.set_enabled(entry.path.is_some());
    {
        let window = window.clone();
        let navigate = navigate.clone();
        let entry = entry.clone();
        let refresh_all = refresh_all.clone();
        let undo_stack = undo_stack.clone();
        action_analyze.connect_activate(move |_, _| {
            if let Some(path) = entry.path.clone() {
                dialogs::disk_analyzer_dialog::show(
                    &window,
                    path,
                    navigate.clone(),
                    refresh_all.clone(),
                    undo_stack.clone(),
                );
            }
        });
    }
    actions.add_action(&action_analyze);

    let action_properties = gio::SimpleAction::new("properties", None);
    action_properties.set_enabled(entry.path.is_some());
    {
        let window = window.clone();
        let entry = entry.clone();
        let thumbnails = thumbnails.clone();
        action_properties.connect_activate(move |_, _| {
            if let Some(path) = entry.path.clone() {
                show_device_properties(&window, path, thumbnails.clone());
            }
        });
    }
    actions.add_action(&action_properties);

    button.insert_action_group("device", Some(&actions));

    let menu = gio::Menu::new();
    menu.append(Some("Open in New Tab"), Some("device.open-tab"));
    menu.append(Some("Mount"), Some("device.mount"));
    menu.append(Some("Unmount"), Some("device.unmount"));
    menu.append(Some("Safe Removal / Eject"), Some("device.eject"));
    menu.append(Some("Analyze Disk…"), Some("device.analyze"));
    menu.append(Some("Properties"), Some("device.properties"));

    let popover = gtk4::PopoverMenu::from_model(Some(&menu));
    popover.set_parent(&button);
    popover.set_has_arrow(false);
    popover.set_halign(gtk4::Align::Start);

    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gdk::BUTTON_SECONDARY);
    gesture.connect_released(move |gesture, _n_press, x, y| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        popover.popup();
    });
    button.add_controller(gesture);

    if let Some(path) = entry.path.clone() {
        fs_async::run_blocking(
            move || devices::query_usage(&path),
            move |usage| match usage {
                Some(usage) => {
                    subtitle_label.set_text(&devices::format_usage(&usage));
                    progress.set_fraction(devices::usage_fraction(&usage));
                    progress.set_visible(true);
                }
                None => subtitle_label.set_text("Usage unavailable"),
            },
        );
    }

    button.upcast()
}

/// Mounts `volume` (prompting for credentials/decryption via a default
/// `GMountOperation` if the backend needs it — password-protected network
/// shares, LUKS-encrypted volumes), then navigates into it once mounted.
/// Failure surfaces as an `AdwAlertDialog` rather than a silent no-op or a
/// crash (Rule #15/#18).
fn request_mount(
    window: &adw::ApplicationWindow,
    volume: gio::Volume,
    navigate: Rc<dyn Fn(VeyraPath)>,
    on_changed: Rc<dyn Fn()>,
) {
    let window = window.clone();
    glib::spawn_future_local(async move {
        let operation = gio::MountOperation::new();
        let result = volume
            .mount_future(gio::MountMountFlags::NONE, Some(&operation))
            .await;
        match result {
            Ok(()) => {
                if let Some(mount) = volume.get_mount() {
                    let root_file = mount.root();
                    let path = match root_file.path() {
                        Some(local) => VeyraPath::from_local(local),
                        None => VeyraPath::from_uri(root_file.uri().to_string()),
                    };
                    navigate(path);
                }
                on_changed();
            }
            Err(err) => {
                tracing::warn!(error = %err, "failed to mount volume");
                show_device_error(&window, "Unable to Mount Device", &err.to_string());
            }
        }
    });
}

/// Unmounts (preferring the mount-level operation, since it's the most
/// common case) or, for hardware that needs it, ejects `entry` — a mount
/// busy with an open file, or a permissions failure, ends in an
/// `AdwAlertDialog` instead of a stuck spinner or a panic (Rule #15/#18).
fn request_eject_or_unmount(
    window: &adw::ApplicationWindow,
    entry: &DeviceEntry,
    on_changed: Rc<dyn Fn()>,
) {
    let window = window.clone();
    if let Some(mount) = entry.mount.clone() {
        let use_eject = mount.can_eject();
        glib::spawn_future_local(async move {
            let operation = gio::MountOperation::new();
            let result = if use_eject {
                mount
                    .eject_with_operation_future(gio::MountUnmountFlags::NONE, Some(&operation))
                    .await
            } else {
                mount
                    .unmount_with_operation_future(gio::MountUnmountFlags::NONE, Some(&operation))
                    .await
            };
            match result {
                Ok(()) => on_changed(),
                Err(err) => {
                    tracing::warn!(error = %err, "failed to unmount/eject device");
                    show_device_error(&window, "Device is Busy", &err.to_string());
                }
            }
        });
        return;
    }

    if let Some(volume) = entry.volume.clone() {
        glib::spawn_future_local(async move {
            let operation = gio::MountOperation::new();
            match volume
                .eject_with_operation_future(gio::MountUnmountFlags::NONE, Some(&operation))
                .await
            {
                Ok(()) => on_changed(),
                Err(err) => {
                    tracing::warn!(error = %err, "failed to eject volume");
                    show_device_error(&window, "Unable to Eject Device", &err.to_string());
                }
            }
        });
        return;
    }

    if let Some(drive) = entry.drive.clone() {
        glib::spawn_future_local(async move {
            let operation = gio::MountOperation::new();
            match drive
                .eject_with_operation_future(gio::MountUnmountFlags::NONE, Some(&operation))
                .await
            {
                Ok(()) => on_changed(),
                Err(err) => {
                    tracing::warn!(error = %err, "failed to eject drive");
                    show_device_error(&window, "Unable to Eject Device", &err.to_string());
                }
            }
        });
    }
}

fn show_device_error(parent: &impl IsA<gtk4::Widget>, heading: &str, body: &str) {
    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .body(body)
        .build();
    dialog.add_responses(&[("ok", "OK")]);
    dialog.set_default_response(Some("ok"));
    dialog.set_close_response("ok");
    dialog.present(Some(parent));
}

/// Stats `path` off the GTK main thread (Rule #11/#12) and opens the shared
/// Properties dialog on success; a device that vanished between the click
/// and the stat (unplugged mid-click) fails gracefully via the same
/// `AdwAlertDialog` pattern rather than panicking.
fn show_device_properties(
    window: &adw::ApplicationWindow,
    path: VeyraPath,
    thumbnails: Rc<ThumbnailService>,
) {
    let window = window.clone();
    fs_async::run_blocking(
        move || veyra_filesystem::stat(&path),
        move |result| match result {
            Ok(item) => {
                dialogs::properties_dialog::show(&window, item, thumbnails, Rc::new(|| {}));
            }
            Err(err) => {
                show_device_error(&window, "Unable to Read Device", &err.to_string());
            }
        },
    );
}

fn row(
    label: &str,
    icon_name: &str,
    target: VeyraPath,
    kind: &str,
    navigate: &Rc<dyn Fn(VeyraPath)>,
) -> gtk4::Widget {
    let button = gtk4::Button::builder().css_classes(["flat"]).build();
    button.set_accessible_role(gtk4::AccessibleRole::Button);
    button.update_property(&[gtk4::accessible::Property::Label(&format!(
        "{label}, {kind}"
    ))]);

    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let icon = gtk4::Image::from_icon_name(icon_name);
    let text = gtk4::Label::new(Some(label));
    text.set_xalign(0.0);
    text.set_hexpand(true);
    text.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    content.append(&icon);
    content.append(&text);
    button.set_child(Some(&content));

    let navigate = navigate.clone();
    button.connect_clicked(move |_| navigate(target.clone()));

    button.upcast()
}

/// Rebuilds `container`'s children from `bookmarks::load()`. Called once at
/// sidebar build time and again every time the bookmarks file changes
/// (Veyra's own mutations, or an external editor/other app).
fn refresh_bookmarks_box(
    container: &gtk4::Box,
    window: &adw::ApplicationWindow,
    navigate: &Rc<dyn Fn(VeyraPath)>,
    open_in_new_tab: &Rc<dyn Fn(VeyraPath)>,
    dnd_execute: &DropExecutor,
) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    let refresh_self: Rc<dyn Fn()> = {
        let container = container.clone();
        let window = window.clone();
        let navigate = navigate.clone();
        let open_in_new_tab = open_in_new_tab.clone();
        let dnd_execute = dnd_execute.clone();
        Rc::new(move || {
            refresh_bookmarks_box(
                &container,
                &window,
                &navigate,
                &open_in_new_tab,
                &dnd_execute,
            )
        })
    };

    for bookmark in bookmarks::load() {
        container.append(&bookmark_row(
            &bookmark,
            window,
            navigate,
            open_in_new_tab,
            &refresh_self,
            dnd_execute,
        ));
    }
}

/// Builds one Bookmarks row: click navigates, a right-click opens a context
/// menu (Open in New Tab / Rename Bookmark… / Remove from Bookmarks) scoped
/// to *this* bookmark via a per-row `SimpleActionGroup` — unlike
/// `context_menu.rs`'s window-wide `win.*` actions, each row needs a
/// different target, so the action group is local instead — and (Faz 26) a
/// file (not folder) dropped on the row is moved/copied/linked into that
/// bookmark's directory; a dropped folder instead falls through to the
/// section-level "add bookmark" drop zone (`build`, above).
fn bookmark_row(
    bookmark: &Bookmark,
    window: &adw::ApplicationWindow,
    navigate: &Rc<dyn Fn(VeyraPath)>,
    open_in_new_tab: &Rc<dyn Fn(VeyraPath)>,
    on_changed: &Rc<dyn Fn()>,
    dnd_execute: &DropExecutor,
) -> gtk4::Widget {
    let label = bookmark.display_label();
    let button = gtk4::Button::builder().css_classes(["flat"]).build();
    button.set_accessible_role(gtk4::AccessibleRole::Button);
    button.update_property(&[gtk4::accessible::Property::Label(&format!(
        "{label}, Bookmark"
    ))]);

    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let icon = gtk4::Image::from_icon_name("starred-symbolic");
    let text = gtk4::Label::new(Some(&label));
    text.set_xalign(0.0);
    text.set_hexpand(true);
    text.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    content.append(&icon);
    content.append(&text);
    button.set_child(Some(&content));

    {
        let navigate = navigate.clone();
        let target = bookmark.path.clone();
        button.connect_clicked(move |_| navigate(target.clone()));
    }

    {
        let drop_target = gtk4::DropTarget::new(
            gdk::FileList::static_type(),
            gdk::DragAction::COPY
                | gdk::DragAction::MOVE
                | gdk::DragAction::LINK
                | gdk::DragAction::ASK,
        );
        let dest = bookmark.path.clone();
        let button_for_popover: gtk4::Widget = button.clone().upcast();
        let dnd_execute = dnd_execute.clone();
        drop_target.connect_drop(move |target, value, x, y| {
            let Ok(file_list) = value.get::<gdk::FileList>() else {
                return false;
            };
            let files: Vec<gio::File> = file_list.files().into_iter().collect();
            // A dropped folder is left for the section-level drop zone
            // (`build`, above) to bookmark instead of being moved/copied
            // into this bookmark's own directory.
            let non_directory_sources: Vec<VeyraPath> = files
                .into_iter()
                .filter(|f| {
                    f.query_file_type(gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE)
                        != gio::FileType::Directory
                })
                .map(|f| VeyraPath::from_gio_file(&f))
                .collect();
            if non_directory_sources.is_empty() {
                return false;
            }
            dnd::resolve_and_execute(
                target,
                &button_for_popover,
                x,
                y,
                non_directory_sources,
                dest.clone(),
                dnd_execute.clone(),
            );
            true
        });
        button.add_controller(drop_target);
    }

    let actions = gio::SimpleActionGroup::new();

    let action_open_tab = gio::SimpleAction::new("open-tab", None);
    {
        let open_in_new_tab = open_in_new_tab.clone();
        let target = bookmark.path.clone();
        action_open_tab.connect_activate(move |_, _| open_in_new_tab(target.clone()));
    }
    actions.add_action(&action_open_tab);

    let action_rename = gio::SimpleAction::new("rename", None);
    {
        let window = window.clone();
        let uri = bookmark.uri.clone();
        let current_label = label.clone();
        let on_changed = on_changed.clone();
        action_rename.connect_activate(move |_, _| {
            let uri = uri.clone();
            let on_changed = on_changed.clone();
            dialogs::rename_dialog::show(&window, &current_label, move |new_label| {
                if let Err(err) = bookmarks::rename(&uri, &new_label) {
                    tracing::warn!(error = %err, "failed to rename bookmark");
                }
                on_changed();
            });
        });
    }
    actions.add_action(&action_rename);

    let action_remove = gio::SimpleAction::new("remove", None);
    {
        let uri = bookmark.uri.clone();
        let on_changed = on_changed.clone();
        action_remove.connect_activate(move |_, _| {
            if let Err(err) = bookmarks::remove(&uri) {
                tracing::warn!(error = %err, "failed to remove bookmark");
            }
            on_changed();
        });
    }
    actions.add_action(&action_remove);

    button.insert_action_group("bookmark", Some(&actions));

    let menu = gio::Menu::new();
    menu.append(Some("Open in New Tab"), Some("bookmark.open-tab"));
    menu.append(Some("Rename Bookmark…"), Some("bookmark.rename"));
    menu.append(Some("Remove from Bookmarks"), Some("bookmark.remove"));

    let popover = gtk4::PopoverMenu::from_model(Some(&menu));
    popover.set_parent(&button);
    popover.set_has_arrow(false);
    popover.set_halign(gtk4::Align::Start);

    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gdk::BUTTON_SECONDARY);
    gesture.connect_released(move |gesture, _n_press, x, y| {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        popover.popup();
    });
    button.add_controller(gesture);

    button.upcast()
}

fn section_heading(title: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(title));
    label.add_css_class("heading");
    label.add_css_class("dim-label");
    label.set_xalign(0.0);
    label.set_margin_top(6);
    label
}
