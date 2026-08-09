//! Faz 9: renders `veyra_search::SearchResult` rows for advanced
//! (`name:`/`type:`/`size:`/`modified:`) search queries. Lives in the
//! header bar's title area, revealed under the search entry only while an
//! advanced query is active — plain-text queries keep using Faz 3's simple
//! per-tab filename filter instead (see `headerbar.rs`).

use std::rc::Rc;

use gtk4::prelude::*;

use veyra_filesystem::{format_size, VeyraPath};
use veyra_search::SearchResult;

#[derive(Clone)]
pub(crate) struct SearchResultsHandles {
    pub revealer: gtk4::Revealer,
    container: gtk4::Box,
}

pub(crate) fn build() -> SearchResultsHandles {
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    container.set_margin_top(4);
    container.set_margin_bottom(4);
    container.set_margin_start(4);
    container.set_margin_end(4);

    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .max_content_height(320)
        .propagate_natural_height(true)
        .child(&container)
        .build();
    scrolled.add_css_class("card");

    let revealer = gtk4::Revealer::new();
    revealer.set_child(Some(&scrolled));
    revealer.set_reveal_child(false);
    revealer.set_transition_type(gtk4::RevealerTransitionType::SlideDown);

    SearchResultsHandles {
        revealer,
        container,
    }
}

/// Replaces the list contents with `results`. `on_activate` fires with the
/// location to navigate to when a row is clicked: the result itself for a
/// directory, its parent directory for a file (search finds files, but
/// navigation always lands on a directory listing).
pub(crate) fn update(
    handles: &SearchResultsHandles,
    results: &[SearchResult],
    on_activate: Rc<dyn Fn(VeyraPath)>,
) {
    while let Some(child) = handles.container.first_child() {
        handles.container.remove(&child);
    }

    if results.is_empty() {
        let label = gtk4::Label::new(Some("No matches"));
        label.add_css_class("dim-label");
        label.set_margin_top(8);
        label.set_margin_bottom(8);
        handles.container.append(&label);
        return;
    }

    for result in results {
        let target = navigation_target(result);
        let button = gtk4::Button::new();
        button.add_css_class("flat");
        button.set_child(Some(&row_content(result)));
        let on_activate = on_activate.clone();
        button.connect_clicked(move |_| on_activate(target.clone()));
        handles.container.append(&button);
    }
}

/// A directory result navigates into itself; a file result navigates to its
/// containing directory (falling back to the result path itself if it has
/// no parent, e.g. a filesystem root — which a real file never is, but
/// keeps this total rather than panicking on an unexpected row).
fn navigation_target(result: &SearchResult) -> VeyraPath {
    if result.is_dir {
        VeyraPath::from_local(result.path.clone())
    } else {
        match result.path.parent() {
            Some(parent) => VeyraPath::from_local(parent.to_path_buf()),
            None => VeyraPath::from_local(result.path.clone()),
        }
    }
}

fn row_content(result: &SearchResult) -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    content.set_margin_top(4);
    content.set_margin_bottom(4);
    content.set_margin_start(8);
    content.set_margin_end(8);

    let icon = gtk4::Image::from_icon_name(icon_name_for(result));
    content.append(&icon);

    let labels = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    labels.set_hexpand(true);

    let name = gtk4::Label::new(Some(&result.name));
    name.set_xalign(0.0);
    name.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    labels.append(&name);

    let path_label = gtk4::Label::new(Some(&result.path.display().to_string()));
    path_label.set_xalign(0.0);
    path_label.add_css_class("dim-label");
    path_label.add_css_class("caption");
    path_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    labels.append(&path_label);

    content.append(&labels);

    if !result.is_dir {
        let size = gtk4::Label::new(Some(&format_size(result.size_bytes)));
        size.add_css_class("dim-label");
        content.append(&size);
    }

    content
}

fn icon_name_for(result: &SearchResult) -> &'static str {
    if result.is_dir {
        return "folder-symbolic";
    }
    match result.kind.as_str() {
        "image" => "image-x-generic-symbolic",
        "video" => "video-x-generic-symbolic",
        "document" => "text-x-generic-symbolic",
        "archive" => "package-x-generic-symbolic",
        "executable" => "system-run-symbolic",
        _ => "text-x-generic-symbolic",
    }
}
