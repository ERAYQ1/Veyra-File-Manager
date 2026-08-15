//! Bridges `veyra_filesystem::create_archive`/`extract_archive` onto a
//! background thread and streams progress/completion back to the GTK main
//! thread — the same shape as `operations.rs`'s bridge, minus the
//! conflict round trip (archive entries are never prompted for, per
//! `extract.rs`'s Zip Slip/symlink handling).

use veyra_filesystem::{
    ArchiveFormat, ArchiveOutcome, FsError, OperationControl, Progress, VeyraPath,
};

/// One event emitted while a compress/extract run executes, delivered on
/// the GTK main thread as the caller drives the returned receiver with
/// `glib::spawn_future_local`.
pub(crate) enum ArchiveEvent {
    Progress(Progress),
    Done(Result<ArchiveOutcome, FsError>),
}

/// Starts packing `sources` into `output_path` as `format` on a background
/// thread. Returns the shared control handle (Pause/Resume/Cancel from the
/// progress UI) and the event stream the caller reads until `Done`.
pub(crate) fn spawn_compress(
    output_path: VeyraPath,
    sources: Vec<VeyraPath>,
    format: ArchiveFormat,
) -> (OperationControl, async_channel::Receiver<ArchiveEvent>) {
    let control = OperationControl::new();
    let worker_control = control.clone();
    let (tx, rx) = async_channel::unbounded();

    std::thread::spawn(move || {
        let progress_tx = tx.clone();
        let result = veyra_filesystem::create_archive(
            &output_path,
            &sources,
            format,
            &worker_control,
            move |progress| {
                let _ = progress_tx.send_blocking(ArchiveEvent::Progress(progress));
            },
        );
        let _ = tx.send_blocking(ArchiveEvent::Done(result));
    });

    (control, rx)
}

/// Starts extracting `archive_path` into `destination_dir` on a background
/// thread, mirroring `spawn_compress`.
pub(crate) fn spawn_extract(
    archive_path: VeyraPath,
    destination_dir: VeyraPath,
) -> (OperationControl, async_channel::Receiver<ArchiveEvent>) {
    let control = OperationControl::new();
    let worker_control = control.clone();
    let (tx, rx) = async_channel::unbounded();

    std::thread::spawn(move || {
        let progress_tx = tx.clone();
        let result = veyra_filesystem::extract_archive(
            &archive_path,
            &destination_dir,
            &worker_control,
            move |progress| {
                let _ = progress_tx.send_blocking(ArchiveEvent::Progress(progress));
            },
        );
        let _ = tx.send_blocking(ArchiveEvent::Done(result));
    });

    (control, rx)
}
