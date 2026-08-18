//! Faz 32 File Operation Queue: bottom panel listing every active bulk
//! Copy/Move/Trash/Delete (and Faz 19 Compress/Extract) operation as its own
//! row — current file, percent, byte total, and per-row Pause/Resume/Cancel
//! controls. Previously a single shared row got silently rebound to
//! whichever operation started most recently, so a second paste while a
//! first was still copying made the first un-pausable/un-cancellable from
//! the UI even though it kept running in the background. Each operation now
//! gets its own row, keyed by an `OperationId`, so any number can run and be
//! controlled concurrently.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::prelude::*;

use veyra_filesystem::{format_size, OperationControl, OperationKind, Progress};

use crate::i18n::t;

/// Identifies one row/operation for the lifetime of `begin`..`finish`.
pub(crate) type OperationId = u64;

struct Row {
    frame: gtk4::Box,
    verb: &'static str,
    title_label: gtk4::Label,
    detail_label: gtk4::Label,
    progress_bar: gtk4::ProgressBar,
}

#[derive(Clone)]
pub(crate) struct ProgressToastHandles {
    pub widget: gtk4::Revealer,
    list: gtk4::Box,
    rows: Rc<RefCell<HashMap<OperationId, Row>>>,
    next_id: Rc<Cell<OperationId>>,
}

pub(crate) fn build() -> ProgressToastHandles {
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 1);

    let scroller = gtk4::ScrolledWindow::new();
    scroller.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroller.set_max_content_height(240);
    scroller.set_propagate_natural_height(true);
    scroller.set_child(Some(&list));

    let widget = gtk4::Revealer::new();
    widget.set_child(Some(&scroller));
    widget.set_transition_type(gtk4::RevealerTransitionType::SlideUp);
    widget.set_reveal_child(false);

    ProgressToastHandles {
        widget,
        list,
        rows: Rc::new(RefCell::new(HashMap::new())),
        next_id: Rc::new(Cell::new(0)),
    }
}

fn verb(kind: OperationKind) -> &'static str {
    match kind {
        OperationKind::Copy => t("toast.copying"),
        OperationKind::Move => t("toast.moving"),
        OperationKind::Trash => t("toast.moving_to_trash"),
        OperationKind::Delete => t("toast.deleting"),
    }
}

/// Adds a row for a freshly-started operation and binds `control` so that
/// row's own Pause/Resume/Cancel buttons act on it. Returns the id later
/// calls to `update`/`finish` must pass to target this row specifically.
pub(crate) fn begin(
    handles: &ProgressToastHandles,
    control: &OperationControl,
    kind: OperationKind,
) -> OperationId {
    begin_with_verb(handles, control, verb(kind))
}

/// Like [`begin`], for operations outside the Copy/Move/Trash/Delete
/// `OperationKind` set (Faz 19's Compress/Extract, which share the same
/// `OperationControl`/`Progress` machinery but aren't bulk file ops).
pub(crate) fn begin_with_verb(
    handles: &ProgressToastHandles,
    control: &OperationControl,
    verb: &'static str,
) -> OperationId {
    let id = handles.next_id.get();
    handles.next_id.set(id + 1);

    let title_label = gtk4::Label::new(Some(verb));
    title_label.set_xalign(0.0);
    title_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    title_label.set_single_line_mode(true);

    let detail_label = gtk4::Label::new(None);
    detail_label.set_xalign(0.0);
    detail_label.add_css_class("dim-label");
    detail_label.add_css_class("caption");

    let progress_bar = gtk4::ProgressBar::new();
    progress_bar.set_hexpand(true);
    progress_bar.set_valign(gtk4::Align::Center);

    let text_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    text_box.set_hexpand(true);
    text_box.set_valign(gtk4::Align::Center);
    text_box.append(&title_label);
    text_box.append(&progress_bar);
    text_box.append(&detail_label);

    let pause_button = gtk4::Button::from_icon_name("media-playback-pause-symbolic");
    pause_button.set_tooltip_text(Some(t("toast.pause")));
    pause_button.update_property(&[gtk4::accessible::Property::Label(t("toast.pause"))]);
    {
        let control = control.clone();
        let pause_button_handle = pause_button.clone();
        pause_button.connect_clicked(move |_| {
            if control.is_paused() {
                control.resume();
                pause_button_handle.set_icon_name("media-playback-pause-symbolic");
                pause_button_handle.set_tooltip_text(Some(t("toast.pause")));
            } else {
                control.pause();
                pause_button_handle.set_icon_name("media-playback-start-symbolic");
                pause_button_handle.set_tooltip_text(Some(t("toast.resume")));
            }
        });
    }

    let cancel_button = gtk4::Button::from_icon_name("process-stop-symbolic");
    cancel_button.set_tooltip_text(Some(t("toast.cancel")));
    cancel_button.update_property(&[gtk4::accessible::Property::Label(t("toast.cancel"))]);
    {
        let control = control.clone();
        cancel_button.connect_clicked(move |_| control.cancel());
    }

    let frame = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    frame.set_margin_top(8);
    frame.set_margin_bottom(8);
    frame.set_margin_start(12);
    frame.set_margin_end(12);
    frame.append(&text_box);
    frame.append(&pause_button);
    frame.append(&cancel_button);
    frame.add_css_class("toolbar");

    handles.list.append(&frame);
    handles.rows.borrow_mut().insert(
        id,
        Row {
            frame,
            verb,
            title_label,
            detail_label,
            progress_bar,
        },
    );
    handles.widget.set_reveal_child(true);

    id
}

/// Updates row `id` from a new `Progress` snapshot. A no-op if that row
/// already finished (e.g. a straggling event after cancel).
pub(crate) fn update(handles: &ProgressToastHandles, id: OperationId, progress: &Progress) {
    let rows = handles.rows.borrow();
    let Some(row) = rows.get(&id) else {
        return;
    };

    row.title_label
        .set_label(&format!("{}… {}", row.verb, progress.current_file));
    row.progress_bar.set_fraction(progress.percent() / 100.0);

    let detail = if progress.bytes_total > 0 {
        format!(
            "{} / {}  ({:.0}%)",
            format_size(progress.bytes_done),
            format_size(progress.bytes_total),
            progress.percent()
        )
    } else if progress.file_count > 0 {
        format!("{} / {}", progress.file_index + 1, progress.file_count)
    } else {
        String::new()
    };
    row.detail_label.set_label(&detail);
}

/// Removes row `id`'s widget and releases its control handle. Hides the
/// whole queue panel once the last row is gone.
pub(crate) fn finish(handles: &ProgressToastHandles, id: OperationId) {
    let mut rows = handles.rows.borrow_mut();
    if let Some(row) = rows.remove(&id) {
        handles.list.remove(&row.frame);
    }
    if rows.is_empty() {
        handles.widget.set_reveal_child(false);
    }
}
