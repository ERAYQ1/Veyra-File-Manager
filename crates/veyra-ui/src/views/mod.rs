mod compact_view;
mod details_view;
mod icon_view;

use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{gdk, gio, glib};

pub(crate) use compact_view::build_compact_view;
pub(crate) use details_view::{build_details_view, DetailsSortWiring};
pub(crate) use icon_view::build_icon_view;

use veyra_filesystem::{FileItem, FileKind};

use crate::dnd::{self, DndWiring};
use crate::thumbnails::ThumbnailService;

/// The three directory presentation modes, all sharing one `SortConfig`-
/// driven sorter and `QuickFilter` (Faz 13, see `crate::sorting`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ViewMode {
    Icon,
    Compact,
    Details,
}

impl ViewMode {
    pub(crate) fn stack_name(self) -> &'static str {
        match self {
            ViewMode::Icon => "icon",
            ViewMode::Compact => "compact",
            ViewMode::Details => "details",
        }
    }
}

/// Retrieves the `FileItem` stored at `position` in a `ListModel` of
/// `glib::BoxedAnyObject`, if any.
pub(crate) fn item_at(model: &impl IsA<gio::ListModel>, position: u32) -> Option<FileItem> {
    let object = model.item(position)?;
    let boxed = object.downcast_ref::<glib::BoxedAnyObject>()?;
    let cloned = boxed.borrow::<FileItem>().clone();
    Some(cloned)
}

/// The first selected item in `selection` (in ascending position order), if
/// any — used where exactly one target is expected (Open, Rename,
/// Properties, …) even though the underlying model may hold a larger
/// multi-selection.
pub(crate) fn selected_item(selection: &gtk4::MultiSelection) -> Option<FileItem> {
    let bitset = selection.selection();
    if bitset.is_empty() {
        return None;
    }
    item_at(selection, bitset.nth(0))
}

/// Every selected item in `selection`, in ascending position order — the
/// full set a bulk action (Copy/Cut/Trash/Delete/Compress/…) should apply
/// to.
pub(crate) fn selected_items(selection: &gtk4::MultiSelection) -> Vec<FileItem> {
    let bitset = selection.selection();
    let count = bitset.size();
    let mut items = Vec::with_capacity(count as usize);
    for i in 0..count {
        let position = bitset.nth(i as u32);
        if let Some(item) = item_at(selection, position) {
            items.push(item);
        }
    }
    items
}

/// Builds the shared filter → sort → selection chain every view wraps the
/// raw item `model` with. All three views observe the same underlying
/// `ListStore`, so a single directory load or search-filter change updates
/// whichever view is currently visible — `GtkGridView`/`GtkColumnView`
/// virtualize rendering, so this stays cheap even for huge directories.
pub(crate) fn build_selection(
    model: &gio::ListStore,
    filter: &gtk4::CustomFilter,
    sorter: Option<impl IsA<gtk4::Sorter>>,
) -> gtk4::MultiSelection {
    let filtered = gtk4::FilterListModel::new(Some(model.clone()), Some(filter.clone()));
    let sorted = gtk4::SortListModel::new(Some(filtered), sorter);
    gtk4::MultiSelection::new(Some(sorted))
}

/// Shared `GtkGridView` builder behind the Icon and Compact views: only the
/// icon pixel size and item layout orientation differ between them.
pub(crate) fn build_grid_view(
    selection: &gtk4::MultiSelection,
    icon_size: i32,
    horizontal_item: bool,
    thumbnails: Rc<ThumbnailService>,
    dnd_wiring: DndWiring,
    on_activate: impl Fn(u32) + 'static,
) -> gtk4::GridView {
    let factory = gtk4::SignalListItemFactory::new();

    {
        let dnd_wiring = dnd_wiring.clone();
        let selection = selection.clone();
        factory.connect_setup(move |_, list_item| {
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("factory item must be ListItem");
            let orientation = if horizontal_item {
                gtk4::Orientation::Horizontal
            } else {
                gtk4::Orientation::Vertical
            };
            let item_box = gtk4::Box::new(orientation, 6);
            item_box.set_margin_top(6);
            item_box.set_margin_bottom(6);
            item_box.set_margin_start(6);
            item_box.set_margin_end(6);

            let icon = gtk4::Image::new();
            icon.set_pixel_size(icon_size);
            item_box.append(&icon);

            let label = gtk4::Label::new(None);
            label.set_single_line_mode(true);
            label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
            if horizontal_item {
                label.set_xalign(0.0);
            } else {
                label.set_max_width_chars(16);
                label.set_justify(gtk4::Justification::Center);
            }
            item_box.append(&label);

            list_item.set_child(Some(&item_box));

            attach_row_dnd(&item_box, list_item, &dnd_wiring, &selection);
        });
    }

    {
        let thumbnails = thumbnails.clone();
        factory.connect_bind(move |_, list_item| {
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("factory item must be ListItem");
            let Some(item) = list_item
                .item()
                .and_then(|o| o.downcast::<glib::BoxedAnyObject>().ok())
            else {
                return;
            };
            let file_item = item.borrow::<FileItem>();

            let Some(item_box) = list_item
                .child()
                .and_then(|w| w.downcast::<gtk4::Box>().ok())
            else {
                return;
            };
            // Faz 14: recycled list items must have the class explicitly
            // cleared for non-hidden entries, not just set for hidden ones.
            if file_item.metadata.is_hidden {
                item_box.add_css_class("veyra-hidden-item");
            } else {
                item_box.remove_css_class("veyra-hidden-item");
            }
            let mut child = item_box.first_child();
            if let Some(icon) = child.and_then(|w| w.downcast::<gtk4::Image>().ok()) {
                icon.set_icon_name(Some(icon_name_for(&file_item)));
                thumbnails.bind(&icon, &file_item);
                child = icon.next_sibling();
            } else {
                child = None;
            }
            if let Some(label) = child.and_then(|w| w.downcast::<gtk4::Label>().ok()) {
                label.set_text(file_item.name());
                label.set_tooltip_text(Some(file_item.name()));
            }
        });
    }

    {
        let thumbnails = thumbnails.clone();
        factory.connect_unbind(move |_, list_item| {
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("factory item must be ListItem");
            if let Some(icon) = list_item
                .child()
                .and_then(|w| w.downcast::<gtk4::Box>().ok())
                .and_then(|b| b.first_child())
                .and_then(|w| w.downcast::<gtk4::Image>().ok())
            {
                thumbnails.unbind(&icon);
            }
        });
    }

    let grid_view = gtk4::GridView::new(Some(selection.clone()), Some(factory));
    grid_view.set_enable_rubberband(true);
    grid_view.connect_activate(move |_, position| on_activate(position));
    attach_background_drop(&grid_view, &dnd_wiring);
    grid_view
}

