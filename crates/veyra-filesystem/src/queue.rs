//! Bulk file operation engine: recursive Copy/Move with live progress and
//! conflict resolution, plus Trash/Delete — the async worker `veyra-ui`
//! drives from a background thread. Blocking, like the rest of `ops`; never
//! call `run_operation` on the GTK main thread (Rule #14).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use gio::prelude::*;

use crate::conflict::{Conflict, ConflictDecision};
use crate::error::{map_gio_error, FsError};
use crate::ops;
use crate::path::VeyraPath;
use crate::progress::Progress;

/// The four bulk operations Faz 5 supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationKind {
    Copy,
    Move,
    Trash,
    Delete,
}

/// A batch of sources to run `kind` against. `destination` is required for
/// `Copy`/`Move` (the target directory) and ignored for `Trash`/`Delete`.
#[derive(Debug, Clone)]
pub struct OperationRequest {
    pub kind: OperationKind,
    pub sources: Vec<VeyraPath>,
    pub destination: Option<VeyraPath>,
}

/// Cooperative cancel/pause/resume switch shared between the caller (UI
/// thread) and the worker thread actually running the operation. Cloning is
/// cheap (`Arc`'d atomics), so both sides hold an independent handle onto
/// the same underlying state.
#[derive(Clone, Default)]
pub struct OperationControl {
    cancelled: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
}

