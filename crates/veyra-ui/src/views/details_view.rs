use std::cmp::Ordering as StdOrdering;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

use veyra_filesystem::FileItem;

use crate::views::{icon_name_for, item_at};

/// Tabular view: Name, Size, Type, Modified, Permissions — each column
/// sortable by clicking its header (`GtkColumnViewSorter`).
pub(crate) fn build_details_view(
    model: &gio::ListStore,
    filter: &gtk4::CustomFilter,
    on_open: impl Fn(FileItem) + 'static,
    has_clipboard: Rc<dyn Fn() -> bool>,
    split_active: Rc<dyn Fn() -> bool>,
) -> (gtk4::Widget, gtk4::SingleSelection) {
    let column_view = gtk4::ColumnView::new(None::<gtk4::SingleSelection>);
    column_view.set_show_row_separators(true);

    let name_col = name_column();
    column_view.append_column(&name_col);
    column_view.append_column(&text_column("Size", 100, size_label, |a, b| {
        a.metadata.size_bytes.cmp(&b.metadata.size_bytes)
    }));
    column_view.append_column(&text_column("Type", 140, type_label, |a, b| {
        type_label(a).cmp(&type_label(b))
    }));
    column_view.append_column(&text_column("Modified", 170, modified_label, |a, b| {
        a.metadata.modified.cmp(&b.metadata.modified)
    }));
    column_view.append_column(&text_column(
        "Permissions",
        110,
        permissions_label,
        |a, b| permissions_label(a).cmp(&permissions_label(b)),
    ));

    let filtered = gtk4::FilterListModel::new(Some(model.clone()), Some(filter.clone()));
    let sort_model = gtk4::SortListModel::new(Some(filtered), None::<gtk4::Sorter>);
    let selection = gtk4::SingleSelection::new(Some(sort_model.clone()));
    column_view.set_model(Some(&selection));

    // GtkColumnView builds its live, header-driven sorter once it has a
    // model; wire it into the model chain so clicking a header re-sorts.
    sort_model.set_sorter(column_view.sorter().as_ref());
    column_view.sort_by_column(Some(&name_col), gtk4::SortType::Ascending);

    let selection_for_activate = selection.clone();
    column_view.connect_activate(move |_, position| {
        if let Some(item) = item_at(&selection_for_activate, position) {
            on_open(item);
        }
    });
    crate::context_menu::attach(&column_view, &selection, has_clipboard, split_active);

    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .child(&column_view)
        .build();

    (scrolled.upcast(), selection)
}

fn name_column() -> gtk4::ColumnViewColumn {
    let factory = gtk4::SignalListItemFactory::new();

    factory.connect_setup(|_, list_item| {
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
    });

    factory.connect_bind(|_, list_item| {
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
        let mut child = row.first_child();
        if let Some(icon) = child.and_then(|w| w.downcast::<gtk4::Image>().ok()) {
            icon.set_icon_name(Some(icon_name_for(&file_item)));
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
        let label = gtk4::Label::new(None);
        label.set_xalign(0.0);
        label.set_margin_start(6);
        label.set_margin_end(6);
        list_item.set_child(Some(&label));
    });

    factory.connect_bind(move |_, list_item| {
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
