//! Faz 51: turns a raw `FsError` into an `ActionableError` — a headline, a
//! human-language reason, the raw technical detail for a bug report, and
//! the set of recovery actions that actually make sense for this failure.
//! No operation should ever surface a bare `err.to_string()` to the user
//! (Rule #15/#18); `dialogs::error_dialog` is the only thing that renders
//! an `ActionableError`, and this module is the only thing that builds one.

use veyra_filesystem::{FsError, VeyraPath};

use crate::i18n::t;

/// One button `error_dialog` can offer. Order here is also render order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoveryAction {
    TryAgain,
    ChooseAnotherLocation,
    Skip,
    Cancel,
}

impl RecoveryAction {
    /// The `AdwAlertDialog` response id used for this action.
    pub(crate) fn response_id(self) -> &'static str {
        match self {
            RecoveryAction::TryAgain => "try-again",
            RecoveryAction::ChooseAnotherLocation => "choose-location",
            RecoveryAction::Skip => "skip",
            RecoveryAction::Cancel => "cancel",
        }
    }

    /// The localized button label.
    pub(crate) fn label(self) -> String {
        match self {
            RecoveryAction::TryAgain => t("error.action.try_again"),
            RecoveryAction::ChooseAnotherLocation => t("error.action.choose_location"),
            RecoveryAction::Skip => t("error.action.skip"),
            RecoveryAction::Cancel => t("error.action.cancel"),
        }
        .to_string()
    }
}

/// A fully human-readable, actionable description of one failed file
/// operation, ready for `dialogs::error_dialog::show`.
pub(crate) struct ActionableError {
    /// Operation-specific, e.g. "Couldn't move the file".
    pub headline: String,
    /// The affected file/folder name, shown highlighted under the headline.
    pub target_name: String,
    /// Plain-language cause, e.g. "The destination is read-only or you
    /// don't have write permissions."
    pub human_reason: String,
    /// Raw OS/GIO error for the collapsible "Show Details" section and the
    /// "Copy Technical Details" button. Never translated — it's meant to be
    /// pasted verbatim into a bug report.
    pub technical_details: String,
    /// Which buttons to offer, in display order.
    pub recovery_actions: Vec<RecoveryAction>,
}

/// What the caller can offer as recovery, and how to phrase the headline —
/// callers own the exact wording since it varies per operation/language,
/// `classify` only decides which buttons make sense for the error kind.
pub(crate) struct ErrorContext {
    pub headline: String,
    /// Copy/Move only: retrying into a different destination folder can fix
    /// a read-only/full/name-collision failure.
    pub allow_choose_location: bool,
    /// Batch operations only: move on to the next item instead of stopping.
    pub allow_skip: bool,
}

/// Builds the human-language reason and the base recovery-action set for
/// one `FsError`, independent of the operation it came from.
fn reason_and_actions(error: &FsError) -> (String, Vec<RecoveryAction>) {
    match error {
        FsError::PermissionDenied(_) => (
            t("error.reason.permission_denied").to_string(),
            vec![
                RecoveryAction::TryAgain,
                RecoveryAction::ChooseAnotherLocation,
            ],
        ),
        FsError::ReadOnly(_) => (
            t("error.reason.read_only").to_string(),
            vec![RecoveryAction::ChooseAnotherLocation],
        ),
        FsError::NotFound(_) => (t("error.reason.not_found").to_string(), vec![]),
        FsError::AlreadyExists(_) => (
            t("error.reason.already_exists").to_string(),
            vec![
                RecoveryAction::TryAgain,
                RecoveryAction::ChooseAnotherLocation,
            ],
        ),
        FsError::NotADirectory(_) => (t("error.reason.not_a_directory").to_string(), vec![]),
        FsError::NotEmpty(_) => (
            t("error.reason.not_empty").to_string(),
            vec![RecoveryAction::TryAgain],
        ),
        FsError::InvalidPath(_) => (t("error.reason.invalid_name").to_string(), vec![]),
        FsError::NoHandlerAvailable(_) => (t("error.reason.no_handler").to_string(), vec![]),
        FsError::Archive(_) => (
            t("error.reason.archive").to_string(),
            vec![
                RecoveryAction::TryAgain,
                RecoveryAction::ChooseAnotherLocation,
            ],
        ),
        FsError::Gio { message, .. } => (message.clone(), vec![RecoveryAction::TryAgain]),
        FsError::Io { source, .. } => io_reason_and_actions(source),
    }
}

