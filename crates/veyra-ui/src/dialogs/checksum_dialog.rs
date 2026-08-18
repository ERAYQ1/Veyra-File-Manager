//! Faz 39: the Developer Mode "Calculate Checksums" dialog — computes MD5
//! and SHA-256 for a single file in one background streaming pass
//! (`dev_tools::compute_checksums`) and shows both with a one-click copy
//! button each. Cancels the computation if the dialog is closed before it
//! finishes (Rule #13), same pattern as `properties_dialog`'s "Contains"
//! row.

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use veyra_filesystem::{OperationControl, VeyraPath};

use crate::dev_tools;
use crate::fs_async;
use crate::i18n::t;

/// Shows the checksum dialog for `path`, parented to `window`. No-op if
/// `path` isn't local (a remote/GVfs location has no local bytes to hash
/// off the GTK thread without first downloading it, which this dialog
/// doesn't attempt).
pub(crate) fn show(window: &adw::ApplicationWindow, path: &VeyraPath) {
    let Some(local) = path.as_local_path().map(std::path::Path::to_path_buf) else {
        return;
    };

    let dialog = adw::Dialog::builder()
        .title(t("dev.checksum.title"))
        .content_width(440)
        .content_height(240)
        .build();

    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_show_end_title_buttons(false);
    let close_button = gtk4::Button::with_label(t("dev.checksum.close"));
    header.pack_start(&close_button);
    {
        let dialog = dialog.clone();
        close_button.connect_clicked(move |_| {
            dialog.close();
        });
    }
    toolbar_view.add_top_bar(&header);

    let group = adw::PreferencesGroup::builder()
        .title(path.file_name().unwrap_or_default())
        .build();

    let md5_row = checksum_row("MD5");
    let sha256_row = checksum_row("SHA-256");
    group.add(&md5_row.row);
    group.add(&sha256_row.row);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.append(&group);
    toolbar_view.set_content(Some(&content));
    dialog.set_child(Some(&toolbar_view));

    let control = OperationControl::new();
    {
        let control = control.clone();
        dialog.connect_closed(move |_| control.cancel());
    }

    fs_async::run_blocking(
        move || dev_tools::compute_checksums(&local, &control),
        move |result| match result {
            Ok(checksums) => {
                md5_row.finish(&checksums.md5);
                sha256_row.finish(&checksums.sha256);
            }
            Err(err) => {
                md5_row.fail(&err.to_string());
                sha256_row.fail(&err.to_string());
            }
        },
    );

    dialog.present(Some(window));
}

/// One checksum's row: starts showing a spinner, then swaps in the hex
/// digest plus a copy button once `finish` lands (or an error subtitle on
/// `fail`).
struct ChecksumRow {
    row: adw::ActionRow,
    spinner: gtk4::Spinner,
    copy_button: gtk4::Button,
}

impl ChecksumRow {
    fn finish(&self, digest: &str) {
        self.row.set_subtitle(digest);
        self.spinner.set_visible(false);
        self.spinner.stop();
        self.copy_button.set_visible(true);
        let digest = digest.to_string();
        self.copy_button.connect_clicked(move |button| {
            button.clipboard().set_text(&digest);
        });
    }

    fn fail(&self, message: &str) {
        self.row.set_subtitle(message);
        self.spinner.set_visible(false);
        self.spinner.stop();
    }
}

fn checksum_row(algorithm: &str) -> ChecksumRow {
    let row = adw::ActionRow::builder()
        .title(algorithm)
        .subtitle(t("dev.checksum.computing"))
        .build();

    let spinner = gtk4::Spinner::new();
    spinner.set_spinning(true);
    row.add_suffix(&spinner);

    let copy_button = gtk4::Button::from_icon_name("edit-copy-symbolic");
    copy_button.set_valign(gtk4::Align::Center);
    copy_button.add_css_class("flat");
    copy_button.set_tooltip_text(Some(t("dev.checksum.copy")));
    copy_button.update_property(&[gtk4::accessible::Property::Label(t("dev.checksum.copy"))]);
    copy_button.set_visible(false);
    row.add_suffix(&copy_button);

    ChecksumRow {
        row,
        spinner,
        copy_button,
    }
}
