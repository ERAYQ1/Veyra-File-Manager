//! Progress reporting for bulk file operations (`queue::run_operation`).

/// Snapshot of progress through a running operation, reported after (and,
/// for Copy/Move, live during) every file.
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub current_file: String,
    pub file_index: usize,
    pub file_count: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

impl Progress {
    /// Overall completion, in `0.0..=100.0`. Falls back to a file-count
    /// ratio when byte totals aren't tracked (Trash/Delete don't move
    /// bytes, so they report progress per-file only).
    pub fn percent(&self) -> f64 {
        if self.bytes_total > 0 {
            (self.bytes_done as f64 / self.bytes_total as f64 * 100.0).clamp(0.0, 100.0)
        } else if self.file_count > 0 {
            (self.file_index as f64 / self.file_count as f64 * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(
        bytes_done: u64,
        bytes_total: u64,
        file_index: usize,
        file_count: usize,
    ) -> Progress {
        Progress {
            current_file: "x".into(),
            file_index,
            file_count,
            bytes_done,
            bytes_total,
        }
    }

    #[test]
    fn percent_uses_bytes_when_known() {
        assert_eq!(progress(50, 200, 0, 4).percent(), 25.0);
    }

    #[test]
    fn percent_falls_back_to_file_count_without_bytes() {
        assert_eq!(progress(0, 0, 2, 4).percent(), 50.0);
    }

    #[test]
    fn percent_never_exceeds_100() {
        assert_eq!(progress(300, 200, 0, 1).percent(), 100.0);
    }

    #[test]
    fn percent_is_zero_with_no_work() {
        assert_eq!(progress(0, 0, 0, 0).percent(), 0.0);
    }
}