/// `std::io::Error` doesn't carry the same taxonomy as GIO — Copy/Move/
/// Delete's local-filesystem paths go through `std::fs` directly, so this
/// is the other half of the human-reason mapping the phase requires.
fn io_reason_and_actions(source: &std::io::Error) -> (String, Vec<RecoveryAction>) {
    use std::io::ErrorKind;
    match source.kind() {
        ErrorKind::PermissionDenied => (
            t("error.reason.permission_denied").to_string(),
            vec![
                RecoveryAction::TryAgain,
                RecoveryAction::ChooseAnotherLocation,
            ],
        ),
        ErrorKind::ReadOnlyFilesystem => (
            t("error.reason.read_only").to_string(),
            vec![RecoveryAction::ChooseAnotherLocation],
        ),
        ErrorKind::StorageFull | ErrorKind::QuotaExceeded | ErrorKind::FileTooLarge => (
            t("error.reason.disk_full").to_string(),
            vec![
                RecoveryAction::TryAgain,
                RecoveryAction::ChooseAnotherLocation,
            ],
        ),
        ErrorKind::ResourceBusy | ErrorKind::ExecutableFileBusy => (
            t("error.reason.busy").to_string(),
            vec![RecoveryAction::TryAgain],
        ),
        ErrorKind::NotFound => (t("error.reason.not_found").to_string(), vec![]),
        ErrorKind::AlreadyExists => (
            t("error.reason.already_exists").to_string(),
            vec![
                RecoveryAction::TryAgain,
                RecoveryAction::ChooseAnotherLocation,
            ],
        ),
        ErrorKind::InvalidFilename => (t("error.reason.invalid_name").to_string(), vec![]),
        ErrorKind::ConnectionReset
        | ErrorKind::ConnectionAborted
        | ErrorKind::NotConnected
        | ErrorKind::HostUnreachable
        | ErrorKind::NetworkUnreachable
        | ErrorKind::NetworkDown
        | ErrorKind::TimedOut
        | ErrorKind::BrokenPipe => (
            t("error.reason.network_lost").to_string(),
            vec![RecoveryAction::TryAgain],
        ),
        _ => (
            t("error.reason.generic").to_string(),
            vec![RecoveryAction::TryAgain],
        ),
    }
}

/// Raw OS/GIO detail for the collapsible "Show Details" section — errno,
/// error kind, and the full path, never translated.
fn technical_details(path: &VeyraPath, error: &FsError) -> String {
    match error {
        FsError::Io { source, .. } => {
            let errno = source
                .raw_os_error()
                .map(|code| format!(", errno: {code}"))
                .unwrap_or_default();
            format!("{path}\n{:?}{errno}\n{source}", source.kind())
        }
        FsError::Gio { message, .. } => format!("{path}\nGIO error: {message}"),
        other => format!("{path}\n{other:?}"),
    }
}

