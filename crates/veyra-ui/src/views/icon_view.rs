use gtk4::gio;
use gtk4::prelude::*;

use crate::views::{build_grid_view, build_selection, default_sorter, item_at};

const ICON_SIZE: i32 = 48;

/// Large scalable-icon grid, folders first then case-insensitive name order.
pub(crate) fn build_icon_view(
    model: &gio::ListStore,
    filter: &gtk4::CustomFilter,
    on_open: impl Fn(veyra_filesystem::FileItem) + 'static,
) -> gtk4::Widget {
    let selection = build_selection(model, filter, Some(default_sorter()));
    let selection_for_activate = selection.clone();

    let grid_view = build_grid_view(&selection, ICON_SIZE, false, move |position| {
        if let Some(item) = item_at(&selection_for_activate, position) {
            on_open(item);
        }
    });
    grid_view.set_min_columns(2);

    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&grid_view)
        .build();

    scrolled.upcast()
}
