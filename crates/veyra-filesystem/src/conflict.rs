//! Name-collision detection and resolution for Copy/Move operations.

use crate::path::VeyraPath;

/// A destination path that already exists, surfaced before a Copy/Move
/// would overwrite it.
#[derive(Debug, Clone)]
pub struct Conflict {
    pub source: VeyraPath,
    pub destination: VeyraPath,
}

/// The user's (or a remembered blanket policy's) answer to a `Conflict`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictDecision {
    /// Overwrite the destination with this source.
    Replace,
    /// Apply `Replace` to this and every remaining conflict in the batch,
    /// without asking again ("Apply to all").
    ReplaceAll,
    /// Write under a different name instead (e.g. `report (2).pdf`).
    Rename(String),
    /// Leave the destination untouched; this source is not copied/moved.
    Skip,
    /// Apply `Skip` to this and every remaining conflict in the batch.
    SkipAll,
    /// Abort the whole operation.
    Cancel,
}

impl ConflictDecision {
    /// Whether this decision should be remembered and applied to every
    /// later conflict in the same batch without asking again.
    pub fn is_blanket(&self) -> bool {
        matches!(
            self,
            ConflictDecision::ReplaceAll | ConflictDecision::SkipAll
        )
    }
}

/// Suggests a non-colliding name for `name` in the style of `report (2).pdf`,
/// `report (3).pdf`, ... — `exists` is queried for each candidate so the
/// suggestion is guaranteed free at the time it's returned.
pub fn suggest_name(name: &str, exists: impl Fn(&str) -> bool) -> String {
    let (stem, ext) = split_extension(name);
    let mut n = 2u32;
    loop {
        let candidate = match ext {
            Some(ext) => format!("{stem} ({n}).{ext}"),
            None => format!("{stem} ({n})"),
        };
        if !exists(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Splits `name` into `(stem, extension)`. Leading-dot names (`.gitignore`)
/// are treated as having no extension, matching common file-manager UX.
fn split_extension(name: &str) -> (&str, Option<&str>) {
    match name.rfind('.') {
        Some(0) | None => (name, None),
        Some(index) => (&name[..index], Some(&name[index + 1..])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggest_name_starts_at_two() {
        assert_eq!(suggest_name("report.pdf", |_| false), "report (2).pdf");
    }

    #[test]
    fn suggest_name_skips_taken_counters() {
        let name = suggest_name("report.pdf", |c| {
            c == "report (2).pdf" || c == "report (3).pdf"
        });
        assert_eq!(name, "report (4).pdf");
    }

    #[test]
    fn suggest_name_without_extension() {
        assert_eq!(suggest_name("README", |_| false), "README (2)");
    }

    #[test]
    fn suggest_name_preserves_dotfile_as_stem() {
        assert_eq!(suggest_name(".gitignore", |_| false), ".gitignore (2)");
    }

    #[test]
    fn blanket_decisions_are_flagged() {
        assert!(ConflictDecision::ReplaceAll.is_blanket());
        assert!(ConflictDecision::SkipAll.is_blanket());
        assert!(!ConflictDecision::Replace.is_blanket());
        assert!(!ConflictDecision::Skip.is_blanket());
        assert!(!ConflictDecision::Rename("x".into()).is_blanket());
        assert!(!ConflictDecision::Cancel.is_blanket());
    }
}
