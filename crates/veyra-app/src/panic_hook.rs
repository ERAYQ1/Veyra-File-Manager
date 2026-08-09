use std::panic;

/// Installs a panic hook that logs panics through `tracing` (so they land in
/// the structured log file, not just stderr) before falling back to Rust's
/// default panic report. A panic here means an unrecoverable programming
/// error, not a user-facing filesystem failure (Rule #18): those must be
/// handled with `Result`, never by panicking.
pub fn install() {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown location".to_string());
        let message = panic_message(info);

        tracing::error!(location = %location, message = %message, "unrecoverable panic");

        default_hook(info);
    }));
}

fn panic_message(info: &panic::PanicHookInfo<'_>) -> String {
    if let Some(s) = info.payload().downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = info.payload().downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_string()
    }
}
