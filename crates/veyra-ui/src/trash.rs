//! Faz 18: Trash Integration.
//!
//! Like `recent:///` (Faz 15), `trash:///` is not read through the generic
//! `veyra_filesystem::read_dir` path — `window.rs` instead calls
//! `veyra_filesystem::list_trash`, which reads the home trash's `files/`
//! directory directly rather than depending on the `trash://` GVfs backend
//! (`gvfsd-trash`), for the same robustness reason `restore_from_trash`
//! already does (see Faz 2's changelog notes). Each listed `FileItem`'s
//! `path` is the physical `Trash/files/...` entry, so it's already a valid
//! `restore_from_trash`/`delete` argument — no separate "trash entry" type
//! needed, and every existing Icon/Compact/Details view renders it for free.

use gtk4::prelude::*;

use veyra_filesystem::{format_size, FileItem, VeyraPath};

/// The virtual location the sidebar's "Trash" entry navigates to.
pub(crate) const TRASH_URI: &str = "trash:///";

/// Whether `path` is Veyra's Trash location.
pub(crate) fn is_trash_location(path: &VeyraPath) -> bool {
    matches!(path, VeyraPath::Uri(uri) if uri == TRASH_URI)
}

/// Renders `items`' count and total size as the Trash banner's title, e.g.
/// `"Trash — 12 items, 4.3 MB total"`. An empty trash falls back to a bare
/// "Trash is Empty" message.
pub(crate) fn format_summary(items: &[FileItem]) -> String {
    if items.is_empty() {
        return "Trash is Empty".to_string();
    }
    let total: u64 = items.iter().map(|item| item.metadata.size_bytes).sum();
    let count_label = if items.len() == 1 {
        "1 item".to_string()
    } else {
        format!("{} items", items.len())
    };
    format!("Trash — {count_label}, {} total", format_size(total))
}

/// The `trash:///`-only info row a panel reveals above its tab strip: item/
/// size summary plus a destructive "Empty Trash" button. Built once per
/// panel in `split_view::build_panel`; `window.rs::update_chrome` reveals it
/// whenever that panel's active tab is showing Trash.
#[derive(Clone)]
pub(crate) struct TrashBannerHandles {
    pub revealer: gtk4::Revealer,
    pub title_label: gtk4::Label,
    pub empty_button: gtk4::Button,
}

/// Builds the (initially collapsed) Trash banner.
pub(crate) fn build_banner() -> TrashBannerHandles {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    row.set_margin_start(12);
    row.set_margin_end(12);
    row.set_margin_top(6);
    row.set_margin_bottom(6);
    row.add_css_class("card");

    let title_label = gtk4::Label::new(Some("Trash"));
    title_label.set_hexpand(true);
    title_label.set_xalign(0.0);
    row.append(&title_label);

    let empty_button = gtk4::Button::with_label("Empty Trash");
    empty_button.add_css_class("destructive-action");
    row.append(&empty_button);

    let revealer = gtk4::Revealer::new();
    revealer.set_transition_type(gtk4::RevealerTransitionType::SlideDown);
    revealer.set_child(Some(&row));
    revealer.set_reveal_child(false);

    TrashBannerHandles {
        revealer,
        title_label,
        empty_button,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item_with_size(name: &str, size: u64) -> FileItem {
        let mut item = veyra_filesystem::stat(&VeyraPath::from_local(std::env::temp_dir()))
            .expect("temp dir must be stat-able");
        item.metadata.name = name.to_string();
        item.metadata.size_bytes = size;
        item
    }

    #[test]
    fn is_trash_location_matches_exact_uri() {
        assert!(is_trash_location(&VeyraPath::from_uri("trash:///")));
        assert!(!is_trash_location(&VeyraPath::from_uri("recent:///")));
        assert!(!is_trash_location(&VeyraPath::from_local("/home/user")));
    }

    #[test]
    fn format_summary_empty_trash() {
        assert_eq!(format_summary(&[]), "Trash is Empty");
    }

    #[test]
    fn format_summary_singular_item() {
        let items = vec![item_with_size("a.txt", 1024)];
        assert_eq!(format_summary(&items), "Trash — 1 item, 1.0 KB total");
    }

    #[test]
    fn format_summary_plural_items_sums_size() {
        let items = vec![item_with_size("a.txt", 1024), item_with_size("b.txt", 1024)];
        assert_eq!(format_summary(&items), "Trash — 2 items, 2.0 KB total");
    }
}
