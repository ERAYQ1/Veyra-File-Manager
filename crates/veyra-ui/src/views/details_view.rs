use std::cell::{Cell, RefCell};
use std::cmp::Ordering as StdOrdering;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

use veyra_filesystem::FileItem;

use crate::dnd::DndWiring;
use crate::sorting::{SortConfig, SortKey, SortOrder};
use crate::thumbnails::ThumbnailService;
use crate::views::{icon_name_for, item_at};

/// The tab-level sort state Details needs on top of its own columns: the
/// shared `SortConfig` and `CustomSorter` every view sorts by, and the
/// reentrancy guard shared with `TabPage::resort` (see `tab_page.rs`).
pub(crate) struct DetailsSortWiring {
    pub sort_config: Rc<RefCell<SortConfig>>,
    pub sorter: gtk4::CustomSorter,
    pub sync_guard: Rc<Cell<bool>>,
}

/// Everything a caller needs back from a built Details view: the widget to
/// place, its selection, and the pieces `TabPage` keeps to mirror
/// `SortConfig` onto the column headers (see `TabPage::resort`).
pub(crate) struct DetailsViewHandles {
    pub widget: gtk4::Widget,
    pub selection: gtk4::SingleSelection,
    pub column_view: gtk4::ColumnView,
    pub sort_columns: Rc<Vec<(SortKey, gtk4::ColumnViewColumn)>>,
}

/// Tabular view: Name, Size, Type, Modified, Permissions. Each sortable
/// column header drives the tab's shared `SortConfig` (Faz 13) — clicking a
/// header updates `sort_config` and re-sorts Icon/Compact to match, and
/// picking a criterion from the header bar's Sort & Filter menu likewise
/// updates this view's header indicator via `sort_by_column`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_details_view(
    model: &gio::ListStore,
    filter: &gtk4::CustomFilter,
    sort: DetailsSortWiring,
    on_open: impl Fn(FileItem) + 'static,
    has_clipboard: Rc<dyn Fn() -> bool>,
    split_active: Rc<dyn Fn() -> bool>,
    is_trash: Rc<dyn Fn() -> bool>,
    thumbnails: Rc<ThumbnailService>,
    dnd_wiring: DndWiring,
) -> DetailsViewHandles {
    let DetailsSortWiring {
        sort_config,
        sorter,
        sync_guard,
    } = sort;
    let column_view = gtk4::ColumnView::new(None::<gtk4::SingleSelection>);
    column_view.set_show_row_separators(true);

    let name_col = name_column(thumbnails, dnd_wiring.clone());
    column_view.append_column(&name_col);
    let size_col = text_column("Size", 100, size_label, |a, b| {
        a.metadata.size_bytes.cmp(&b.metadata.size_bytes)
    });
    column_view.append_column(&size_col);
    let type_col = text_column("Type", 140, type_label, |a, b| {
        type_label(a).cmp(&type_label(b))
    });
    column_view.append_column(&type_col);
    let modified_col = text_column("Modified", 170, modified_label, |a, b| {
        a.metadata.modified.cmp(&b.metadata.modified)
    });
    column_view.append_column(&modified_col);
    column_view.append_column(&text_column(
        "Permissions",
        110,
        permissions_label,
        |a, b| permissions_label(a).cmp(&permissions_label(b)),
    ));

    // Only columns with a direct `SortKey` counterpart participate in
    // header-click <-> menu synchronization; Permissions has none.
    let sort_columns = Rc::new(vec![
        (SortKey::Name, name_col.clone()),
        (SortKey::Size, size_col),
        (SortKey::Type, type_col),
        (SortKey::Modified, modified_col),
    ]);

    let filtered = gtk4::FilterListModel::new(Some(model.clone()), Some(filter.clone()));
    let sort_model = gtk4::SortListModel::new(Some(filtered), Some(sorter.clone()));
    let selection = gtk4::SingleSelection::new(Some(sort_model));
    column_view.set_model(Some(&selection));

    // GtkColumnView's own header-click sorter (`ColumnViewSorter`) is kept
    // separate from the model's sorter above: we only use it to get native
    // header-click handling and sort-indicator arrows, then mirror its
    // result into the tab's shared `SortConfig` so Icon/Compact follow.
    column_view.sort_by_column(Some(&name_col), sort_config.borrow().order.to_gtk());
    if let Some(native) = column_view.sorter() {
        let sort_columns = sort_columns.clone();
        let sort_config = sort_config.clone();
        let sync_guard = sync_guard.clone();
        let sorter = sorter.clone();
        native.connect_changed(move |native, _change| {
            if sync_guard.get() {
                return;
            }
            let Some(cv_sorter) = native.downcast_ref::<gtk4::ColumnViewSorter>() else {
                return;
            };
            let Some(primary_col) = cv_sorter.primary_sort_column() else {
                return;
            };
            let Some((key, _)) = sort_columns.iter().find(|(_, col)| *col == primary_col) else {
                return;
            };
            let order = SortOrder::from_gtk(cv_sorter.primary_sort_order());
            {
                let mut config = sort_config.borrow_mut();
                if config.key == *key && config.order == order {
                    return;
                }
                config.key = *key;
                config.order = order;
            }
            sorter.changed(gtk4::SorterChange::Different);
        });
    }

    let selection_for_activate = selection.clone();
    column_view.connect_activate(move |_, position| {
        if let Some(item) = item_at(&selection_for_activate, position) {
            on_open(item);
        }
    });
    crate::context_menu::attach(
        &column_view,
        &selection,
        has_clipboard,
        split_active,
        is_trash,
    );
    crate::views::attach_background_drop(&column_view, &dnd_wiring);

    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .child(&column_view)
        .build();

    DetailsViewHandles {
        widget: scrolled.upcast(),
        selection,
        column_view,
        sort_columns,
    }
}

