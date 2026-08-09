//! Veyra application entry point: process lifecycle, logging bootstrap,
//! root-privilege guard and panic handling. UI construction lives in
//! `veyra-ui`; this binary only wires the pieces together.

mod logging;
mod panic_hook;
mod root_guard;

use std::process::ExitCode;

use veyra_core::XdgDirs;

const APP_ID: &str = "io.github.erayq1.Veyra";
const APP_NAME: &str = "veyra";

fn main() -> ExitCode {
    if root_guard::is_running_as_root() {
        eprintln!(
            "error: Veyra refuses to run as root. Re-run as a regular user; \
             privileged operations are handled via a dedicated helper."
        );
        return ExitCode::FAILURE;
    }

    let xdg_dirs = match XdgDirs::resolve(APP_NAME) {
        Ok(dirs) => dirs,
        Err(err) => {
            eprintln!("error: failed to resolve XDG directories: {err}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(err) = xdg_dirs.ensure_created() {
        eprintln!("error: failed to create XDG directories: {err}");
        return ExitCode::FAILURE;
    }

    if let Err(err) = logging::init(&xdg_dirs.log_file()) {
        eprintln!("error: failed to initialize logging: {err:#}");
        return ExitCode::FAILURE;
    }

    panic_hook::install();

    tracing::info!(
        app_id = APP_ID,
        config_dir = %xdg_dirs.config_dir.display(),
        cache_dir = %xdg_dirs.cache_dir.display(),
        state_dir = %xdg_dirs.state_dir.display(),
        "starting Veyra"
    );

    let exit_code = veyra_ui::run(APP_ID);

    tracing::info!(code = exit_code.value(), "Veyra shut down");

    if exit_code.value() == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
