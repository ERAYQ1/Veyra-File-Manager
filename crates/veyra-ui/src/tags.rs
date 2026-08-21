//! Faz 62: Smart Color Tags UI wiring — a thin layer over
//! `veyra_filesystem::tags` that adds the three things a persistence-only
//! backend can't know about: the virtual `tag:///<color>` sidebar location
//! (same pattern as `recent:///`/`trash:///`), the in-memory `SharedTags`
//! map every view's row factory reads from on bind (mirroring
//! `state::SharedGitStatuses`, but app-wide rather than per-directory since
//! a tag is a property of the file, not of the folder it happens to sit
//! in), and the CSS/i18n plumbing the color pill badges and sidebar rows
//! need.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::prelude::*;

use veyra_filesystem::{FileItem, TagColor, VeyraPath};

use crate::i18n::t;

/// URI → tag color, keyed the same way `veyra_filesystem::tags` persists
/// them (`VeyraPath::to_gio_file().uri()`) so a row's tag can be looked up
/// regardless of which location it's currently listed under — a real
/// directory, `recent:///`, or a `tag:///` results list itself.
pub(crate) type SharedTags = Rc<RefCell<HashMap<String, TagColor>>>;

pub(crate) fn new_shared() -> SharedTags {
    Rc::new(RefCell::new(HashMap::new()))
}

/// Rebuilds `tags` from the real tags file — called once at startup and
/// again after every tag mutation (`window.rs`'s `win.set-tag-selected`/
/// `win.remove-tag-selected` actions), same refresh-on-mutation contract
/// `bookmarks.rs`'s `GFileMonitor` gets for free but a plain JSON file
/// doesn't need: Veyra is the only writer, so an explicit reload after its
/// own writes is enough.
pub(crate) fn reload(tags: &SharedTags) {
    let fresh: HashMap<String, TagColor> = veyra_filesystem::list_all_tagged()
        .into_iter()
        .map(|(path, color)| (path.to_gio_file().uri().to_string(), color))
        .collect();
    *tags.borrow_mut() = fresh;
}

/// `item`'s tag color, if any — looked up by URI in the shared map.
pub(crate) fn tag_for(tags: &SharedTags, item: &FileItem) -> Option<TagColor> {
    let uri = item.path.to_gio_file().uri().to_string();
    tags.borrow().get(&uri).copied()
}

/// The virtual location a sidebar tag row navigates to.
pub(crate) fn tag_uri(color: TagColor) -> String {
    format!("tag:///{}", color.slug())
}

/// Whether `path` is one of Veyra's virtual `tag:///<color>` locations, and
/// which color if so.
pub(crate) fn tag_location_color(path: &VeyraPath) -> Option<TagColor> {
    let VeyraPath::Uri(uri) = path else {
        return None;
    };
    let slug = uri.strip_prefix("tag:///")?;
    TagColor::from_slug(slug)
}

/// Builds a `tag:///<color>`'s listing: every path tagged `color`, `stat`'d
/// (skipping entries whose target has since been deleted/moved without the
/// tag being cleared — Rule #18/#20, a stale tag record must never surface
/// as an error). Blocking — always call off the GTK main thread, same
/// contract as `recent::list_recent_items`.
pub(crate) fn list_tagged_items(color: TagColor) -> Vec<FileItem> {
    veyra_filesystem::get_paths_by_tag(color)
        .into_iter()
        .filter_map(|path| veyra_filesystem::stat(&path).ok())
        .collect()
}

/// The CSS class a `.veyra-tag-pill`/`.veyra-tag-dot` element needs on top
/// of the base class to pick up `color`'s hex swatch (`split_view.rs`'s
/// `install_panel_css`).
pub(crate) fn css_class(color: TagColor) -> String {
    format!("veyra-tag-{}", color.slug())
}

/// Every tag CSS class any pill/dot might carry — recycled list items must
/// have all of these cleared before the current row's class (if any) is
/// re-applied, same as `views::GIT_BADGE_CSS_CLASSES`.
pub(crate) fn all_css_classes() -> [String; 6] {
    TagColor::ALL.map(css_class)
}

