//! Faz 39: the Developer Mode "Calculate Checksums" dialog — computes MD5,
//! SHA-256, and SHA-512 (Faz 41) for a single file in one background
//! streaming pass (`dev_tools::compute_checksums`) and shows all three with
//! a one-click copy button each, plus a paste-to-verify field. Cancels the
//! computation if the dialog is closed before it finishes (Rule #13), same
//! pattern as `properties_dialog`'s "Contains" row.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use veyra_filesystem::{OperationControl, VeyraPath};

use crate::dev_tools::{self, ChecksumResult};
use crate::fs_async;
use crate::i18n::{t, t_fmt};

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
        .content_height(360)
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
    let sha512_row = checksum_row("SHA-512");
    group.add(&md5_row.row);
    group.add(&sha256_row.row);
    group.add(&sha512_row.row);

    let verify = build_verify_section();

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.append(&group);
    content.append(&verify.widget);
    toolbar_view.set_content(Some(&content));
    dialog.set_child(Some(&toolbar_view));

    let control = OperationControl::new();
    {
        let control = control.clone();
        dialog.connect_closed(move |_| control.cancel());
    }

    let computed: Rc<RefCell<Option<ChecksumResult>>> = Rc::new(RefCell::new(None));
    {
        let computed = computed.clone();
        let status_label = verify.status_label.clone();
        verify
            .entry
            .connect_changed(move |entry| update_verify_status(&computed, entry, &status_label));
    }

    fs_async::run_blocking(
        move || dev_tools::compute_checksums(&local, &control),
        move |result| match result {
            Ok(checksums) => {
                md5_row.finish(&checksums.md5);
                sha256_row.finish(&checksums.sha256);
                sha512_row.finish(&checksums.sha512);
                *computed.borrow_mut() = Some(checksums);
                update_verify_status(&computed, &verify.entry, &verify.status_label);
            }
            Err(err) => {
                md5_row.fail(&err.to_string());
                sha256_row.fail(&err.to_string());
                sha512_row.fail(&err.to_string());
            }
        },
    );

    dialog.present(Some(window));
}

/// The "paste expected checksum to verify" entry plus its match/mismatch
/// status label, shared by this dialog and `properties_dialog`'s Checksums
/// group (Faz 41).
pub(crate) struct VerifySection {
    pub widget: gtk4::Widget,
    pub entry: gtk4::Entry,
    pub status_label: gtk4::Label,
}

/// Builds the verify entry + status label, unbound to any particular
/// `ChecksumResult` — callers wire `connect_changed` themselves so they can
/// re-run the comparison both on every keystroke and once the background
/// hash computation lands.
pub(crate) fn build_verify_section() -> VerifySection {
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 6);

    let entry = gtk4::Entry::new();
    entry.set_placeholder_text(Some(t("dev.checksum.verify_placeholder")));
    entry.update_property(&[gtk4::accessible::Property::Label(t(
        "dev.checksum.verify_placeholder",
    ))]);
    container.append(&entry);

    let status_label = gtk4::Label::new(None);
    status_label.set_xalign(0.0);
    status_label.set_wrap(true);
    status_label.set_visible(false);
    container.append(&status_label);

    VerifySection {
        widget: container.upcast(),
        entry,
        status_label,
    }
}

/// Re-evaluates `entry`'s current text against `computed`'s digests (if the
/// background hash has landed yet) and updates `status_label` accordingly:
/// hidden while empty, a green "Matches …" once a digest matches, a red
/// "Checksum mismatch" otherwise (Faz 41).
pub(crate) fn update_verify_status(
    computed: &Rc<RefCell<Option<ChecksumResult>>>,
    entry: &gtk4::Entry,
    status_label: &gtk4::Label,
) {
    let text = entry.text();
    if text.trim().is_empty() {
        status_label.set_visible(false);
        status_label.remove_css_class("success");
        status_label.remove_css_class("error");
        return;
    }

    let borrowed = computed.borrow();
    let Some(result) = borrowed.as_ref() else {
        // Hash still computing — nothing to compare against yet.
        status_label.set_visible(false);
        return;
    };

    status_label.set_visible(true);
    match dev_tools::matching_algorithm(result, &text) {
        Some(algorithm) => {
            status_label.set_text(&t_fmt(
                "dev.checksum.matches",
                &[("algorithm", algorithm.label())],
            ));
            status_label.remove_css_class("error");
            status_label.add_css_class("success");
        }
        None => {
            status_label.set_text(t("dev.checksum.mismatch"));
            status_label.remove_css_class("success");
            status_label.add_css_class("error");
        }
    }
}

/// One checksum's row: starts showing a spinner, then swaps in the hex
/// digest plus a copy button once `finish` lands (or an error subtitle on
/// `fail`). Shared with `properties_dialog`'s Checksums group (Faz 41).
pub(crate) struct ChecksumRow {
    pub row: adw::ActionRow,
    spinner: gtk4::Spinner,
    copy_button: gtk4::Button,
}

impl ChecksumRow {
    pub(crate) fn finish(&self, digest: &str) {
        self.row.set_subtitle(digest);
        self.spinner.set_visible(false);
        self.spinner.stop();
        self.copy_button.set_visible(true);
        let digest = digest.to_string();
        self.copy_button.connect_clicked(move |button| {
            button.clipboard().set_text(&digest);
        });
    }

    pub(crate) fn fail(&self, message: &str) {
        self.row.set_subtitle(message);
        self.spinner.set_visible(false);
        self.spinner.stop();
    }
}

pub(crate) fn checksum_row(algorithm: &str) -> ChecksumRow {
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
