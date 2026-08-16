//! Faz 33: Undo/Redo engine for Rename/Move/Copy/Trash/Create-folder/
//! Create-file operations (`Ctrl+Z` / `Ctrl+Shift+Z` / `Ctrl+Y`).
//!
//! Every user-facing mutation records an `UndoableAction` onto the shared
//! `UndoStack` at its own call site in `window.rs`, right after the
//! underlying operation succeeds. `win.undo`/`win.redo` pop from one stack,
//! run the inverse filesystem call here on a background thread
//! (`fs_async::run_blocking`, Rule #11/#14), and — on success — push the
//! action that was actually executed back onto the other stack so it can be
//! replayed. Permanently deleted paths (`win.delete-selection`) purge any
//! pending record that references them: Rule #39 says a permanent delete is
//! never itself undoable, and a stale record pointing at a now-gone path
//! must not linger to fail later.

use std::cell::RefCell;
use std::rc::Rc;

use veyra_filesystem::VeyraPath;

/// A window's undo/redo stacks, shared (via `Rc<RefCell<_>>`, the same
/// pattern every other piece of per-window mutable state in `window.rs`
/// uses) across every `win.*` action that can record or execute a step.
pub(crate) type SharedUndoStack = Rc<RefCell<UndoStack>>;

/// Depth cap for both stacks (Faz 33 spec): pushing a 51st entry evicts the
/// oldest one.
const MAX_DEPTH: usize = 50;

/// One reversible file operation, recorded at its own call site right after
/// the underlying filesystem call succeeded.
#[derive(Debug, Clone)]
pub(crate) enum UndoableAction {
    Rename {
        old_path: VeyraPath,
        new_path: VeyraPath,
    },
    /// Root-level `(source, destination)` pairs — one per top-level item the
    /// user acted on, not per descendant file of a moved directory.
    Move(Vec<(VeyraPath, VeyraPath)>),
    /// Root-level `(source, destination)` pairs. The spec names this
    /// `Vec<dst_path>`, but Redo has to re-copy from *somewhere* — the
    /// source is kept alongside each destination so Redo doesn't need to
    /// guess it back out of a path that may no longer exist.
    Copy(Vec<(VeyraPath, VeyraPath)>),
    /// `(original_path, trashed_file_path)` pairs.
    Trash(Vec<(VeyraPath, VeyraPath)>),
    /// `win.restore-selected`'s own record — the mirror image of `Trash`:
    /// restoring is itself a forward, undoable action (undo re-trashes it,
    /// redo restores it again), not merely the execution of some other
    /// action's undo step. `(restored_path, trashed_source_path)` pairs.
    Restore(Vec<(VeyraPath, VeyraPath)>),
    CreateFolder(VeyraPath),
    CreateFile(VeyraPath),
}

impl UndoableAction {
    fn referenced_paths(&self) -> Vec<&VeyraPath> {
        match self {
            UndoableAction::Rename { old_path, new_path } => vec![old_path, new_path],
            UndoableAction::Move(pairs)
            | UndoableAction::Copy(pairs)
            | UndoableAction::Trash(pairs)
            | UndoableAction::Restore(pairs) => pairs.iter().flat_map(|(a, b)| [a, b]).collect(),
            UndoableAction::CreateFolder(p) | UndoableAction::CreateFile(p) => vec![p],
        }
    }

    /// Whether this action references any of `deleted` (or a path
    /// underneath one) — used to purge stale records once a permanent
    /// delete removes something a pending Undo/Redo entry still points at.
    fn references_any(&self, deleted: &[VeyraPath]) -> bool {
        self.referenced_paths()
            .into_iter()
            .any(|p| deleted.iter().any(|d| is_same_or_descendant(p, d)))
    }

