use std::rc::Rc;

use gtk4::glib::UserDirectory;
use gtk4::prelude::*;
use gtk4::{gdk, gio, glib};
use libadwaita as adw;

use veyra_filesystem::VeyraPath;

use crate::bookmarks::{self, Bookmark};
use crate::dialogs;
use crate::fs_async;

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
) -> gtk4::Widget {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(6);
    root.set_margin_end(6);

    root.append(&section_heading("Places"));
    for (label, icon, path) in places_entries() {
        root.append(&row(label, icon, path, &navigate));
    }

    let bookmarks_section = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    bookmarks_section.append(&section_heading("Bookmarks"));
    let bookmarks_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    bookmarks_section.append(&bookmarks_box);
    root.append(&bookmarks_section);

    let refresh_bookmarks: Rc<dyn Fn()> = {
        let bookmarks_box = bookmarks_box.clone();
        let window = window.clone();
        let navigate = navigate.clone();
        let open_in_new_tab = open_in_new_tab.clone();
        Rc::new(move || refresh_bookmarks_box(&bookmarks_box, &window, &navigate, &open_in_new_tab))
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

    root.append(&section_heading("Devices"));
    let devices = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    root.append(&devices);

    let monitor = gio::VolumeMonitor::get();
    refresh_devices(&devices, &monitor, &navigate);

    let on_change = {
        let devices = devices.clone();
        let navigate = navigate.clone();
        let monitor = monitor.clone();
        move || refresh_devices(&devices, &monitor, &navigate)
    };
    {
        let on_change = on_change.clone();
        monitor.connect_mount_added(move |_, _| on_change());
    }
    {
        let on_change = on_change.clone();
        monitor.connect_mount_removed(move |_, _| on_change());
    }
    monitor.connect_mount_changed(move |_, _| on_change());

    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&root)
        .build();
    scrolled.upcast()
}

fn places_entries() -> Vec<(&'static str, &'static str, VeyraPath)> {
    let mut entries = vec![(
        "Home",
        "user-home-symbolic",
        VeyraPath::from_local(glib::home_dir()),
    )];

    let user_dirs: [(&str, &str, UserDirectory); 5] = [
        ("Desktop", "user-desktop-symbolic", UserDirectory::Desktop),
        (
            "Documents",
            "folder-documents-symbolic",
            UserDirectory::Documents,
        ),
        (
            "Downloads",
            "folder-download-symbolic",
            UserDirectory::Downloads,
        ),
        ("Music", "folder-music-symbolic", UserDirectory::Music),
        (
            "Pictures",
            "folder-pictures-symbolic",
            UserDirectory::Pictures,
        ),
    ];
    for (label, icon, dir) in user_dirs {
        if let Some(path) = glib::user_special_dir(dir) {
            entries.push((label, icon, VeyraPath::from_local(path)));
        }
    }
    if let Some(path) = glib::user_special_dir(UserDirectory::Videos) {
        entries.push((
            "Videos",
            "folder-videos-symbolic",
            VeyraPath::from_local(path),
        ));
    }

    entries.push((
        "Recent",
        "document-open-recent-symbolic",
        VeyraPath::from_uri("recent:///"),
    ));
    entries.push((
        "Trash",
        "user-trash-symbolic",
        VeyraPath::from_uri("trash:///"),
    ));

    entries
}

fn refresh_devices(
    container: &gtk4::Box,
    monitor: &gio::VolumeMonitor,
    navigate: &Rc<dyn Fn(VeyraPath)>,
) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    for mount in monitor.mounts() {
        let root_file = mount.root();
        let path = match root_file.path() {
            Some(local) => VeyraPath::from_local(local),
            None => VeyraPath::from_uri(root_file.uri().to_string()),
        };
        container.append(&row(
            &mount.name(),
            "drive-harddisk-symbolic",
            path,
            navigate,
        ));
    }
}

fn row(
    label: &str,
    icon_name: &str,
    target: VeyraPath,
    navigate: &Rc<dyn Fn(VeyraPath)>,
) -> gtk4::Widget {
    let button = gtk4::Button::builder().css_classes(["flat"]).build();
    button.set_accessible_role(gtk4::AccessibleRole::Button);
    button.update_property(&[gtk4::accessible::Property::Label(label)]);

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
) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    let refresh_self: Rc<dyn Fn()> = {
        let container = container.clone();
        let window = window.clone();
        let navigate = navigate.clone();
        let open_in_new_tab = open_in_new_tab.clone();
        Rc::new(move || refresh_bookmarks_box(&container, &window, &navigate, &open_in_new_tab))
    };

    for bookmark in bookmarks::load() {
        container.append(&bookmark_row(
            &bookmark,
            window,
            navigate,
            open_in_new_tab,
            &refresh_self,
        ));
    }
}

/// Builds one Bookmarks row: click navigates, and a right-click opens a
/// context menu (Open in New Tab / Rename Bookmark… / Remove from
/// Bookmarks) scoped to *this* bookmark via a per-row `SimpleActionGroup` —
/// unlike `context_menu.rs`'s window-wide `win.*` actions, each row needs a
/// different target, so the action group is local instead.
fn bookmark_row(
    bookmark: &Bookmark,
    window: &adw::ApplicationWindow,
    navigate: &Rc<dyn Fn(VeyraPath)>,
    open_in_new_tab: &Rc<dyn Fn(VeyraPath)>,
    on_changed: &Rc<dyn Fn()>,
) -> gtk4::Widget {
    let label = bookmark.display_label();
    let button = gtk4::Button::builder().css_classes(["flat"]).build();
    button.set_accessible_role(gtk4::AccessibleRole::Button);
    button.update_property(&[gtk4::accessible::Property::Label(label.as_str())]);

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
