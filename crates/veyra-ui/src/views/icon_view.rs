use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;

use crate::thumbnails::ThumbnailService;
use crate::views::{build_grid_view, build_selection, item_at};

const ICON_SIZE: i32 = 48;

/// Large scalable-icon grid, ordered by the tab's shared `SortConfig`
/// (`sorter`, see `crate::sorting`) — identical ordering to Compact/Details.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_icon_view(
    model: &gio::ListStore,
    filter: &gtk4::CustomFilter,
    sorter: &gtk4::CustomSorter,
    on_open: impl Fn(veyra_filesystem::FileItem) + 'static,
    has_clipboard: Rc<dyn Fn() -> bool>,
    split_active: Rc<dyn Fn() -> bool>,
    is_trash: Rc<dyn Fn() -> bool>,
    thumbnails: Rc<ThumbnailService>,
) -> (gtk4::Widget, gtk4::SingleSelection) {
    let selection = build_selection(model, filter, Some(sorter.clone()));
    let selection_for_activate = selection.clone();

    let grid_view = build_grid_view(&selection, ICON_SIZE, false, thumbnails, move |position| {
        if let Some(item) = item_at(&selection_for_activate, position) {
            on_open(item);
        }
    });
    grid_view.set_min_columns(2);
    crate::context_menu::attach(
        &grid_view,
        &selection,
        has_clipboard,
        split_active,
        is_trash,
    );

    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .child(&grid_view)
        .build();

    (scrolled.upcast(), selection)
}
