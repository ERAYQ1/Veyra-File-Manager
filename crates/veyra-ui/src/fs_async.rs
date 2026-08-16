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

/// Like `run_blocking`, but for work that produces a stream of intermediate
/// values before its final result. `work` runs on a background thread and is
/// handed an `emit` callback it can invoke any number of times; each
/// invocation is delivered to `on_chunk` on the GTK main thread as soon as it
/// arrives — not buffered until the whole operation finishes — so a huge
/// directory listing can paint its first batch immediately (Rule #30)
/// instead of waiting for the entire scan. `on_done` runs once, after the
/// last chunk, with `work`'s return value.
pub(crate) fn run_streaming<T, R, F, C, D>(work: F, mut on_chunk: C, on_done: D)
where
    T: Send + 'static,
    R: Send + 'static,
    F: FnOnce(&mut dyn FnMut(T)) -> R + Send + 'static,
    C: FnMut(T) + 'static,
    D: FnOnce(R) + 'static,
{
    enum Msg<T, R> {
        Chunk(T),
        Done(R),
    }

    let (sender, receiver) = async_channel::unbounded();

    std::thread::spawn(move || {
        let chunk_sender = sender.clone();
        let mut emit = move |chunk: T| {
            let _ = chunk_sender.send_blocking(Msg::Chunk(chunk));
        };
        let result = work(&mut emit);
        let _ = sender.send_blocking(Msg::Done(result));
    });

    glib::spawn_future_local(async move {
        while let Ok(msg) = receiver.recv().await {
            match msg {
                Msg::Chunk(chunk) => on_chunk(chunk),
                Msg::Done(result) => {
                    on_done(result);
                    break;
                }
            }
        }
    });
}
