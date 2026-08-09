//! Back/forward navigation history for the active window. Kept separate from
//! `AppState` so the stack-manipulation logic (record/back/forward) has a
//! single owner and its own unit tests, per Rule #34.

use veyra_filesystem::VeyraPath;

#[derive(Default)]
pub(crate) struct History {
    back: Vec<VeyraPath>,
    forward: Vec<VeyraPath>,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records `previous` (the location being left) onto the back stack and
    /// clears the forward stack, per the standard browser-history model: any
    /// "forward" branch is discarded once a new location is visited.
    pub fn record(&mut self, previous: VeyraPath) {
        self.back.push(previous);
        self.forward.clear();
    }

    /// Pops the most recent back entry, pushing `current` onto the forward
    /// stack so `go_forward` can return to it. Returns `None` (no-op) when
    /// the back stack is empty.
    pub fn go_back(&mut self, current: VeyraPath) -> Option<VeyraPath> {
        let previous = self.back.pop()?;
        self.forward.push(current);
        Some(previous)
    }

    /// Pops the most recent forward entry, pushing `current` back onto the
    /// back stack. Returns `None` (no-op) when the forward stack is empty.
    pub fn go_forward(&mut self, current: VeyraPath) -> Option<VeyraPath> {
        let next = self.forward.pop()?;
        self.back.push(current);
        Some(next)
    }

    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn path(s: &str) -> VeyraPath {
        VeyraPath::from_local(PathBuf::from(s))
    }

    #[test]
    fn fresh_history_has_no_back_or_forward() {
        let history = History::new();
        assert!(!history.can_go_back());
        assert!(!history.can_go_forward());
    }

    #[test]
    fn record_enables_back_only() {
        let mut history = History::new();
        history.record(path("/a"));
        assert!(history.can_go_back());
        assert!(!history.can_go_forward());
    }

    #[test]
    fn go_back_returns_previous_and_enables_forward() {
        let mut history = History::new();
        history.record(path("/a"));
        let previous = history.go_back(path("/b"));
        assert_eq!(previous, Some(path("/a")));
        assert!(!history.can_go_back());
        assert!(history.can_go_forward());
    }

    #[test]
    fn go_forward_returns_to_current_after_back() {
        let mut history = History::new();
        history.record(path("/a"));
        history.go_back(path("/b"));
        let next = history.go_forward(path("/a"));
        assert_eq!(next, Some(path("/b")));
        assert!(history.can_go_back());
        assert!(!history.can_go_forward());
    }

    #[test]
    fn go_back_on_empty_stack_is_noop() {
        let mut history = History::new();
        assert_eq!(history.go_back(path("/a")), None);
        assert!(!history.can_go_forward());
    }

    #[test]
    fn go_forward_on_empty_stack_is_noop() {
        let mut history = History::new();
        assert_eq!(history.go_forward(path("/a")), None);
        assert!(!history.can_go_back());
    }

    #[test]
    fn new_navigation_clears_forward_stack() {
        let mut history = History::new();
        history.record(path("/a"));
        history.go_back(path("/b"));
        assert!(history.can_go_forward());

        history.record(path("/b"));
        assert!(!history.can_go_forward());
        assert!(history.can_go_back());
    }

    #[test]
    fn multi_step_back_and_forward_round_trip() {
        let mut history = History::new();
        history.record(path("/a"));
        history.record(path("/b"));
        history.record(path("/c"));

        assert_eq!(history.go_back(path("/d")), Some(path("/c")));
        assert_eq!(history.go_back(path("/c")), Some(path("/b")));
        assert_eq!(history.go_back(path("/b")), Some(path("/a")));
        assert!(!history.can_go_back());

        assert_eq!(history.go_forward(path("/a")), Some(path("/b")));
        assert_eq!(history.go_forward(path("/b")), Some(path("/c")));
        assert_eq!(history.go_forward(path("/c")), Some(path("/d")));
        assert!(!history.can_go_forward());
    }
}