    fn undo_label(&self) -> String {
        match self {
            UndoableAction::Rename { old_path, new_path } => format!(
                "Undid: Renamed '{}' back to '{}'",
                display_name(new_path),
                display_name(old_path)
            ),
            UndoableAction::Move(pairs) => format!(
                "Undid: Moved {} back to '{}'",
                item_count(pairs.len()),
                parent_name(&pairs[0].0).unwrap_or_default()
            ),
            UndoableAction::Copy(pairs) => {
                format!("Undid: Removed {} copied", item_count(pairs.len()))
            }
            UndoableAction::Trash(pairs) => {
                format!("Undid: Restored {} from Trash", item_count(pairs.len()))
            }
            UndoableAction::Restore(pairs) => {
                format!("Undid: Moved {} back to Trash", item_count(pairs.len()))
            }
            UndoableAction::CreateFolder(p) => {
                format!("Undid: Removed folder '{}'", display_name(p))
            }
            UndoableAction::CreateFile(p) => format!("Undid: Removed file '{}'", display_name(p)),
        }
    }

    fn redo_label(&self) -> String {
        match self {
            UndoableAction::Rename { old_path, new_path } => format!(
                "Redid: Renamed '{}' to '{}'",
                display_name(old_path),
                display_name(new_path)
            ),
            UndoableAction::Move(pairs) => format!(
                "Redid: Moved {} to '{}'",
                item_count(pairs.len()),
                parent_name(&pairs[0].1).unwrap_or_default()
            ),
            UndoableAction::Copy(pairs) => format!("Redid: Copied {}", item_count(pairs.len())),
            UndoableAction::Trash(pairs) => {
                format!("Redid: Moved {} to Trash", item_count(pairs.len()))
            }
            UndoableAction::Restore(pairs) => {
                format!("Redid: Restored {} from Trash", item_count(pairs.len()))
            }
            UndoableAction::CreateFolder(p) => {
                format!("Redid: Created folder '{}'", display_name(p))
            }
            UndoableAction::CreateFile(p) => format!("Redid: Created file '{}'", display_name(p)),
        }
    }
}

fn display_name(path: &VeyraPath) -> String {
    path.file_name().unwrap_or_else(|| path.to_string())
}

/// The last path component of `path`'s *parent* — the folder an undone Move
/// lands back in, or a redone Move lands in, for toast text like "...back to
/// 'Projects'".
fn parent_name(path: &VeyraPath) -> Option<String> {
    match path {
        VeyraPath::Local(p) => p
            .parent()
            .and_then(|parent| parent.file_name())
            .map(|n| n.to_string_lossy().into_owned()),
        VeyraPath::Uri(uri) => {
            let trimmed = uri.trim_end_matches('/');
            let (parent, _) = trimmed.rsplit_once('/')?;
            parent
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .map(str::to_string)
        }
    }
}

fn item_count(n: usize) -> String {
    if n == 1 {
        "1 item".to_string()
    } else {
        format!("{n} items")
    }
}

/// Whether `path` is `root` itself or lives underneath it — a permanent
/// delete of a directory must purge undo/redo records for anything that was
/// inside it, not just an exact-path match.
fn is_same_or_descendant(path: &VeyraPath, root: &VeyraPath) -> bool {
    match (path, root) {
        (VeyraPath::Local(p), VeyraPath::Local(r)) => p == r || p.starts_with(r),
        (VeyraPath::Uri(p), VeyraPath::Uri(r)) => {
            let r = r.trim_end_matches('/');
            p == r || p.strip_prefix(r).is_some_and(|rest| rest.starts_with('/'))
        }
        _ => false,
    }
}

/// The two LIFO stacks a window keeps: what can be undone, and what can be
/// redone. A fresh user action clears `redo` (it invalidates whatever was
/// previously redoable) — `record` is only ever called for that case; the
/// engine's own undo/redo execution pushes onto the *opposite* stack via
/// `record_undo`/`record_redo` without touching the other one.
#[derive(Default)]
pub(crate) struct UndoStack {
    undo: Vec<UndoableAction>,
    redo: Vec<UndoableAction>,
}

