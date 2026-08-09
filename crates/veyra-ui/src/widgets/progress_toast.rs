//! Bottom progress bar for the active bulk Copy/Move/Trash/Delete
//! operation: current file, percent, byte total, and Pause/Resume/Cancel
//! controls. A `GtkRevealer` rather than an `AdwToast` — Faz 5 needs live
//! Pause/Resume buttons and a progress bar inside it, which a plain toast
//! (text + one action) can't host.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;

use veyra_filesystem::{format_size, OperationControl, OperationKind, Progress};

#[derive(Clone)]
pub(crate) struct ProgressToastHandles {
    pub widget: gtk4::Revealer,
    control: Rc<RefCell<Option<OperationControl>>>,
    verb: Rc<RefCell<&'static str>>,
    title_label: gtk4::Label,
    detail_label: gtk4::Label,
    progress_bar: gtk4::ProgressBar,
    pause_button: gtk4::Button,
}

pub(crate) fn build() -> ProgressToastHandles {
    let control: Rc<RefCell<Option<OperationControl>>> = Rc::new(RefCell::new(None));

    let title_label = gtk4::Label::new(None);
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
    pause_button.set_tooltip_text(Some("Pause"));
    pause_button.update_property(&[gtk4::accessible::Property::Label("Pause")]);
    {
        let control = control.clone();
        let pause_button_handle = pause_button.clone();
        pause_button.connect_clicked(move |_| {
            let Some(op) = control.borrow().clone() else {
                return;
            };
            if op.is_paused() {
                op.resume();
                pause_button_handle.set_icon_name("media-playback-pause-symbolic");
                pause_button_handle.set_tooltip_text(Some("Pause"));
            } else {
                op.pause();
                pause_button_handle.set_icon_name("media-playback-start-symbolic");
                pause_button_handle.set_tooltip_text(Some("Resume"));
            }
        });
    }

    let cancel_button = gtk4::Button::from_icon_name("process-stop-symbolic");
    cancel_button.set_tooltip_text(Some("Cancel"));
    cancel_button.update_property(&[gtk4::accessible::Property::Label("Cancel")]);
    {
        let control = control.clone();
        cancel_button.connect_clicked(move |_| {
            if let Some(op) = control.borrow().clone() {
                op.cancel();
            }
        });
    }

    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    content.set_margin_top(8);
    content.set_margin_bottom(8);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.append(&text_box);
    content.append(&pause_button);
    content.append(&cancel_button);
    content.add_css_class("toolbar");

    let widget = gtk4::Revealer::new();
    widget.set_child(Some(&content));
    widget.set_transition_type(gtk4::RevealerTransitionType::SlideUp);
    widget.set_reveal_child(false);

    ProgressToastHandles {
        widget,
        control,
        verb: Rc::new(RefCell::new("Working")),
        title_label,
        detail_label,
        progress_bar,
        pause_button,
    }
}

fn verb(kind: OperationKind) -> &'static str {
    match kind {
        OperationKind::Copy => "Copying",
        OperationKind::Move => "Moving",
        OperationKind::Trash => "Moving to Trash",
        OperationKind::Delete => "Deleting",
    }
}

/// Reveals the bar for a freshly-started operation and binds `control` so
/// Pause/Resume/Cancel act on it.
pub(crate) fn begin(
    handles: &ProgressToastHandles,
    control: &OperationControl,
    kind: OperationKind,
) {
    *handles.control.borrow_mut() = Some(control.clone());
    *handles.verb.borrow_mut() = verb(kind);
    handles
        .pause_button
        .set_icon_name("media-playback-pause-symbolic");
    handles.pause_button.set_tooltip_text(Some("Pause"));
    handles.title_label.set_label(verb(kind));
    handles.detail_label.set_label("");
    handles.progress_bar.set_fraction(0.0);
    handles.widget.set_reveal_child(true);
}

/// Updates the bar from a new `Progress` snapshot.
pub(crate) fn update(handles: &ProgressToastHandles, progress: &Progress) {
    let verb = *handles.verb.borrow();
    handles
        .title_label
        .set_label(&format!("{verb}… {}", progress.current_file));
    handles
        .progress_bar
        .set_fraction(progress.percent() / 100.0);

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
    handles.detail_label.set_label(&detail);
}

/// Hides the bar and releases the finished operation's control handle.
pub(crate) fn finish(handles: &ProgressToastHandles) {
    handles.widget.set_reveal_child(false);
    *handles.control.borrow_mut() = None;
}