/// Attaches a drag source (so `row` can be dragged out — the whole current
/// multi-selection if `row`'s own item is part of it, otherwise just that
/// one item, matching Faz "Ara Faz" B/C multi-drag) and a folder-only drop
/// target to a recycled list-item row. Both callbacks re-resolve "the item
/// this row currently shows" live via `list_item.item()` rather than
/// capturing the `FileItem` at setup time, since `connect_bind` swaps it out
/// from under the same row as the list scrolls.
pub(crate) fn attach_row_dnd(
    row: &impl IsA<gtk4::Widget>,
    list_item: &gtk4::ListItem,
    dnd_wiring: &DndWiring,
    selection: &gtk4::MultiSelection,
) {
    let current_item = {
        let weak = list_item.downgrade();
        move || -> Option<FileItem> {
            let list_item = weak.upgrade()?;
            let boxed = list_item.item()?.downcast::<glib::BoxedAnyObject>().ok()?;
            let item = boxed.borrow::<FileItem>().clone();
            Some(item)
        }
    };

    // If the row being dragged is part of a larger current multi-selection,
    // drag every selected item's path; otherwise just this row's own item.
    let drag_paths = {
        let current_item = current_item.clone();
        let list_item = list_item.downgrade();
        let selection = selection.clone();
        move || -> Option<(Vec<veyra_filesystem::VeyraPath>, &'static str)> {
            let item = current_item()?;
            let icon = icon_name_for(&item);
            let position = list_item.upgrade().map(|li| li.position());
            let is_multi = position
                .map(|pos| selection.is_selected(pos) && selection.selection().size() > 1)
                .unwrap_or(false);
            let paths = if is_multi {
                selected_items(&selection)
                    .into_iter()
                    .map(|i| i.path)
                    .collect()
            } else {
                vec![item.path]
            };
            Some((paths, icon))
        }
    };

    {
        let drag_paths = drag_paths.clone();
        dnd::attach_drag_source(row, gdk::BUTTON_PRIMARY, drag_paths);
    }
    dnd::attach_drag_source(row, gdk::BUTTON_SECONDARY, drag_paths);

    let accept = {
        let current_item = current_item.clone();
        move || current_item().is_some_and(|item| item.kind().is_directory())
    };
    let destination = {
        let current_item = current_item.clone();
        move || {
            current_item()
                .filter(|item| item.kind().is_directory())
                .map(|item| item.path)
        }
    };
    dnd::attach_drop_target(row, accept, destination, dnd_wiring.execute.clone());
}

/// Attaches the whole-view background drop target: any drop GTK doesn't
/// deliver to a specific folder row (empty space, or a non-directory row's
/// `accept` returning `false`) bubbles up to this one, targeting the tab's
/// current directory.
pub(crate) fn attach_background_drop(view: &impl IsA<gtk4::Widget>, dnd_wiring: &DndWiring) {
    let current_dir = dnd_wiring.current_dir.clone();
    dnd::attach_drop_target(
        view,
        || true,
        move || Some(current_dir()),
        dnd_wiring.execute.clone(),
    );
}

/// Standard Adwaita/GNOME symbolic icon name for `item`.
pub(crate) fn icon_name_for(item: &FileItem) -> &'static str {
    match item.kind() {
        FileKind::Directory => "folder-symbolic",
        FileKind::Symlink {
            is_broken: true, ..
        } => "action-unavailable-symbolic",
        FileKind::Symlink { .. } => "folder-symbolic",
        FileKind::Socket => "network-server-symbolic",
        FileKind::Fifo | FileKind::BlockDevice | FileKind::CharDevice => "drive-harddisk-symbolic",
        FileKind::Unknown => "text-x-generic-symbolic",
        FileKind::Regular if item.metadata.is_executable() => "system-run-symbolic",
        FileKind::Regular => "text-x-generic-symbolic",
    }
}