impl UndoStack {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Records a freshly completed user action.
    pub(crate) fn record(&mut self, action: UndoableAction) {
        self.redo.clear();
        self.push_capped(action);
    }

    // `win.undo`/`win.redo` follow the same always-enabled, no-op-if-empty
    // convention every other selection-scoped `win.*` action in `window.rs`
    // already uses (see `action_copy`/`action_rename`: no `set_enabled`
    // gating on selection state), so these aren't called from production
    // code — kept for tests, same as `ShortcutMap::accels`.
    #[cfg(test)]
    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub(crate) fn take_undo(&mut self) -> Option<UndoableAction> {
        self.undo.pop()
    }

    pub(crate) fn take_redo(&mut self) -> Option<UndoableAction> {
        self.redo.pop()
    }

    /// Pushes the action `win.undo` just executed back onto the redo stack.
    pub(crate) fn record_redo(&mut self, action: UndoableAction) {
        self.redo.push(action);
    }

    /// Pushes the action `win.redo` just executed back onto the undo stack.
    pub(crate) fn record_undo(&mut self, action: UndoableAction) {
        self.push_capped(action);
    }

    fn push_capped(&mut self, action: UndoableAction) {
        self.undo.push(action);
        if self.undo.len() > MAX_DEPTH {
            self.undo.remove(0);
        }
    }

    /// Rule #39: drops every pending record in both stacks that references
    /// any of `deleted_paths` (or a path underneath one) — called right
    /// after a permanent delete completes.
    pub(crate) fn purge(&mut self, deleted_paths: &[VeyraPath]) {
        self.undo.retain(|a| !a.references_any(deleted_paths));
        self.redo.retain(|a| !a.references_any(deleted_paths));
    }
}

/// What executing an undo/redo step produced: a status message for the
/// caller to display, the action to push onto the *opposite* stack (`None`
/// if nothing at all succeeded), and how many of a multi-item action's
/// entries failed (a missing/moved-since path never panics — Rule #15/#16 —
/// it's just dropped from the resulting action and counted here).
pub(crate) struct ExecutionResult {
    pub message: String,
    pub resulting_action: Option<UndoableAction>,
    pub error_count: usize,
}

fn exists(path: &VeyraPath) -> bool {
    use gtk4::prelude::FileExt;
    path.to_gio_file()
        .query_exists(gtk4::gio::Cancellable::NONE)
}

fn missing_message(kind: &str) -> ExecutionResult {
    ExecutionResult {
        message: format!("Can't undo/redo: {kind} no longer exists"),
        resulting_action: None,
        error_count: 1,
    }
}

