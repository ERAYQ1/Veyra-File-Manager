//! Permanent-delete confirmation. Rule #38/#39: destructive, irreversible
//! deletion must never fire from a bare `Delete` keypress — that trashes
//! instead (see `window.rs`) — and permanent delete always requires this
//! explicit `AdwAlertDialog` confirmation.

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use veyra_filesystem::VeyraPath;

/// Shows the confirmation dialog for permanently deleting `items`, calling
/// `on_confirm` only if the user picks the destructive response.
pub(crate) fn show(
    parent: &impl IsA<gtk4::Widget>,
    items: &[VeyraPath],
    on_confirm: impl FnOnce() + 'static,
) {
    let (heading, body) = match items {
        [only] => (
            "Permanently Delete?".to_string(),
            format!(
                "\"{}\" will be permanently deleted. This cannot be undone.",
                only.file_name().unwrap_or_else(|| only.to_string())
            ),
        ),
        _ => (
            format!("Permanently Delete {} Items?", items.len()),
            format!(
                "{} items will be permanently deleted. This cannot be undone.",
                items.len()
            ),
        ),
    };

    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .body(body)
        .build();
    dialog.add_responses(&[("cancel", "Cancel"), ("delete", "Delete Permanently")]);
    dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");

    dialog.choose(parent, gtk4::gio::Cancellable::NONE, move |response| {
        if response == "delete" {
            on_confirm();
        }
    });
}