impl OperationControl {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Blocks the calling (worker) thread while paused. Returns `true` if
    /// the operation should stop now (cancelled either before or while
    /// waiting out a pause).
    fn wait_if_paused(&self) -> bool {
        while self.is_paused() {
            if self.is_cancelled() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        self.is_cancelled()
    }
}

/// What happened to each source after `run_operation` returns. For
/// Trash/Delete/Move-by-rename, entries are the top-level sources passed
/// in; for a recursively-expanded Copy/Move, `completed` and `errors` are
/// reported per file actually written, since that is the granularity the
/// progress bar itself works at.
#[derive(Debug, Default)]
pub struct OperationOutcome {
    pub completed: Vec<VeyraPath>,
    pub skipped: Vec<VeyraPath>,
    pub errors: Vec<(VeyraPath, FsError)>,
    pub cancelled: bool,
}

/// Runs `request` to completion (or cancellation/pause), invoking
/// `on_progress` after every file and `on_conflict` whenever a Copy/Move
/// destination already exists. Blocking — call from a background thread.
pub fn run_operation(
    request: OperationRequest,
    control: &OperationControl,
    mut on_progress: impl FnMut(Progress),
    mut on_conflict: impl FnMut(&Conflict) -> ConflictDecision,
) -> OperationOutcome {
    match request.kind {
        OperationKind::Trash | OperationKind::Delete => {
            run_simple(request, control, &mut on_progress)
        }
        OperationKind::Copy | OperationKind::Move => {
            run_copy_move(request, control, &mut on_progress, &mut on_conflict)
        }
    }
}

fn run_simple(
    request: OperationRequest,
    control: &OperationControl,
    on_progress: &mut impl FnMut(Progress),
) -> OperationOutcome {
    let mut outcome = OperationOutcome::default();
    let file_count = request.sources.len();

    for (index, source) in request.sources.into_iter().enumerate() {
        if control.wait_if_paused() {
            outcome.cancelled = true;
            return outcome;
        }

        on_progress(Progress {
            current_file: source.file_name().unwrap_or_default(),
            file_index: index,
            file_count,
            bytes_done: 0,
            bytes_total: 0,
        });

        let result = match request.kind {
            OperationKind::Trash => ops::trash(&source),
            OperationKind::Delete => ops::delete(&source),
            OperationKind::Copy | OperationKind::Move => unreachable!(),
        };

        match result {
            Ok(()) => outcome.completed.push(source),
            Err(err) => outcome.errors.push((source, err)),
        }
    }

    outcome
}

fn run_copy_move(
    request: OperationRequest,
    control: &OperationControl,
    on_progress: &mut impl FnMut(Progress),
    on_conflict: &mut impl FnMut(&Conflict) -> ConflictDecision,
) -> OperationOutcome {
    let mut outcome = OperationOutcome::default();

    let Some(dest_dir) = request.destination.clone() else {
        for source in request.sources {
            outcome.errors.push((
                source,
                FsError::InvalidPath("copy/move requires a destination directory".to_string()),
            ));
        }
        return outcome;
    };

    let mut plan: Vec<PlanItem> = Vec::new();
    let mut blanket: Option<ConflictDecision> = None;
    // Roots that took the recursive-copy fallback below (cross-device moves)
    // rather than the instant-rename fast path; their originals are deleted
    // once the copy phase finishes, but only if nothing in the whole batch
    // errored (conservative: an unrelated failure leaves every source
    // intact rather than risk losing data).
    let mut pending_move_cleanup: Vec<VeyraPath> = Vec::new();

    for source in &request.sources {
        if control.is_cancelled() {
            outcome.cancelled = true;
            return outcome;
        }

        let Some(name) = source.file_name() else {
            outcome.errors.push((
                source.clone(),
                FsError::InvalidPath(format!("cannot determine a file name for {source}")),
            ));
            continue;
        };

        let mut dest = join_path(&dest_dir, &name);
        let mut overwrite = false;

        if exists(&dest) {
            let decision = match &blanket {
                Some(ConflictDecision::ReplaceAll) => ConflictDecision::Replace,
                Some(ConflictDecision::SkipAll) => ConflictDecision::Skip,
                _ => on_conflict(&Conflict {
                    source: source.clone(),
                    destination: dest.clone(),
                }),
            };
            if decision.is_blanket() {
                blanket = Some(decision.clone());
            }
            match decision {
                ConflictDecision::Cancel => {
                    outcome.cancelled = true;
                    return outcome;
                }
                ConflictDecision::Skip | ConflictDecision::SkipAll => {
                    outcome.skipped.push(source.clone());
                    continue;
                }
                ConflictDecision::Replace | ConflictDecision::ReplaceAll => {
                    overwrite = true;
                }
                ConflictDecision::Rename(new_name) => {
                    dest = join_path(&dest_dir, &new_name);
                }
            }
        }

        // Same-filesystem moves are a plain rename: instant, and correct
        // for directories without walking them at all. Only fall back to
        // the recursive copy-then-delete plan below when that fails (e.g.
        // a cross-device move).
        if request.kind == OperationKind::Move && ops::move_entry(source, &dest, overwrite).is_ok()
        {
            outcome.completed.push(source.clone());
            continue;
        }

        if let Err(err) = flatten(source, &dest, &mut plan) {
            outcome.errors.push((source.clone(), err));
        } else if request.kind == OperationKind::Move {
            pending_move_cleanup.push(source.clone());
        }
    }

    let bytes_total: u64 = plan
        .iter()
        .map(|item| match item {
            PlanItem::File { size, .. } => *size,
            PlanItem::Dir { .. } => 0,
        })
        .sum();
    let file_count = plan
        .iter()
        .filter(|item| matches!(item, PlanItem::File { .. }))
        .count();

    let mut bytes_done = 0u64;
    let mut file_index = 0usize;

    for item in plan {
        if control.wait_if_paused() {
            outcome.cancelled = true;
            return outcome;
        }

        match item {
            PlanItem::Dir { dest } => {
                if let Err(err) = ops::create_dir(&dest) {
                    if !matches!(err, FsError::AlreadyExists(_)) {
                        outcome.errors.push((dest, err));
                    }
                }
            }
            PlanItem::File { source, dest, size } => {
                let base = bytes_done;
                let display_name = source.file_name().unwrap_or_default();
                let copy_result = {
                    let mut report = |current: i64, _total: i64| {
                        on_progress(Progress {
                            current_file: display_name.clone(),
                            file_index,
                            file_count,
                            bytes_done: base + current.max(0) as u64,
                            bytes_total,
                        });
                    };
                    source.to_gio_file().copy(
                        &dest.to_gio_file(),
                        gio::FileCopyFlags::OVERWRITE,
                        gio::Cancellable::NONE,
                        Some(&mut report),
                    )
                };

                file_index += 1;
                match copy_result {
                    Ok(()) => {
                        bytes_done = base + size;
                        outcome.completed.push(source);
                    }
                    Err(err) => outcome
                        .errors
                        .push((source.clone(), map_gio_error(&source, err))),
                }
            }
        }
    }

    if request.kind == OperationKind::Move && outcome.errors.is_empty() {
        for source in &pending_move_cleanup {
            let _ = ops::delete(source);
        }
    }

    outcome
}

/// One step of a flattened Copy/Move plan: either "ensure this destination
/// directory exists" or "copy this one source file to this destination".
/// Directories always precede the files planned underneath them.
enum PlanItem {
    Dir {
        dest: VeyraPath,
    },
    File {
        source: VeyraPath,
        dest: VeyraPath,
        size: u64,
    },
}

/// Recursively walks `source`, appending a `PlanItem` for it (and, if it's
/// a directory, for every descendant) targeting the mirrored path under
/// `dest`.
fn flatten(source: &VeyraPath, dest: &VeyraPath, out: &mut Vec<PlanItem>) -> Result<(), FsError> {
    let info = source
        .to_gio_file()
        .query_info(
            "standard::type,standard::size",
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            gio::Cancellable::NONE,
        )
        .map_err(|e| map_gio_error(source, e))?;

    if info.file_type() == gio::FileType::Directory {
        out.push(PlanItem::Dir { dest: dest.clone() });
        for child in ops::read_dir(source)? {
            let child_dest = join_path(dest, child.name());
            flatten(&child.path, &child_dest, out)?;
        }
    } else {
        out.push(PlanItem::File {
            source: source.clone(),
            dest: dest.clone(),
            size: info.size().max(0) as u64,
        });
    }

    Ok(())
}

fn join_path(base: &VeyraPath, name: &str) -> VeyraPath {
    match base {
        VeyraPath::Local(path) => VeyraPath::Local(path.join(name)),
        VeyraPath::Uri(uri) => VeyraPath::Uri(format!("{}/{name}", uri.trim_end_matches('/'))),
    }
}

fn exists(path: &VeyraPath) -> bool {
    path.to_gio_file().query_exists(gio::Cancellable::NONE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn local(p: impl Into<PathBuf>) -> VeyraPath {
        VeyraPath::from_local(p.into())
    }

    fn no_conflict(_: &Conflict) -> ConflictDecision {
        panic!("no conflict expected in this test");
    }

    #[test]
    fn copy_single_file_reports_completion_and_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.txt");
        fs::write(&src, b"hello world").unwrap();
        let dest_dir = dir.path().join("out");
        fs::create_dir(&dest_dir).unwrap();

        let request = OperationRequest {
            kind: OperationKind::Copy,
            sources: vec![local(&src)],
            destination: Some(local(&dest_dir)),
        };
        let control = OperationControl::new();
        let mut last_progress = None;
        let outcome = run_operation(request, &control, |p| last_progress = Some(p), no_conflict);

        assert_eq!(outcome.completed.len(), 1);
        assert!(outcome.errors.is_empty());
        assert!(dest_dir.join("a.txt").is_file());
        assert_eq!(fs::read(dest_dir.join("a.txt")).unwrap(), b"hello world");
        let last = last_progress.unwrap();
        assert_eq!(last.bytes_total, 11);
        assert_eq!(last.percent(), 100.0);
    }

    #[test]
    fn copy_directory_recursively_mirrors_tree() {
        let dir = tempfile::tempdir().unwrap();
        let src_root = dir.path().join("project");
        fs::create_dir_all(src_root.join("nested")).unwrap();
        fs::write(src_root.join("top.txt"), b"top").unwrap();
        fs::write(src_root.join("nested/inner.txt"), b"inner").unwrap();
        let dest_dir = dir.path().join("out");
        fs::create_dir(&dest_dir).unwrap();

        let request = OperationRequest {
            kind: OperationKind::Copy,
            sources: vec![local(&src_root)],
            destination: Some(local(&dest_dir)),
        };
        let control = OperationControl::new();
        let outcome = run_operation(request, &control, |_| {}, no_conflict);

        assert!(outcome.errors.is_empty());
        assert_eq!(outcome.completed.len(), 2);
        assert!(dest_dir.join("project/top.txt").is_file());
        assert!(dest_dir.join("project/nested/inner.txt").is_file());
        assert!(src_root.exists(), "copy must not remove the source");
    }

    #[test]
    fn move_removes_source_after_success() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.txt");
        fs::write(&src, b"payload").unwrap();
        let dest_dir = dir.path().join("out");
        fs::create_dir(&dest_dir).unwrap();

        let request = OperationRequest {
            kind: OperationKind::Move,
            sources: vec![local(&src)],
            destination: Some(local(&dest_dir)),
        };
        let control = OperationControl::new();
        let outcome = run_operation(request, &control, |_| {}, no_conflict);

        assert!(outcome.errors.is_empty());
        assert!(!src.exists());
        assert!(dest_dir.join("a.txt").is_file());
    }