/// The default (never user-overridden) localized name for `color`, e.g.
/// "Red"/"Kırmızı" — the Preferences "Tags" page's `AdwEntryRow` placeholder
/// text, and `display_name`'s fallback.
pub(crate) fn default_label(color: TagColor) -> &'static str {
    match color {
        TagColor::Red => t("tags.red"),
        TagColor::Orange => t("tags.orange"),
        TagColor::Yellow => t("tags.yellow"),
        TagColor::Green => t("tags.green"),
        TagColor::Blue => t("tags.blue"),
        TagColor::Purple => t("tags.purple"),
    }
}

/// Faz 63: `color`'s display name — the user's custom name if they've set
/// one (`veyra_filesystem::get_custom_tag_name`), otherwise the default
/// localized color name. This is the one every live-facing surface (sidebar
/// rows, the context menu's color picker, view row tooltips) should call
/// rather than `default_label` directly, so a renamed tag shows up
/// everywhere at once. Reads straight from disk on every call rather than
/// caching — `tags.json` is tiny and this is only ever called at
/// render/bind time, never in a hot per-frame loop.
pub(crate) fn display_name(color: TagColor) -> String {
    veyra_filesystem::get_custom_tag_name(color).unwrap_or_else(|| default_label(color).to_string())
}

/// Builds one Tags sidebar row: a small colored dot plus the color's
/// current display name, navigating to that color's `tag:///` results list
/// on click — same click-to-navigate contract as `sidebar.rs`'s Places
/// `row`.
pub(crate) fn sidebar_row(color: TagColor, navigate: &Rc<dyn Fn(VeyraPath)>) -> gtk4::Widget {
    let name = display_name(color);
    let button = gtk4::Button::builder().css_classes(["flat"]).build();
    button.set_accessible_role(gtk4::AccessibleRole::Button);
    button.update_property(&[gtk4::accessible::Property::Label(&name)]);

    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    dot.add_css_class("veyra-tag-dot");
    dot.add_css_class(&css_class(color));
    dot.set_valign(gtk4::Align::Center);
    content.append(&dot);

    let text = gtk4::Label::new(Some(&name));
    text.set_xalign(0.0);
    text.set_hexpand(true);
    text.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    content.append(&text);
    button.set_child(Some(&content));

    let target = VeyraPath::from_uri(tag_uri(color));
    let navigate = navigate.clone();
    button.connect_clicked(move |_| navigate(target.clone()));

    button.upcast()
}

/// Watches the real tags file for changes (Faz 63: a renamed/reset/cleared
/// tag from the Preferences "Tags" page) and invokes `on_change` whenever it
/// does — same live-sync pattern `bookmarks::watch` uses for the Bookmarks
/// section, letting `sidebar.rs` rebuild its Tags section's row labels
/// without any direct callback wired from the Preferences dialog. Returns
/// `None` (logging a warning) instead of panicking if the monitor can't be
/// created (Rule #15/#18) — the sidebar still shows the names loaded at
/// startup, it just won't live-refresh.
pub(crate) fn watch(on_change: impl Fn() + 'static) -> Option<gtk4::gio::FileMonitor> {
    let file = gtk4::gio::File::for_path(veyra_filesystem::tags_path());
    match file.monitor_file(
        gtk4::gio::FileMonitorFlags::NONE,
        gtk4::gio::Cancellable::NONE,
    ) {
        Ok(monitor) => {
            monitor.connect_changed(move |_, _, _, _event| on_change());
            Some(monitor)
        }
        Err(err) => {
            tracing::warn!(error = %err, "failed to watch tags file for changes");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_uri_round_trips_through_tag_location_color() {
        for color in TagColor::ALL {
            let path = VeyraPath::from_uri(tag_uri(color));
            assert_eq!(tag_location_color(&path), Some(color));
        }
    }

    #[test]
    fn tag_location_color_rejects_other_schemes() {
        assert_eq!(tag_location_color(&VeyraPath::from_uri("trash:///")), None);
        assert_eq!(tag_location_color(&VeyraPath::from_uri("recent:///")), None);
        assert_eq!(
            tag_location_color(&VeyraPath::from_local("/home/user")),
            None
        );
    }

    #[test]
    fn tag_location_color_rejects_unknown_slug() {
        assert_eq!(
            tag_location_color(&VeyraPath::from_uri("tag:///magenta")),
            None
        );
    }

    #[test]
    fn css_class_is_distinct_per_color() {
        let classes: Vec<String> = TagColor::ALL.iter().map(|c| css_class(*c)).collect();
        for (i, class) in classes.iter().enumerate() {
            assert!(!classes[..i].contains(class));
        }
    }
}
