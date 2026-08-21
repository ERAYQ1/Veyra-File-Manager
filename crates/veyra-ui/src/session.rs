//! Faz 34: minimal open-tab session persistence backing the Preferences
//! "Restore Previous Tabs on Startup" toggle — `~/.config/veyra/session.json`
//! records each panel's open tab locations on window close; `build_window`
//! reopens them on the next launch instead of the usual single start
//! directory, but only when the toggle is on. Same load/save/atomic-write
//! shape as `config.rs`/`shortcuts.rs`.

use std::io;
use std::path::{Path, PathBuf};

use gtk4::glib;

use veyra_filesystem::VeyraPath;

/// Every open tab's location in each panel, in tab order. Right is empty
/// when the split view was never activated (or had no tabs of its own).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Session {
    #[serde(default)]
    pub left_tabs: Vec<String>,
    #[serde(default)]
    pub right_tabs: Vec<String>,
}

impl Session {
    /// Loads `~/.config/veyra/session.json`, falling back to an empty
    /// session (no tabs to restore) when the file is missing, unreadable,
    /// or not valid JSON.
    pub(crate) fn load() -> Self {
        load_from(&session_path())
    }

    /// Atomically writes this session snapshot to
    /// `~/.config/veyra/session.json`.
    pub(crate) fn save(&self) -> io::Result<()> {
        save_to(&session_path(), self)
    }

    /// `left_tabs`/`right_tabs`, parsed back into `VeyraPath`s.
    pub(crate) fn left_paths(&self) -> Vec<VeyraPath> {
        self.left_tabs.iter().map(|s| VeyraPath::parse(s)).collect()
    }

    pub(crate) fn right_paths(&self) -> Vec<VeyraPath> {
        self.right_tabs
            .iter()
            .map(|s| VeyraPath::parse(s))
            .collect()
    }
}

fn session_dir() -> PathBuf {
    glib::user_config_dir().join("veyra")
}

fn session_path() -> PathBuf {
    session_dir().join("session.json")
}

fn load_from(path: &Path) -> Session {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Session::default();
    };
    serde_json::from_str(&contents).unwrap_or_default()
}

fn save_to(path: &Path, session: &Session) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(session)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    let tmp_path = path.with_extension("json.tmp");
    veyra_core::security::write_atomic_private(&tmp_path, path, json.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_from_missing_file_returns_empty_session() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist.json");
        assert_eq!(load_from(&path), Session::default());
    }

    #[test]
    fn load_from_corrupt_file_returns_empty_session() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        std::fs::write(&path, b"{ not valid json").unwrap();
        assert_eq!(load_from(&path), Session::default());
    }

    #[test]
    fn save_then_load_round_trips_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("session.json");
        let session = Session {
            left_tabs: vec!["/home/alice/Documents".to_string(), "/tmp".to_string()],
            right_tabs: vec!["/home/alice/Downloads".to_string()],
        };
        save_to(&path, &session).unwrap();
        assert!(path.exists());
        assert!(!path.with_extension("json.tmp").exists());
        assert_eq!(load_from(&path), session);
    }

    #[test]
    fn left_paths_parses_local_and_uri_entries() {
        let session = Session {
            left_tabs: vec!["/home/alice".to_string(), "sftp://host/remote".to_string()],
            right_tabs: vec![],
        };
        let paths = session.left_paths();
        assert_eq!(paths.len(), 2);
        assert!(paths[0].is_local());
        assert!(!paths[1].is_local());
    }

    /// Faz 47: `right_paths` must parse local/URI entries the same way
    /// `left_paths` does — the two panels share no special-casing.
    #[test]
    fn right_paths_parses_local_and_uri_entries() {
        let session = Session {
            left_tabs: vec![],
            right_tabs: vec![
                "/home/alice/Downloads".to_string(),
                "smb://nas/share".to_string(),
            ],
        };
        let paths = session.right_paths();
        assert_eq!(paths.len(), 2);
        assert!(paths[0].is_local());
        assert!(!paths[1].is_local());
    }

    /// Faz 47: a session file truncated mid-write (crash, disk full,
    /// killed process) is exactly as corrupt as hand-edited garbage —
    /// must fall back to an empty session, never panic (Rule #15).
    #[test]
    fn load_from_truncated_json_returns_empty_session() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        std::fs::write(&path, br#"{"left_tabs": ["/home/alice", "#).unwrap();
        assert_eq!(load_from(&path), Session::default());
    }

    /// Faz 47: an empty JSON object is valid (both fields `#[serde(default)]`)
    /// and must load as an empty session rather than an error.
    #[test]
    fn load_from_empty_json_object_returns_empty_session() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        std::fs::write(&path, b"{}").unwrap();
        assert_eq!(load_from(&path), Session::default());
    }

    /// Faz 47: many tabs across both panels, including Unicode/RTL/emoji
    /// path segments, must survive a save/load round trip byte-for-byte —
    /// the session file is plain JSON text, so this is really testing that
    /// nothing along the way silently mangles non-ASCII paths.
    #[test]
    fn save_then_load_round_trips_many_unicode_tabs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let session = Session {
            left_tabs: vec![
                "/home/user/Desktop".to_string(),
                "/home/user/中文_测试".to_string(),
                "/home/user/😀_emoji_folder".to_string(),
                "/home/user/العربية_ملف".to_string(),
                "/home/user/türkçe_şçğüöı".to_string(),
            ],
            right_tabs: vec![
                "/home/user/한국어_파일".to_string(),
                "sftp://host/עברית".to_string(),
            ],
        };
        save_to(&path, &session).unwrap();
        let loaded = load_from(&path);
        assert_eq!(loaded, session);
        assert_eq!(loaded.left_paths().len(), 5);
        assert_eq!(loaded.right_paths().len(), 2);
    }

    /// Faz 47: a session with zero tabs in both panels (e.g. every tab was
    /// closed before quitting) must still round-trip cleanly rather than
    /// being indistinguishable from "no session file at all" in a way that
    /// breaks equality.
    #[test]
    fn save_then_load_round_trips_an_empty_but_present_session() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let session = Session::default();
        save_to(&path, &session).unwrap();
        assert!(path.exists());
        assert_eq!(load_from(&path), session);
    }
}
