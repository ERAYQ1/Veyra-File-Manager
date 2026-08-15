//! Faz 15: confirmation before `win.clear-recent-history` empties the XDG
//! recent-files registry. Not destructive to any actual file (only to the
//! Recent Files list itself), but still irreversible, so it gets the same
//! `AdwAlertDialog` treatment as permanent delete (Rule #38).

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

/// Shows the confirmation dialog for clearing recent file history, calling
/// `on_confirm` only if the user picks the destructive response.
pub(crate) fn show(parent: &impl IsA<gtk4::Widget>, on_confirm: impl FnOnce() + 'static) {
    let dialog = adw::AlertDialog::builder()
        .heading("Clear recent file history?")
        .body("Veyra's list of recently opened files will be cleared. This cannot be undone.")
        .build();
    dialog.add_responses(&[("cancel", "Cancel"), ("clear", "Clear History")]);
    dialog.set_response_appearance("clear", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");

    dialog.choose(parent, gtk4::gio::Cancellable::NONE, move |response| {
        if response == "clear" {
            on_confirm();
        }
    });
}
