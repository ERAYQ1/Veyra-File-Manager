use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;

use veyra_filesystem::VeyraPath;

/// Shared, mutable window state: the current location, navigation history,
/// and the single source-of-truth item model every view (Icon/Compact/
/// Details) observes.
pub(crate) struct AppState {
    pub current_dir: VeyraPath,
    pub back_stack: Vec<VeyraPath>,
    pub forward_stack: Vec<VeyraPath>,
    /// Raw directory contents as `glib::BoxedAnyObject<FileItem>`. Each view
    /// wraps this same store in its own filter/sort/selection chain, so a
    /// single `read_dir` result updates whichever view is visible.
    pub model: gio::ListStore,
}

pub(crate) type SharedState = Rc<RefCell<AppState>>;

impl AppState {
    pub fn new(start_dir: VeyraPath) -> SharedState {
        Rc::new(RefCell::new(Self {
            current_dir: start_dir,
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
            model: gio::ListStore::new::<gtk4::glib::BoxedAnyObject>(),
        }))
    }

    pub fn can_go_back(&self) -> bool {
        !self.back_stack.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward_stack.is_empty()
    }

    pub fn can_go_up(&self) -> bool {
        self.current_dir
            .as_local_path()
            .is_some_and(|p| p.parent().is_some())
    }
}
