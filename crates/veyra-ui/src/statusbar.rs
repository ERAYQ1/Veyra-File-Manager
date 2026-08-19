use gtk4::prelude::*;

use crate::i18n::t_plural;

/// Handles to the two status bar labels: item count (left) and free disk
/// space for the current location (right).
#[derive(Clone)]
pub(crate) struct StatusBarHandles {
    pub widget: gtk4::Box,
    pub left_label: gtk4::Label,
    pub right_label: gtk4::Label,
}

pub(crate) fn build() -> StatusBarHandles {
    let widget = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    widget.set_margin_start(12);
    widget.set_margin_end(12);
    widget.set_margin_top(4);
    widget.set_margin_bottom(4);

    let left_label = gtk4::Label::new(Some(&t_plural("status.items_count", 0, &[("count", "0")])));
    left_label.set_xalign(0.0);
    left_label.add_css_class("dim-label");

    let right_label = gtk4::Label::new(None);
    right_label.set_xalign(1.0);
    right_label.add_css_class("dim-label");
    right_label.set_hexpand(true);
    right_label.set_halign(gtk4::Align::End);

    widget.append(&left_label);
    widget.append(&right_label);

    StatusBarHandles {
        widget,
        left_label,
        right_label,
    }
}