/// Classifies `error` (which occurred acting on `path`) into an
/// `ActionableError`, filtering `ctx`'s optional actions in and always
/// appending `Cancel` last.
pub(crate) fn classify(ctx: &ErrorContext, path: &VeyraPath, error: &FsError) -> ActionableError {
    let (human_reason, mut actions) = reason_and_actions(error);

    if !ctx.allow_choose_location {
        actions.retain(|a| *a != RecoveryAction::ChooseAnotherLocation);
    }
    if ctx.allow_skip && !actions.contains(&RecoveryAction::Skip) {
        actions.push(RecoveryAction::Skip);
    }
    actions.push(RecoveryAction::Cancel);
    actions.dedup();

    ActionableError {
        headline: ctx.headline.clone(),
        target_name: path.file_name().unwrap_or_else(|| path.to_string()),
        human_reason,
        technical_details: technical_details(path, error),
        recovery_actions: actions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path() -> VeyraPath {
        VeyraPath::from_local("/home/user/report.pdf")
    }

    fn ctx(allow_choose_location: bool, allow_skip: bool) -> ErrorContext {
        ErrorContext {
            headline: "Couldn't move the file".to_string(),
            allow_choose_location,
            allow_skip,
        }
    }

    #[test]
    fn permission_denied_offers_choose_location_and_try_again() {
        let error = FsError::PermissionDenied(path());
        let result = classify(&ctx(true, false), &path(), &error);
        assert_eq!(result.target_name, "report.pdf");
        assert!(result.human_reason.to_lowercase().contains("permission"));
        assert!(result.recovery_actions.contains(&RecoveryAction::TryAgain));
        assert!(result
            .recovery_actions
            .contains(&RecoveryAction::ChooseAnotherLocation));
        assert_eq!(
            result.recovery_actions.last(),
            Some(&RecoveryAction::Cancel)
        );
    }

    #[test]
    fn choose_location_hidden_when_context_disallows_it() {
        let error = FsError::PermissionDenied(path());
        let result = classify(&ctx(false, false), &path(), &error);
        assert!(!result
            .recovery_actions
            .contains(&RecoveryAction::ChooseAnotherLocation));
    }

    #[test]
    fn skip_only_offered_in_batch_context() {
        let error = FsError::NotFound(path());
        let solo = classify(&ctx(false, false), &path(), &error);
        assert!(!solo.recovery_actions.contains(&RecoveryAction::Skip));

        let batch = classify(&ctx(false, true), &path(), &error);
        assert!(batch.recovery_actions.contains(&RecoveryAction::Skip));
    }

    #[test]
    fn not_found_never_offers_try_again() {
        let error = FsError::NotFound(path());
        let result = classify(&ctx(true, true), &path(), &error);
        assert!(!result.recovery_actions.contains(&RecoveryAction::TryAgain));
    }

    #[test]
    fn read_only_reason_mentions_read_only() {
        let error = FsError::ReadOnly(path());
        let result = classify(&ctx(true, false), &path(), &error);
        assert!(result.human_reason.to_lowercase().contains("read-only"));
    }

    #[test]
    fn io_storage_full_maps_to_disk_full_reason() {
        let source = std::io::Error::from(std::io::ErrorKind::StorageFull);
        let error = FsError::Io {
            path: path(),
            source,
        };
        let result = classify(&ctx(true, false), &path(), &error);
        assert!(result.human_reason.to_lowercase().contains("disk space"));
        assert!(result
            .recovery_actions
            .contains(&RecoveryAction::ChooseAnotherLocation));
    }

    #[test]
    fn io_busy_maps_to_in_use_reason_without_choose_location() {
        let source = std::io::Error::from(std::io::ErrorKind::ResourceBusy);
        let error = FsError::Io {
            path: path(),
            source,
        };
        let result = classify(&ctx(true, false), &path(), &error);
        assert!(result.human_reason.to_lowercase().contains("use"));
        assert!(!result
            .recovery_actions
            .contains(&RecoveryAction::ChooseAnotherLocation));
    }

    #[test]
    fn io_network_lost_offers_try_again_only() {
        let source = std::io::Error::from(std::io::ErrorKind::ConnectionReset);
        let error = FsError::Io {
            path: path(),
            source,
        };
        let result = classify(&ctx(true, false), &path(), &error);
        assert!(result.human_reason.to_lowercase().contains("connection"));
        assert!(result.recovery_actions.contains(&RecoveryAction::TryAgain));
        assert!(!result
            .recovery_actions
            .contains(&RecoveryAction::ChooseAnotherLocation));
    }

    #[test]
    fn technical_details_include_path_and_errno() {
        let source = std::io::Error::from_raw_os_error(28); // ENOSPC
        let error = FsError::Io {
            path: path(),
            source,
        };
        let details = technical_details(&path(), &error);
        assert!(details.contains("report.pdf"));
        assert!(details.contains("errno: 28"));
    }

    #[test]
    fn cancel_always_last_and_actions_deduped() {
        let error = FsError::AlreadyExists(path());
        let result = classify(&ctx(true, true), &path(), &error);
        assert_eq!(
            result.recovery_actions.last(),
            Some(&RecoveryAction::Cancel)
        );
        let mut sorted = result.recovery_actions.clone();
        let before = sorted.len();
        sorted.dedup();
        assert_eq!(before, sorted.len());
    }
}
