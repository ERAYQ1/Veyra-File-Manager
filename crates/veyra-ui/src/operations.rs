//! Bridges `veyra_filesystem::run_operation` (a blocking, multi-file Copy/
//! Move/Trash/Delete run) onto a background thread and streams its
//! progress/conflict/completion events back to the GTK main thread —
//! `fs_async`'s single request/response model doesn't fit an operation that
//! reports many times and sometimes needs an answer back, so this is its
//! own small bridge instead of an extension of it.

use veyra_filesystem::{Conflict, ConflictDecision, OperationControl, OperationRequest, Progress};

/// One event emitted while an operation runs, delivered on the GTK main
/// thread as the caller drives the returned receiver with
/// `glib::spawn_future_local`.
pub(crate) enum OperationEvent {
    Progress(Progress),
    /// A destination collision. The worker thread (a real background
    /// `std::thread`, not the GTK main loop) blocks on `answer_rx` until
    /// the UI sends a decision — safe to block since it never touches the
    /// main thread, and it's exactly the synchronous pause a modal
    /// conflict dialog needs.
    Conflict(Conflict, async_channel::Sender<ConflictDecision>),
    Done(veyra_filesystem::OperationOutcome),
}

/// Starts `request` on a background thread. Returns the shared control
/// handle (for pause/resume/cancel from the progress UI) and the event
/// stream the caller reads until it yields `Done`.
pub(crate) fn spawn(
    request: OperationRequest,
) -> (OperationControl, async_channel::Receiver<OperationEvent>) {
    let control = OperationControl::new();
    let worker_control = control.clone();
    let (tx, rx) = async_channel::unbounded();

    std::thread::spawn(move || {
        let progress_tx = tx.clone();
        let conflict_tx = tx.clone();
        let outcome = veyra_filesystem::run_operation(
            request,
            &worker_control,
            move |progress| {
                let _ = progress_tx.send_blocking(OperationEvent::Progress(progress));
            },
            move |conflict| {
                let (answer_tx, answer_rx) = async_channel::bounded(1);
                if conflict_tx
                    .send_blocking(OperationEvent::Conflict(conflict.clone(), answer_tx))
                    .is_err()
                {
                    return ConflictDecision::Cancel;
                }
                answer_rx
                    .recv_blocking()
                    .unwrap_or(ConflictDecision::Cancel)
            },
        );
        let _ = tx.send_blocking(OperationEvent::Done(outcome));
    });

    (control, rx)
}
