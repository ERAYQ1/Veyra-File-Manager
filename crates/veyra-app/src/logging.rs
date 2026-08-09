use std::fs::OpenOptions;
use std::path::Path;

use anyhow::Context;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

/// Initializes structured `tracing` logging: human-readable output on stdout
/// plus a plain (non-ANSI) copy appended to `log_file` under the XDG state
/// directory. Level is controlled by `RUST_LOG`, defaulting to `info`.
pub fn init(log_file: &Path) -> anyhow::Result<()> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let stdout_layer = fmt::layer().with_target(false).compact();

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file)
        .with_context(|| format!("failed to open log file at {}", log_file.display()))?;

    let file_layer = fmt::layer()
        .with_target(false)
        .with_ansi(false)
        .with_writer(file);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stdout_layer)
        .with(file_layer)
        .try_init()
        .context("failed to install tracing subscriber (already initialized?)")?;

    Ok(())
}
