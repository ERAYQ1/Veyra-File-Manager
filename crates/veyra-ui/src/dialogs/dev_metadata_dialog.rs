//! Faz 39: the Developer Mode "Developer Metadata Inspector" — inode,
//! device id, octal permissions, MIME type, and hardlink count for a single
//! selected entry. Inode/permissions/MIME are already carried on `FileItem`
//! (Faz 12's `stat()` upgrade already loads them for Properties); device id
//! and hardlink count cost an extra `stat_advanced` call, fetched off the
//! GTK main thread same as `properties_dialog`'s Advanced page.

use libadwaita as adw;
use libadwaita::prelude::*;

use veyra_filesystem::FileItem;

use crate::fs_async;
use crate::i18n::t;

pub(crate) fn show(window: &adw::ApplicationWindow, item: &FileItem) {
    let dialog = adw::Dialog::builder()
        .title(t("dev.metadata.title"))
        .content_width(440)
        .content_height(320)
        .build();

    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_show_end_title_buttons(false);
    let close_button = gtk4::Button::with_label(t("dev.metadata.close"));
    header.pack_start(&close_button);
    {
        let dialog = dialog.clone();
        close_button.connect_clicked(move |_| {
            dialog.close();
        });
    }
    toolbar_view.add_top_bar(&header);

    let group = adw::PreferencesGroup::builder()
        .title(item.name().to_string())
        .build();

    let unknown = t("dev.metadata.unknown");

    let inode_row = info_row(
        t("dev.metadata.inode"),
        &item
            .metadata
            .inode
            .map_or_else(|| unknown.to_string(), |i| i.to_string()),
    );
    group.add(&inode_row);

    let permissions_row = info_row(
        t("dev.metadata.permissions"),
        &item
            .metadata
            .permissions
            .map_or_else(|| unknown.to_string(), |p| p.octal_string()),
    );
    group.add(&permissions_row);

    let mime_row = info_row(t("dev.metadata.mime_type"), &item.metadata.mime_type);
    group.add(&mime_row);

    let device_row = info_row(t("dev.metadata.device_id"), unknown);
    group.add(&device_row);

    let links_row = info_row(t("dev.metadata.hard_links"), unknown);
    group.add(&links_row);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.append(&group);
    toolbar_view.set_content(Some(&content));
    dialog.set_child(Some(&toolbar_view));

    let path = item.path.clone();
    fs_async::run_blocking(
        move || veyra_filesystem::stat_advanced(&path),
        move |result| {
            if let Ok(info) = result {
                device_row.set_subtitle(
                    &info
                        .device_id
                        .map_or_else(|| t("dev.metadata.unknown").to_string(), |d| d.to_string()),
                );
                links_row.set_subtitle(
                    &info
                        .hard_link_count
                        .map_or_else(|| t("dev.metadata.unknown").to_string(), |n| n.to_string()),
                );
            }
        },
    );

    dialog.present(Some(window));
}

fn info_row(title: &str, value: &str) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title(title)
        .subtitle(value)
        .subtitle_selectable(true)
        .build()
}