/// Runs the inverse of `action` on the calling thread — a background thread
/// only, never the GTK main loop (Rule #11/#14); `window.rs` wraps this in
/// `fs_async::run_blocking`.
pub(crate) fn perform_undo(action: UndoableAction) -> ExecutionResult {
    match action {
        UndoableAction::Rename { old_path, new_path } => {
            if !exists(&new_path) {
                return missing_message("the renamed item");
            }
            let Some(old_name) = old_path.file_name() else {
                return missing_message("the renamed item");
            };
            match veyra_filesystem::rename(&new_path, &old_name) {
                Ok(_) => {
                    let resulting = UndoableAction::Rename { old_path, new_path };
                    ExecutionResult {
                        message: resulting.undo_label(),
                        resulting_action: Some(resulting),
                        error_count: 0,
                    }
                }
                Err(err) => fail(format!("Undo failed: {err}")),
            }
        }
        UndoableAction::Move(pairs) => {
            let mut succeeded = Vec::new();
            let mut errors = 0;
            for (src, dst) in pairs {
                if exists(&dst) && veyra_filesystem::move_entry(&dst, &src, false).is_ok() {
                    succeeded.push((src, dst));
                } else {
                    errors += 1;
                }
            }
            finish_batch(succeeded, errors, UndoableAction::Move, |a| a.undo_label())
        }
        UndoableAction::Copy(pairs) => {
            let mut succeeded = Vec::new();
            let mut errors = 0;
            for (src, dst) in pairs {
                if exists(&dst) && veyra_filesystem::trash(&dst).is_ok() {
                    succeeded.push((src, dst));
                } else {
                    errors += 1;
                }
            }
            finish_batch(succeeded, errors, UndoableAction::Copy, |a| a.undo_label())
        }
        UndoableAction::Trash(pairs) => {
            let mut succeeded = Vec::new();
            let mut errors = 0;
            for (original, trashed) in pairs {
                if !exists(&trashed) {
                    errors += 1;
                    continue;
                }
                match veyra_filesystem::restore_from_trash(&trashed) {
                    Ok(restored) => succeeded.push((original, restored)),
                    Err(_) => errors += 1,
                }
            }
            finish_batch(succeeded, errors, UndoableAction::Trash, |a| a.undo_label())
        }
        UndoableAction::Restore(pairs) => {
            // Undo of a Restore is a re-trash: `Restore`'s mirror of
            // `Trash`'s own undo (which restores).
            let mut succeeded = Vec::new();
            let mut errors = 0;
            for (restored_path, _previous_trashed) in pairs {
                if !exists(&restored_path) {
                    errors += 1;
                    continue;
                }
                match veyra_filesystem::trash_tracked(&restored_path) {
                    Ok(trashed) => succeeded.push((restored_path, trashed)),
                    Err(_) => errors += 1,
                }
            }
            finish_batch(succeeded, errors, UndoableAction::Restore, |a| {
                a.undo_label()
            })
        }
        UndoableAction::CreateFolder(path) => {
            if !exists(&path) {
                return missing_message("the created folder");
            }
            match veyra_filesystem::trash(&path) {
                Ok(()) => {
                    let resulting = UndoableAction::CreateFolder(path);
                    ExecutionResult {
                        message: resulting.undo_label(),
                        resulting_action: Some(resulting),
                        error_count: 0,
                    }
                }
                Err(err) => fail(format!("Undo failed: {err}")),
            }
        }
        UndoableAction::CreateFile(path) => {
            if !exists(&path) {
                return missing_message("the created file");
            }
            match veyra_filesystem::trash(&path) {
                Ok(()) => {
                    let resulting = UndoableAction::CreateFile(path);
                    ExecutionResult {
                        message: resulting.undo_label(),
                        resulting_action: Some(resulting),
                        error_count: 0,
                    }
                }
                Err(err) => fail(format!("Undo failed: {err}")),
            }
        }
    }
}

