use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::gio;

use veyra_filesystem::{GitFileStatus, OperationControl, VeyraPath};

use crate::history::History;

/// File name → Git status for the entries directly inside `current_dir`,
/// shared with every view's row factory so a background `query_dir_git_
/// statuses` result can update already-bound rows' badges (Faz 40). Empty
/// whenever `current_dir` isn't inside a Git repository.
pub(crate) type SharedGitStatuses = Rc<RefCell<HashMap<String, GitFileStatus>>>;

/// Shared, mutable window state: the current location, navigation history,
/// and the single source-of-truth item model every view (Icon/Compact/
/// Details) observes.
pub(crate) struct AppState {
    pub current_dir: VeyraPath,
    pub history: History,
    /// Raw directory contents as `glib::BoxedAnyObject<FileItem>`. Each view
    /// wraps this same store in its own filter/sort/selection chain, so a
    /// single `read_dir` result updates whichever view is visible.
    pub model: gio::ListStore,
    /// Cancel switch for whichever `read_dir_chunked` scan is currently
    /// streaming into `model`, if any. `load_directory` cancels this before
    /// starting a new scan, so navigating away from a huge directory mid-scan
    /// stops it immediately (Rule #13) instead of letting a stale listing
    /// keep appending to a model the user has already left.
    pub load_control: Option<OperationControl>,
    /// Faz 40: `current_dir`'s per-file Git status, refreshed by
    /// `window::update_git_file_statuses` on every navigation/reload.
    pub git_statuses: SharedGitStatuses,
}

pub(crate) type SharedState = Rc<RefCell<AppState>>;

impl AppState {
    pub fn new(start_dir: VeyraPath) -> SharedState {
        Rc::new(RefCell::new(Self {
            current_dir: start_dir,
            history: History::new(),
            model: gio::ListStore::new::<gtk4::glib::BoxedAnyObject>(),
            load_control: None,
            git_statuses: Rc::new(RefCell::new(HashMap::new())),
        }))
    }

    pub fn can_go_back(&self) -> bool {
        self.history.can_go_back()
    }

    pub fn can_go_forward(&self) -> bool {
        self.history.can_go_forward()
    }

    pub fn can_go_up(&self) -> bool {
        self.current_dir
            .as_local_path()
            .is_some_and(|p| p.parent().is_some())
    }
}