    #[test]
    fn conflict_skip_leaves_destination_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.txt");
        fs::write(&src, b"new").unwrap();
        let dest_dir = dir.path().join("out");
        fs::create_dir(&dest_dir).unwrap();
        fs::write(dest_dir.join("a.txt"), b"old").unwrap();

        let request = OperationRequest {
            kind: OperationKind::Copy,
            sources: vec![local(&src)],
            destination: Some(local(&dest_dir)),
        };
        let control = OperationControl::new();
        let outcome = run_operation(request, &control, |_| {}, |_| ConflictDecision::Skip);

        assert_eq!(outcome.skipped.len(), 1);
        assert!(outcome.completed.is_empty());
        assert_eq!(fs::read(dest_dir.join("a.txt")).unwrap(), b"old");
    }

    #[test]
    fn conflict_replace_overwrites_destination() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.txt");
        fs::write(&src, b"new").unwrap();
        let dest_dir = dir.path().join("out");
        fs::create_dir(&dest_dir).unwrap();
        fs::write(dest_dir.join("a.txt"), b"old").unwrap();

        let request = OperationRequest {
            kind: OperationKind::Copy,
            sources: vec![local(&src)],
            destination: Some(local(&dest_dir)),
        };
        let control = OperationControl::new();
        let outcome = run_operation(request, &control, |_| {}, |_| ConflictDecision::Replace);

        assert_eq!(outcome.completed.len(), 1);
        assert_eq!(fs::read(dest_dir.join("a.txt")).unwrap(), b"new");
    }

    #[test]
    fn conflict_rename_writes_under_new_name() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.txt");
        fs::write(&src, b"new").unwrap();
        let dest_dir = dir.path().join("out");
        fs::create_dir(&dest_dir).unwrap();
        fs::write(dest_dir.join("a.txt"), b"old").unwrap();

        let request = OperationRequest {
            kind: OperationKind::Copy,
            sources: vec![local(&src)],
            destination: Some(local(&dest_dir)),
        };
        let control = OperationControl::new();
        let outcome = run_operation(
            request,
            &control,
            |_| {},
            |_| ConflictDecision::Rename("a (2).txt".to_string()),
        );

        assert_eq!(outcome.completed.len(), 1);
        assert_eq!(fs::read(dest_dir.join("a.txt")).unwrap(), b"old");
        assert_eq!(fs::read(dest_dir.join("a (2).txt")).unwrap(), b"new");
    }

    #[test]
    fn conflict_cancel_aborts_before_any_write() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.txt");
        fs::write(&src, b"new").unwrap();
        let dest_dir = dir.path().join("out");
        fs::create_dir(&dest_dir).unwrap();
        fs::write(dest_dir.join("a.txt"), b"old").unwrap();

        let request = OperationRequest {
            kind: OperationKind::Copy,
            sources: vec![local(&src)],
            destination: Some(local(&dest_dir)),
        };
        let control = OperationControl::new();
        let outcome = run_operation(request, &control, |_| {}, |_| ConflictDecision::Cancel);

        assert!(outcome.cancelled);
        assert!(outcome.completed.is_empty());
    }

    #[test]
    fn skip_all_applies_to_later_conflicts_without_asking() {
        let dir = tempfile::tempdir().unwrap();
        let dest_dir = dir.path().join("out");
        fs::create_dir(&dest_dir).unwrap();
        let mut sources = Vec::new();
        for name in ["a.txt", "b.txt"] {
            fs::write(dir.path().join(name), b"new").unwrap();
            fs::write(dest_dir.join(name), b"old").unwrap();
            sources.push(local(dir.path().join(name)));
        }

        let request = OperationRequest {
            kind: OperationKind::Copy,
            sources,
            destination: Some(local(&dest_dir)),
        };
        let control = OperationControl::new();
        let mut asked = 0;
        let outcome = run_operation(
            request,
            &control,
            |_| {},
            |_| {
                asked += 1;
                ConflictDecision::SkipAll
            },
        );

        assert_eq!(asked, 1, "SkipAll must not prompt again");
        assert_eq!(outcome.skipped.len(), 2);
    }

    #[test]
    fn cancel_stops_before_remaining_sources() {
        let dir = tempfile::tempdir().unwrap();
        let dest_dir = dir.path().join("out");
        fs::create_dir(&dest_dir).unwrap();
        let mut sources = Vec::new();
        for name in ["a.txt", "b.txt", "c.txt"] {
            fs::write(dir.path().join(name), b"payload").unwrap();
            sources.push(local(dir.path().join(name)));
        }

        let request = OperationRequest {
            kind: OperationKind::Copy,
            sources,
            destination: Some(local(&dest_dir)),
        };
        let control = OperationControl::new();
        let control_for_progress = control.clone();
        let outcome = run_operation(
            request,
            &control,
            move |_| control_for_progress.cancel(),
            no_conflict,
        );

        assert!(outcome.cancelled);
        assert!(outcome.completed.len() < 3);
    }

    #[test]
    fn pause_blocks_worker_until_resumed() {
        let dir = tempfile::tempdir().unwrap();
        let dest_dir = dir.path().join("out");
        fs::create_dir(&dest_dir).unwrap();
        let mut sources = Vec::new();
        for name in ["a.txt", "b.txt"] {
            fs::write(dir.path().join(name), b"payload").unwrap();
            sources.push(local(dir.path().join(name)));
        }

        let request = OperationRequest {
            kind: OperationKind::Copy,
            sources,
            destination: Some(local(&dest_dir)),
        };
        let control = OperationControl::new();
        control.pause();

        let worker_control = control.clone();
        let handle = std::thread::spawn(move || {
            run_operation(request, &worker_control, |_| {}, no_conflict)
        });

        std::thread::sleep(Duration::from_millis(100));
        assert!(
            dest_dir.read_dir().unwrap().next().is_none(),
            "paused operation must not have copied anything yet"
        );

        control.resume();
        let outcome = handle.join().unwrap();
        assert_eq!(outcome.completed.len(), 2);
    }

    #[test]
    fn trash_and_delete_report_per_file_progress_without_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.txt");
        fs::write(&src, b"payload").unwrap();

        let request = OperationRequest {
            kind: OperationKind::Delete,
            sources: vec![local(&src)],
            destination: None,
        };
        let control = OperationControl::new();
        let mut seen = None;
        let outcome = run_operation(request, &control, |p| seen = Some(p), no_conflict);

        assert_eq!(outcome.completed.len(), 1);
        assert!(!src.exists());
        assert_eq!(seen.unwrap().bytes_total, 0);
    }
}
