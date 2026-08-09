use std::fmt;
use std::path::{Path, PathBuf};

use gio::prelude::*;

/// A filesystem location, unified across local POSIX paths and GIO virtual
/// filesystem URIs (`sftp://`, `smb://`, `trash://`, ...). Every
/// `veyra-filesystem` operation accepts and returns `VeyraPath` so callers
/// never need to branch on local-vs-remote themselves.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VeyraPath {
    /// A native path on the local filesystem.
    Local(PathBuf),
    /// A GIO-resolvable URI (remote mount, trash, or any GVfs backend).
    Uri(String),
}

impl VeyraPath {
    pub fn from_local(path: impl Into<PathBuf>) -> Self {
        VeyraPath::Local(path.into())
    }

    pub fn from_uri(uri: impl Into<String>) -> Self {
        VeyraPath::Uri(uri.into())
    }

    /// Parses a user- or config-supplied string into a `VeyraPath`. Strings
    /// containing a `scheme://` prefix are treated as GIO URIs, everything
    /// else as a local path.
    pub fn parse(input: &str) -> Self {
        if input.contains("://") {
            VeyraPath::Uri(input.to_string())
        } else {
            VeyraPath::Local(PathBuf::from(input))
        }
    }

    pub fn is_local(&self) -> bool {
        matches!(self, VeyraPath::Local(_))
    }

    pub fn as_local_path(&self) -> Option<&Path> {
        match self {
            VeyraPath::Local(path) => Some(path),
            VeyraPath::Uri(_) => None,
        }
    }

    /// The last path/URI segment, suitable as a display or default rename
    /// value. Falls back to the full string when no separator is present.
    pub fn file_name(&self) -> Option<String> {
        match self {
            VeyraPath::Local(path) => path.file_name().map(|n| n.to_string_lossy().into_owned()),
            VeyraPath::Uri(uri) => uri
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .filter(|s| !s.is_empty())
                .map(str::to_string),
        }
    }

    /// Builds the GIO handle used to actually perform I/O against this
    /// location, whether local or remote.
    pub fn to_gio_file(&self) -> gio::File {
        match self {
            VeyraPath::Local(path) => gio::File::for_path(path),
            VeyraPath::Uri(uri) => gio::File::for_uri(uri),
        }
    }

    /// Reconstructs a `VeyraPath` from a resolved `gio::File`, preferring the
    /// native local path when GIO reports one.
    pub fn from_gio_file(file: &gio::File) -> Self {
        match file.path() {
            Some(path) => VeyraPath::Local(path),
            None => VeyraPath::Uri(file.uri().to_string()),
        }
    }
}

impl fmt::Display for VeyraPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VeyraPath::Local(path) => write!(f, "{}", path.display()),
            VeyraPath::Uri(uri) => write!(f, "{uri}"),
        }
    }
}

impl From<PathBuf> for VeyraPath {
    fn from(path: PathBuf) -> Self {
        VeyraPath::Local(path)
    }
}

impl From<&Path> for VeyraPath {
    fn from(path: &Path) -> Self {
        VeyraPath::Local(path.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_detects_uri_scheme() {
        assert_eq!(
            VeyraPath::parse("sftp://user@host/path"),
            VeyraPath::Uri("sftp://user@host/path".to_string())
        );
    }

    #[test]
    fn parse_defaults_to_local() {
        assert_eq!(
            VeyraPath::parse("/home/user/Documents"),
            VeyraPath::Local(PathBuf::from("/home/user/Documents"))
        );
    }

    #[test]
    fn file_name_from_local_path() {
        let path = VeyraPath::from_local("/home/user/notes.txt");
        assert_eq!(path.file_name().as_deref(), Some("notes.txt"));
    }

    #[test]
    fn file_name_from_uri_trailing_slash() {
        let path = VeyraPath::from_uri("smb://server/share/folder/");
        assert_eq!(path.file_name().as_deref(), Some("folder"));
    }

    #[test]
    fn display_renders_local_and_uri_forms() {
        assert_eq!(VeyraPath::from_local("/tmp/file").to_string(), "/tmp/file");
        assert_eq!(VeyraPath::from_uri("trash:///").to_string(), "trash:///");
    }
}