/// Runs `action` forward again — the mirror of `perform_undo`, called by
/// `win.redo`.
pub(crate) fn perform_redo(action: UndoableAction) -> ExecutionResult {
    match action {
        UndoableAction::Rename { old_path, new_path } => {
            if !exists(&old_path) {
                return missing_message("the item");
            }
            let Some(new_name) = new_path.file_name() else {
                return missing_message("the item");
            };
            match veyra_filesystem::rename(&old_path, &new_name) {
                Ok(_) => {
                    let resulting = UndoableAction::Rename { old_path, new_path };
                    ExecutionResult {
                        message: resulting.redo_label(),
                        resulting_action: Some(resulting),
                        error_count: 0,
                    }
                }
                Err(err) => fail(format!("Redo failed: {err}")),
            }
        }
        UndoableAction::Move(pairs) => {
            let mut succeeded = Vec::new();
            let mut errors = 0;
            for (src, dst) in pairs {
                if exists(&src) && veyra_filesystem::move_entry(&src, &dst, false).is_ok() {
                    succeeded.push((src, dst));
                } else {
                    errors += 1;
                }
            }
            finish_batch(succeeded, errors, UndoableAction::Move, |a| a.redo_label())
        }
        UndoableAction::Copy(pairs) => {
            let mut succeeded = Vec::new();
            let mut errors = 0;
            for (src, dst) in pairs {
                if exists(&src) && veyra_filesystem::copy(&src, &dst, false).is_ok() {
                    succeeded.push((src, dst));
                } else {
                    errors += 1;
                }
            }
            finish_batch(succeeded, errors, UndoableAction::Copy, |a| a.redo_label())
        }
        UndoableAction::Trash(pairs) => {
            let mut succeeded = Vec::new();
            let mut errors = 0;
            for (original, _previous_trashed) in pairs {
                if !exists(&original) {
                    errors += 1;
                    continue;
                }
                match veyra_filesystem::trash_tracked(&original) {
                    Ok(trashed) => succeeded.push((original, trashed)),
                    Err(_) => errors += 1,
                }
            }
            finish_batch(succeeded, errors, UndoableAction::Trash, |a| a.redo_label())
        }
        UndoableAction::Restore(pairs) => {
            // Redo of a Restore restores it again — `Restore`'s mirror of
            // `Trash`'s own redo (which re-trashes).
            let mut succeeded = Vec::new();
            let mut errors = 0;
            for (_previous_restored, trashed_source) in pairs {
                if !exists(&trashed_source) {
                    errors += 1;
                    continue;
                }
                match veyra_filesystem::restore_from_trash(&trashed_source) {
                    Ok(restored) => succeeded.push((restored, trashed_source)),
                    Err(_) => errors += 1,
                }
            }
            finish_batch(succeeded, errors, UndoableAction::Restore, |a| {
                a.redo_label()
            })
        }
        UndoableAction::CreateFolder(path) => {
            if exists(&path) {
                return missing_message("a same-named item");
            }
            match veyra_filesystem::create_dir(&path) {
                Ok(()) => {
                    let resulting = UndoableAction::CreateFolder(path);
                    ExecutionResult {
                        message: resulting.redo_label(),
                        resulting_action: Some(resulting),
                        error_count: 0,
                    }
                }
                Err(err) => fail(format!("Redo failed: {err}")),
            }
        }
        UndoableAction::CreateFile(path) => {
            if exists(&path) {
                return missing_message("a same-named item");
            }
            match veyra_filesystem::create_file(&path) {
                Ok(()) => {
                    let resulting = UndoableAction::CreateFile(path);
                    ExecutionResult {
                        message: resulting.redo_label(),
                        resulting_action: Some(resulting),
                        error_count: 0,
                    }
                }
                Err(err) => fail(format!("Redo failed: {err}")),
            }
        }
    }
}

fn fail(message: String) -> ExecutionResult {
    ExecutionResult {
        message,
        resulting_action: None,
        error_count: 1,
    }
}

