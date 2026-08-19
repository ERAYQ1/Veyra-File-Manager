use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::gio;
use libadwaita as adw;

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
    /// Faz 50: this tab's empty-state overlay page (`AdwStatusPage`),
    /// shown instead of the Icon/Compact/Details content whenever the
    /// current listing has zero visible items. Built and reassigned onto
    /// this field in `window::open_tab` once the real widget exists;
    /// `window::update_empty_state` is the only thing that mutates it
    /// afterwards.
    pub empty_page: adw::StatusPage,
    /// Faz 50: the same underlying `model`, filtered by this tab's live
    /// search/quick-filter/hidden-files filter — its `n_items()` is what
    /// `window::update_empty_state` checks, so "no search results" and
    /// "empty folder" can be told apart from the filtered count alone.
    /// Reassigned in `window::open_tab` once the tab's real filter exists.
    pub empty_filter_model: gtk4::FilterListModel,
    /// Faz 50: mirrors the tab's live search query text so
    /// `window::update_empty_state` can tell "no search results" apart from
    /// "empty folder" without needing the full `TabPage`. Reassigned in
    /// `window::open_tab` to share the same `Rc` as `TabPage::search_query`.
    pub search_query: Rc<RefCell<String>>,
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
            empty_page: adw::StatusPage::new(),
            empty_filter_model: gtk4::FilterListModel::new(
                None::<gio::ListStore>,
                None::<gtk4::CustomFilter>,
            ),
            search_query: Rc::new(RefCell::new(String::new())),
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
