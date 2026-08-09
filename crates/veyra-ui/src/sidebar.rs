use std::rc::Rc;

use gtk4::glib::UserDirectory;
use gtk4::prelude::*;
use gtk4::{gio, glib};

use veyra_filesystem::VeyraPath;

/// Builds the Places + Devices sidebar. Places are the standard XDG user
/// directories (resolved via GLib so localized folder names are respected)
/// plus the virtual `trash://` and `recent://` GIO locations. Devices are
/// populated from `GVolumeMonitor` and kept live as drives are
/// mounted/unmounted.
pub(crate) fn build(navigate: Rc<dyn Fn(VeyraPath)>) -> gtk4::Widget {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(6);
    root.set_margin_end(6);

    root.append(&section_heading("Places"));
    for (label, icon, path) in places_entries() {
        root.append(&row(label, icon, path, &navigate));
    }

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

fn section_heading(title: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(title));
    label.add_css_class("heading");
    label.add_css_class("dim-label");
    label.set_xalign(0.0);
    label.set_margin_top(6);
    label
}
