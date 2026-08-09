//! Bridges blocking `veyra-filesystem` calls onto a background thread and
//! delivers their result back to the GTK main thread — the UI thread must
//! never perform synchronous I/O itself (Rule #14).
//!
//! `glib::MainContext::channel` (the API the roadmap names) was removed
//! upstream; `async-channel` + `glib::spawn_future_local` is the documented
//! gtk-rs replacement and is used here instead.

use gtk4::glib;

/// Runs `work` on a background thread, then calls `on_done` with its result
/// on the GTK main thread.
pub(crate) fn run_blocking<T, F, D>(work: F, on_done: D)
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
    D: FnOnce(T) + 'static,
{
    let (sender, receiver) = async_channel::bounded(1);

    std::thread::spawn(move || {
        let result = work();
        // The receiving end only goes away if the window itself is gone, in
        // which case there is nothing left to deliver to.
        let _ = sender.send_blocking(result);
    });

    glib::spawn_future_local(async move {
        if let Ok(result) = receiver.recv().await {
            on_done(result);
        }
    });
}