fn name_column(thumbnails: Rc<ThumbnailService>, dnd_wiring: DndWiring) -> gtk4::ColumnViewColumn {
    let factory = gtk4::SignalListItemFactory::new();

    factory.connect_setup(move |_, list_item| {
        let list_item = list_item
            .downcast_ref::<gtk4::ListItem>()
            .expect("factory item must be ListItem");
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        row.set_margin_top(4);
        row.set_margin_bottom(4);
        row.set_margin_start(6);
        row.set_margin_end(6);

        let icon = gtk4::Image::new();
        icon.set_pixel_size(16);
        row.append(&icon);

        let label = gtk4::Label::new(None);
        label.set_xalign(0.0);
        label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
        row.append(&label);

        list_item.set_child(Some(&row));

        crate::views::attach_row_dnd(&row, list_item, &dnd_wiring);
    });

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

        let Some(row) = list_item
            .child()
            .and_then(|w| w.downcast::<gtk4::Box>().ok())
        else {
            return;
        };
        // Faz 14: recycled list items must have the class explicitly
        // cleared for non-hidden entries, not just set for hidden ones.
        if file_item.metadata.is_hidden {
            row.add_css_class("veyra-hidden-item");
        } else {
            row.remove_css_class("veyra-hidden-item");
        }
        let mut child = row.first_child();
        if let Some(icon) = child.and_then(|w| w.downcast::<gtk4::Image>().ok()) {
            icon.set_icon_name(Some(icon_name_for(&file_item)));
            thumbnails.bind(&icon, &file_item);
            child = icon.next_sibling();
        } else {
            child = None;
        }
        if let Some(label) = child.and_then(|w| w.downcast::<gtk4::Label>().ok()) {
            label.set_text(file_item.name());
        }
    });

    let column = gtk4::ColumnViewColumn::new(Some("Name"), Some(factory));
    column.set_expand(true);
    column.set_resizable(true);
    column.set_sorter(Some(&field_sorter(|a, b| {
        a.name().to_lowercase().cmp(&b.name().to_lowercase())
    })));
    column
}

fn text_column(
    title: &str,
    fixed_width: i32,
    render: fn(&FileItem) -> String,
    compare: fn(&FileItem, &FileItem) -> StdOrdering,
) -> gtk4::ColumnViewColumn {
    let factory = gtk4::SignalListItemFactory::new();

    factory.connect_setup(|_, list_item| {
        let list_item = list_item
            .downcast_ref::<gtk4::ListItem>()
            .expect("factory item must be ListItem");
        let label = gtk4::Label::new(None);
        label.set_xalign(0.0);
        label.set_margin_start(6);
        label.set_margin_end(6);
        list_item.set_child(Some(&label));
    });

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

        if let Some(label) = list_item
            .child()
            .and_then(|w| w.downcast::<gtk4::Label>().ok())
        {
            label.set_text(&render(&file_item));
            // Faz 14: dim the rest of the row to match the Name cell.
            if file_item.metadata.is_hidden {
                label.add_css_class("veyra-hidden-item");
            } else {
                label.remove_css_class("veyra-hidden-item");
            }
        }
    });

    let column = gtk4::ColumnViewColumn::new(Some(title), Some(factory));
    column.set_fixed_width(fixed_width);
    column.set_resizable(true);
    column.set_sorter(Some(&field_sorter(compare)));
    column
}

fn field_sorter(compare: fn(&FileItem, &FileItem) -> StdOrdering) -> gtk4::CustomSorter {
    gtk4::CustomSorter::new(move |a, b| {
        let a = a
            .downcast_ref::<glib::BoxedAnyObject>()
            .expect("model item must be BoxedAnyObject<FileItem>")
            .borrow::<FileItem>();
        let b = b
            .downcast_ref::<glib::BoxedAnyObject>()
            .expect("model item must be BoxedAnyObject<FileItem>")
            .borrow::<FileItem>();
        compare(&a, &b).into()
    })
}

fn size_label(item: &FileItem) -> String {
    if item.kind().is_directory() {
        String::new()
    } else {
        item.metadata.size_human()
    }
}

fn type_label(item: &FileItem) -> String {
    use veyra_filesystem::FileKind;

    match item.kind() {
        FileKind::Directory => "Folder".to_string(),
        FileKind::Symlink {
            is_broken: true, ..
        } => "Broken Link".to_string(),
        FileKind::Symlink { .. } => "Link".to_string(),
        FileKind::Socket => "Socket".to_string(),
        FileKind::Fifo => "Named Pipe".to_string(),
        FileKind::BlockDevice => "Block Device".to_string(),
        FileKind::CharDevice => "Character Device".to_string(),
        FileKind::Unknown => "Unknown".to_string(),
        FileKind::Regular => item.metadata.mime_type.clone(),
    }
}

fn modified_label(item: &FileItem) -> String {
    item.metadata
        .modified
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default()
}

fn permissions_label(item: &FileItem) -> String {
    item.metadata
        .permissions
        .map(|p| p.symbolic_string())
        .unwrap_or_default()
}
