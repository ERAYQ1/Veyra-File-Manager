//! Veyra GTK4 + Libadwaita user interface layer.
//!
//! Faz 3 provides a usable file manager shell: sidebar (Places/Devices),
//! header bar (navigation, breadcrumbs, search, view switcher), status bar,
//! and three item views (Icon, Compact, Details) backed by an
//! `AdwNavigationSplitView`. All filesystem I/O runs off the GTK main
//! thread (`fs_async`), per Rule #14.

#![forbid(unsafe_code)]

mod breadcrumbs;
mod fs_async;
mod headerbar;
mod sidebar;
mod state;
mod statusbar;
mod views;
mod window;

use gtk4::glib;
use libadwaita::prelude::*;
use libadwaita::Application;

use veyra_filesystem::VeyraPath;

/// Builds and runs the Veyra GTK application under the given D-Bus application ID.
///
/// Blocks the calling thread until the GTK main loop exits. Must be called on
/// the same thread the process started on (GTK main thread requirement).
pub fn run(app_id: &str) -> glib::ExitCode {
    let app = Application::builder().application_id(app_id).build();

    app.connect_activate(|app| {
        tracing::info!("activating primary window");
        let start_dir = VeyraPath::from_local(glib::home_dir());
        window::build_window(app, start_dir).present();
    });

    app.run()
}
