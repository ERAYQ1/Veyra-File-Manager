//! Faz 51: the actionable error dialog every file operation failure routes
//! through — headline, affected item, plain-language cause, a collapsible
//! technical-details section with a copy button, and whichever recovery
//! buttons apply (Try Again / Choose Another Location / Skip / Cancel).
//! Never shown directly; callers build an `error_ux::ActionableError` via
//! `error_ux::classify` first.

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::error_ux::{ActionableError, RecoveryAction};
use crate::i18n::t;

/// Shows `error` over `parent`, invoking `on_action` once with whichever
/// recovery button the user picked (including `Cancel`, so callers don't
/// need to special-case a dismissed dialog).
pub(crate) fn show(
    parent: &impl IsA<gtk4::Widget>,
    error: &ActionableError,
    on_action: impl Fn(RecoveryAction) + 'static,
) {
    let dialog = adw::AlertDialog::builder()
        .heading(&error.headline)
        .body(&error.target_name)
        .build();

    let content = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .build();

    let icon_row = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(8)
        .build();
    icon_row.append(&gtk4::Image::from_icon_name("dialog-error-symbolic"));
    let reason_label = gtk4::Label::builder()
        .label(&error.human_reason)
        .wrap(true)
        .xalign(0.0)
        .build();
    icon_row.append(&reason_label);
    content.append(&icon_row);

    let expander = adw::ExpanderRow::builder()
        .title(t("error.show_details"))
        .build();
    expander.add_css_class("card");

    let details_label = gtk4::Label::builder()
        .label(&error.technical_details)
        .wrap(true)
        .xalign(0.0)
        .selectable(true)
        .margin_start(12)
        .margin_end(12)
        .margin_top(8)
        .margin_bottom(8)
        .build();
    details_label.add_css_class("monospace");
    details_label.add_css_class("dim-label");
    expander.add_row(&details_label);

    let copy_row = adw::ActionRow::builder().build();
    let copy_button = gtk4::Button::builder()
        .label(t("error.copy_technical_details"))
        .valign(gtk4::Align::Center)
        .build();
    copy_button.add_css_class("flat");
    let technical_details = error.technical_details.clone();
    copy_button.connect_clicked(move |button| {
        button.clipboard().set_text(&technical_details);
    });
    copy_row.add_suffix(&copy_button);
    expander.add_row(&copy_row);

    let details_list = gtk4::ListBox::builder()
        .selection_mode(gtk4::SelectionMode::None)
        .build();
    details_list.add_css_class("boxed-list");
    details_list.append(&expander);
    content.append(&details_list);
    dialog.set_extra_child(Some(&content));

    for action in &error.recovery_actions {
        dialog.add_response(action.response_id(), &action.label());
        if *action == RecoveryAction::TryAgain {
            dialog
                .set_response_appearance(action.response_id(), adw::ResponseAppearance::Suggested);
        }
    }
    let default_action = error
        .recovery_actions
        .iter()
        .find(|a| **a == RecoveryAction::TryAgain)
        .or_else(|| error.recovery_actions.first())
        .copied()
        .unwrap_or(RecoveryAction::Cancel);
    dialog.set_default_response(Some(default_action.response_id()));
    dialog.set_close_response(RecoveryAction::Cancel.response_id());

    dialog.connect_response(None, move |_, response| {
        let action = error_action_for_response(response);
        on_action(action);
    });

    dialog.present(Some(parent));
}

fn error_action_for_response(response: &str) -> RecoveryAction {
    match response {
        "try-again" => RecoveryAction::TryAgain,
        "choose-location" => RecoveryAction::ChooseAnotherLocation,
        "skip" => RecoveryAction::Skip,
        _ => RecoveryAction::Cancel,
    }
}