/// Shared tail for the Move/Copy/Trash batch arms above: wraps whatever
/// pairs actually succeeded back into an `UndoableAction` for the opposite
/// stack, or reports a plain failure if none did.
fn finish_batch(
    succeeded: Vec<(VeyraPath, VeyraPath)>,
    errors: usize,
    build: impl FnOnce(Vec<(VeyraPath, VeyraPath)>) -> UndoableAction,
    label: impl FnOnce(&UndoableAction) -> String,
) -> ExecutionResult {
    if succeeded.is_empty() {
        return ExecutionResult {
            message: "Can't undo/redo: item(s) no longer available".to_string(),
            resulting_action: None,
            error_count: errors,
        };
    }
    let resulting = build(succeeded);
    ExecutionResult {
        message: label(&resulting),
        resulting_action: Some(resulting),
        error_count: errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn local(p: impl Into<std::path::PathBuf>) -> VeyraPath {
        VeyraPath::from_local(p.into())
    }

    #[test]
    fn record_clears_redo_stack() {
        let mut stack = UndoStack::new();
        stack.record(UndoableAction::CreateFile(local("/tmp/a.txt")));
        stack.record_redo(UndoableAction::CreateFile(local("/tmp/b.txt")));
        assert!(stack.can_redo());

        stack.record(UndoableAction::CreateFile(local("/tmp/c.txt")));
        assert!(!stack.can_redo());
        assert!(stack.can_undo());
    }

    #[test]
    fn stack_depth_is_capped_at_50() {
        let mut stack = UndoStack::new();
        for i in 0..60 {
            stack.record(UndoableAction::CreateFile(local(format!("/tmp/{i}.txt"))));
        }
        let mut popped = 0;
        while stack.take_undo().is_some() {
            popped += 1;
        }
        assert_eq!(popped, MAX_DEPTH);
    }

    #[test]
    fn undo_then_redo_round_trips_through_opposite_stacks() {
        let mut stack = UndoStack::new();
        stack.record(UndoableAction::CreateFile(local("/tmp/a.txt")));
        let action = stack.take_undo().unwrap();
        assert!(!stack.can_undo());

        stack.record_redo(action);
        assert!(stack.can_redo());

        let action = stack.take_redo().unwrap();
        stack.record_undo(action);
        assert!(stack.can_undo());
        assert!(!stack.can_redo());
    }

    #[test]
    fn purge_drops_records_referencing_a_deleted_path() {
        let mut stack = UndoStack::new();
        stack.record(UndoableAction::Rename {
            old_path: local("/tmp/report.pdf"),
            new_path: local("/tmp/report_v2.pdf"),
        });
        stack.record(UndoableAction::CreateFolder(local("/tmp/unrelated")));

        stack.purge(&[local("/tmp/report_v2.pdf")]);

        assert!(stack.can_undo());
        let remaining = stack.take_undo().unwrap();
        assert!(matches!(remaining, UndoableAction::CreateFolder(_)));
        assert!(!stack.can_undo());
    }

    #[test]
    fn purge_drops_records_for_paths_under_a_deleted_directory() {
        let mut stack = UndoStack::new();
        stack.record(UndoableAction::CreateFile(local("/tmp/project/notes.txt")));
        stack.purge(&[local("/tmp/project")]);
        assert!(!stack.can_undo());
    }

    #[test]
    fn undo_rename_moves_the_name_back() {
        let dir = tempfile::tempdir().unwrap();
        let old_path = local(dir.path().join("report.pdf"));
        let new_path = local(dir.path().join("report_v2.pdf"));
        fs::write(dir.path().join("report_v2.pdf"), b"x").unwrap();

        let result = perform_undo(UndoableAction::Rename {
            old_path: old_path.clone(),
            new_path,
        });
        assert_eq!(result.error_count, 0);
        assert!(dir.path().join("report.pdf").exists());
        assert!(matches!(
            result.resulting_action,
            Some(UndoableAction::Rename { .. })
        ));
    }

    #[test]
    fn undo_rename_reports_missing_target_without_panicking() {
        let dir = tempfile::tempdir().unwrap();
        let old_path = local(dir.path().join("report.pdf"));
        let new_path = local(dir.path().join("gone.pdf"));

        let result = perform_undo(UndoableAction::Rename { old_path, new_path });
        assert_eq!(result.error_count, 1);
        assert!(result.resulting_action.is_none());
    }

    #[test]
    fn undo_create_folder_trashes_it_then_redo_recreates_it() {
        let home_dir = tempfile::Builder::new()
            .prefix("veyra-undo-test-")
            .tempdir_in(std::env::var("HOME").expect("HOME must be set"))
            .unwrap();
        let folder = local(home_dir.path().join("New Folder"));
        veyra_filesystem::create_dir(&folder).unwrap();
        assert!(exists(&folder));

        let undo_result = perform_undo(UndoableAction::CreateFolder(folder.clone()));
        assert_eq!(undo_result.error_count, 0);
        assert!(!exists(&folder));

        let redo_result = perform_redo(UndoableAction::CreateFolder(folder.clone()));
        assert_eq!(redo_result.error_count, 0);
        assert!(exists(&folder));

        veyra_filesystem::delete(&folder).unwrap();
    }
}
