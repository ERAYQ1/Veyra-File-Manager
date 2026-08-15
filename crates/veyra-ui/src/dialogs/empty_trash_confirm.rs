//! Faz 18: confirmation before `win.empty-trash` permanently deletes every
//! item currently in the Trash. Same `AdwAlertDialog` treatment as permanent
//! delete (Rule #38/#39) — irreversible, so it never fires from a bare
//! button click.

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

/// Shows the confirmation dialog for emptying the Trash, calling
/// `on_confirm` only if the user picks the destructive response.
pub(crate) fn show(parent: &impl IsA<gtk4::Widget>, on_confirm: impl FnOnce() + 'static) {
    let dialog = adw::AlertDialog::builder()
        .heading("Empty all items from Trash?")
        .body("This action cannot be undone.")
        .build();
    dialog.add_responses(&[("cancel", "Cancel"), ("empty", "Empty Trash")]);
    dialog.set_response_appearance("empty", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");

    dialog.choose(parent, gtk4::gio::Cancellable::NONE, move |response| {
        if response == "empty" {
            on_confirm();
        }
    });
}
