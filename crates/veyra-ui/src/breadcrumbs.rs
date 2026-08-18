use std::path::PathBuf;
use std::rc::Rc;

use gtk4::prelude::*;

use veyra_filesystem::VeyraPath;

use crate::dnd::{self, DropExecutor};

/// Rebuilds `container` into a row of clickable path-segment buttons for
/// `path`, e.g. `Home > Projects > Veyra`. Segments under the user's home
/// directory are rendered relative to `Home`, matching the HIG mockup. Every
/// segment button (Faz 26) is also a drop target: dropping files onto an
/// ancestor crumb moves/copies them into that ancestor directory.
pub(crate) fn rebuild(
    container: &gtk4::Box,
    path: &VeyraPath,
    navigate: Rc<dyn Fn(VeyraPath)>,
    execute: DropExecutor,
) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    match path {
        VeyraPath::Local(local_path) => build_local(container, local_path, navigate, execute),
        VeyraPath::Uri(uri) => {
            let label = gtk4::Label::new(Some(uri));
            label.add_css_class("dim-label");
            container.append(&label);
        }
    }
}

fn build_local(
    container: &gtk4::Box,
    path: &std::path::Path,
    navigate: Rc<dyn Fn(VeyraPath)>,
    execute: DropExecutor,
) {
    let home = gtk4::glib::home_dir();
    let (root_label, root_path, remainder): (&str, PathBuf, &std::path::Path) =
        match path.strip_prefix(&home) {
            Ok(rest) => ("Home", home.clone(), rest),
            Err(_) => (
                "/",
                PathBuf::from("/"),
                path.strip_prefix("/").unwrap_or(path),
            ),
        };

    append_segment(
        container,
        root_label,
        VeyraPath::from_local(root_path.clone()),
        &navigate,
        &execute,
    );

    let mut accumulated = root_path;
    let components: Vec<_> = remainder.components().collect();
    for (index, component) in components.iter().enumerate() {
        accumulated.push(component.as_os_str());
        append_separator(container);
        let label = component.as_os_str().to_string_lossy().into_owned();
        let is_last = index == components.len() - 1;
        if is_last {
            let current = gtk4::Label::new(Some(&label));
            current.add_css_class("heading");
            container.append(&current);
        } else {
            append_segment(
                container,
                &label,
                VeyraPath::from_local(accumulated.clone()),
                &navigate,
                &execute,
            );
        }
    }
}

fn append_segment(
    container: &gtk4::Box,
    label: &str,
    target: VeyraPath,
    navigate: &Rc<dyn Fn(VeyraPath)>,
    execute: &DropExecutor,
) {
    let button = gtk4::Button::builder()
        .label(label)
        .css_classes(["flat"])
        .build();
    button.set_tooltip_text(Some(&target.to_string()));
    button.update_property(&[gtk4::accessible::Property::Label(&format!(
        "Navigate to {label}"
    ))]);
    let navigate = navigate.clone();
    {
        let target = target.clone();
        button.connect_clicked(move |_| navigate(target.clone()));
    }
    dnd::attach_drop_target(
        &button,
        || true,
        move || Some(target.clone()),
        execute.clone(),
    );
    container.append(&button);
}

fn append_separator(container: &gtk4::Box) {
    let separator = gtk4::Label::new(Some("›"));
    separator.add_css_class("dim-label");
    container.append(&separator);
}
