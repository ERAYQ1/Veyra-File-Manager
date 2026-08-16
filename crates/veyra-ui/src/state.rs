use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;

use veyra_filesystem::{OperationControl, VeyraPath};

use crate::history::History;

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
}

pub(crate) type SharedState = Rc<RefCell<AppState>>;

impl AppState {
    pub fn new(start_dir: VeyraPath) -> SharedState {
        Rc::new(RefCell::new(Self {
            current_dir: start_dir,
            history: History::new(),
            model: gio::ListStore::new::<gtk4::glib::BoxedAnyObject>(),
            load_control: None,
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
