//! Veyra GTK4 + Libadwaita user interface layer.
//!
//! Faz 3 provides a usable file manager shell: sidebar (Places/Devices),
//! header bar (navigation, breadcrumbs, search, view switcher), status bar,
//! and three item views (Icon, Compact, Details) backed by an
//! `AdwNavigationSplitView`. All filesystem I/O runs off the GTK main
//! thread (`fs_async`), per Rule #14.

#![forbid(unsafe_code)]

mod archive_ops;
mod bookmarks;
mod breadcrumbs;
mod command_palette;
mod config;
mod context_menu;
mod devices;
mod dialogs;
mod dnd;
mod file_associations;
mod fs_async;
mod headerbar;
mod history;
mod network;
mod open_with;
mod operations;
mod preview;
mod privileged;
mod recent;
mod search_results;
mod session;
mod shortcuts;
mod sidebar;
mod sorting;
mod split_view;
mod state;
mod statusbar;
mod tab_page;
mod terminal;
mod thumbnails;
mod trash;
mod undo;
mod views;
mod widgets;
mod window;

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use gtk4::glib;
use libadwaita::prelude::*;
use libadwaita::Application;

use veyra_filesystem::VeyraPath;

/// Builds and runs the Veyra GTK application under the given D-Bus
/// application ID, starting in `start_dir` (the user's home directory when
/// `None` — the default first-launch location, and what "Open in New
/// Window" passes explicitly to open at the source item's directory).
/// `cache_dir` is where the Faz 9 search index database lives
/// (`<cache_dir>/search_index.db`).
///
/// Blocks the calling thread until the GTK main loop exits. Must be called on
/// the same thread the process started on (GTK main thread requirement).
pub fn run(app_id: &str, start_dir: Option<VeyraPath>, cache_dir: &Path) -> glib::ExitCode {
    let app = Application::builder().application_id(app_id).build();

    let default_icon_name = app_id.to_string();
    let cache_dir = cache_dir.to_path_buf();
    app.connect_activate(move |app| {
        tracing::info!("activating primary window");

        // Faz 34: `VeyraSettings::load()` is the single source of truth for
        // every user preference from here on — the theme applies before the
        // window is even built, and the loaded value (rather than a
        // hardcoded `ColorScheme::Default`) is what a corrupt/missing
        // `settings.json` falls back to via `VeyraSettings::default()`.
        let settings = config::VeyraSettings::load();
        veyra_core::security::set_sanitize_log_paths(settings.sanitize_log_paths);
        libadwaita::StyleManager::default().set_color_scheme(settings.color_scheme.to_adw());
        config::apply_accent_color(settings.accent_color);
        let settings: config::SharedSettings = Rc::new(RefCell::new(settings));

        if let Some(display) = gtk4::gdk::Display::default() {
            let icon_theme = gtk4::IconTheme::for_display(&display);
            icon_theme.add_search_path("data/icons");
        }
        gtk4::Window::set_default_icon_name(&default_icon_name);

        let start_dir = start_dir
            .clone()
            .unwrap_or_else(|| VeyraPath::from_local(glib::home_dir()));
        window::build_window(app, start_dir, &cache_dir, settings).present();
    });

    // `start_dir` is already resolved above from our own arg parsing
    // (`main.rs`); passing the real `argv` back into `GApplication` here
    // would make it apply its own default "open files" command-line
    // handling to that same path argument (which this app doesn't opt
    // into via `HANDLES_OPEN`/`HANDLES_COMMAND_LINE`) instead of a plain
    // activation.
    app.run_with_args::<&str>(&[])
}
