//! Veyra GTK4 + Libadwaita user interface layer.
//!
//! Faz 1 provides the application shell: a single Libadwaita window. Views,
//! sidebar, headerbar and navigation are added starting Faz 3.

#![forbid(unsafe_code)]

use gtk4::glib;
use libadwaita::prelude::*;
use libadwaita::{Application, ApplicationWindow};

const WINDOW_TITLE: &str = "Veyra - Modern Linux File Manager";
const WINDOW_DEFAULT_WIDTH: i32 = 1024;
const WINDOW_DEFAULT_HEIGHT: i32 = 680;

/// Builds and runs the Veyra GTK application under the given D-Bus application ID.
///
/// Blocks the calling thread until the GTK main loop exits. Must be called on
/// the same thread the process started on (GTK main thread requirement).
pub fn run(app_id: &str) -> glib::ExitCode {
    let app = Application::builder().application_id(app_id).build();

    app.connect_activate(|app| {
        tracing::info!("activating primary window");
        build_window(app).present();
    });

    app.run()
}

fn build_window(app: &Application) -> ApplicationWindow {
    ApplicationWindow::builder()
        .application(app)
        .title(WINDOW_TITLE)
        .default_width(WINDOW_DEFAULT_WIDTH)
        .default_height(WINDOW_DEFAULT_HEIGHT)
        .build()
}
