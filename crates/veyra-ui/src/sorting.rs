//! Faz 13: the shared sorting and quick-filtering engine every view
//! (Icon/Compact/Details) and the header bar's Sort & Filter menu drive
//! together, so all three stay perfectly synchronized within a tab.
//!
//! [`SortConfig`] is the single source of truth for a tab's ordering;
//! [`compare_items`] is the one comparator every view's `GtkCustomSorter`
//! calls. [`QuickFilter`] is ANDed with the free-text search filter in
//! `window.rs`'s combined `GtkCustomFilter`.

use std::cell::RefCell;
use std::cmp::Ordering as StdOrdering;
use std::rc::Rc;

use chrono::{DateTime, Utc};
use gtk4::glib;
use gtk4::prelude::*;

use veyra_filesystem::FileItem;

/// Files larger than this count as "Large Files" in the quick filter.
const LARGE_FILE_THRESHOLD_BYTES: u64 = 100 * 1024 * 1024;

/// Window (in days, relative to "now") for the "Recently Modified" quick
/// filter.
const RECENTLY_MODIFIED_DAYS: i64 = 7;

/// The criteria a tab can sort its directory listing by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SortKey {
    Name,
    Size,
    Type,
    Modified,
    Created,
    Accessed,
    Owner,
}

impl SortKey {
    pub(crate) const ALL: [SortKey; 7] = [
        SortKey::Name,
        SortKey::Size,
        SortKey::Type,
        SortKey::Modified,
        SortKey::Created,
        SortKey::Accessed,
        SortKey::Owner,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            SortKey::Name => "Name",
            SortKey::Size => "Size",
            SortKey::Type => "Type",
            SortKey::Modified => "Modified",
            SortKey::Created => "Created",
            SortKey::Accessed => "Accessed",
            SortKey::Owner => "Owner",
        }
    }
}

/// Sort direction, applied after `SortKey` comparison (folders-first, when
/// enabled, always wins regardless of direction).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SortOrder {
    #[default]
    Ascending,
    Descending,
}

impl SortOrder {
    pub(crate) fn apply(self, ordering: StdOrdering) -> StdOrdering {
        match self {
            SortOrder::Ascending => ordering,
            SortOrder::Descending => ordering.reverse(),
        }
    }

    pub(crate) fn to_gtk(self) -> gtk4::SortType {
        match self {
            SortOrder::Ascending => gtk4::SortType::Ascending,
            SortOrder::Descending => gtk4::SortType::Descending,
        }
    }

    pub(crate) fn from_gtk(sort_type: gtk4::SortType) -> Self {
        match sort_type {
            gtk4::SortType::Descending => SortOrder::Descending,
            _ => SortOrder::Ascending,
        }
    }
}

/// A tab's full sort configuration: criterion, direction, and whether
/// directories are pinned ahead of files regardless of the other two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SortConfig {
    pub key: SortKey,
    pub order: SortOrder,
    pub folders_first: bool,
}

impl Default for SortConfig {
    fn default() -> Self {
        SortConfig {
            key: SortKey::Name,
            order: SortOrder::Ascending,
            folders_first: true,
        }
    }
}

/// The one comparator every view's sorter (Icon/Compact/Details) calls,
/// so all three list the same directory in the same order.
pub(crate) fn compare_items(a: &FileItem, b: &FileItem, config: &SortConfig) -> StdOrdering {
    if config.folders_first {
        let a_dir = a.kind().is_directory();
        let b_dir = b.kind().is_directory();
        if a_dir != b_dir {
            return b_dir.cmp(&a_dir);
        }
    }
    let ordering = key_cmp(a, b, config.key);
    config.order.apply(ordering)
}

fn key_cmp(a: &FileItem, b: &FileItem, key: SortKey) -> StdOrdering {
    match key {
        SortKey::Name => natural_name_cmp(a.name(), b.name()),
        SortKey::Size => a
            .metadata
            .size_bytes
            .cmp(&b.metadata.size_bytes)
            .then_with(|| natural_name_cmp(a.name(), b.name())),
        SortKey::Type => type_key(a)
            .cmp(&type_key(b))
            .then_with(|| natural_name_cmp(a.name(), b.name())),
        SortKey::Modified => a
            .metadata
            .modified
            .cmp(&b.metadata.modified)
            .then_with(|| natural_name_cmp(a.name(), b.name())),
        SortKey::Created => a
            .metadata
            .created
            .cmp(&b.metadata.created)
            .then_with(|| natural_name_cmp(a.name(), b.name())),
        SortKey::Accessed => a
            .metadata
            .accessed
            .cmp(&b.metadata.accessed)
            .then_with(|| natural_name_cmp(a.name(), b.name())),
        SortKey::Owner => a
            .metadata
            .owner
            .cmp(&b.metadata.owner)
            .then_with(|| natural_name_cmp(a.name(), b.name())),
    }
}

/// Extension (lowercased, no dot) if there is one, else the MIME type — a
/// stable, cheap "type" key that groups files the same way the Details
/// view's Type column reads.
fn type_key(item: &FileItem) -> String {
    if item.kind().is_directory() {
        return String::new();
    }
    match item.path.as_local_path().and_then(|p| p.extension()) {
        Some(ext) => ext.to_string_lossy().to_lowercase(),
        None => item.metadata.mime_type.clone(),
    }
}

/// Case-insensitive natural sort: runs of digits compare numerically, so
/// "file2" sorts before "file10".
fn natural_name_cmp(a: &str, b: &str) -> StdOrdering {
    let a = a.to_lowercase();
    let b = b.to_lowercase();
    let mut a_chars = a.chars().peekable();
    let mut b_chars = b.chars().peekable();

    loop {
        let (Some(&ac), Some(&bc)) = (a_chars.peek(), b_chars.peek()) else {
            return a_chars.next().is_some().cmp(&b_chars.next().is_some());
        };

        if ac.is_ascii_digit() && bc.is_ascii_digit() {
            let a_num = take_number(&mut a_chars);
            let b_num = take_number(&mut b_chars);
            match a_num.cmp(&b_num) {
                StdOrdering::Equal => continue,
                other => return other,
            }
        }

        match ac.cmp(&bc) {
            StdOrdering::Equal => {
                a_chars.next();
                b_chars.next();
            }
            other => return other,
        }
    }
}

fn take_number(chars: &mut std::iter::Peekable<std::str::Chars>) -> u64 {
    let mut value: u64 = 0;
    while let Some(&c) = chars.peek() {
        if let Some(digit) = c.to_digit(10) {
            value = value.saturating_mul(10).saturating_add(digit as u64);
            chars.next();
        } else {
            break;
        }
    }
    value
}

/// Builds the shared `GtkCustomSorter` a tab's Icon, Compact, and Details
/// views all use, so changing `config` (from the menu or a Details column
/// header click) re-sorts every view at once via `Sorter::changed`.
pub(crate) fn build_sorter(config: Rc<RefCell<SortConfig>>) -> gtk4::CustomSorter {
    gtk4::CustomSorter::new(move |a, b| {
        let a = a
            .downcast_ref::<glib::BoxedAnyObject>()
            .expect("model item must be BoxedAnyObject<FileItem>")
            .borrow::<FileItem>();
        let b = b
            .downcast_ref::<glib::BoxedAnyObject>()
            .expect("model item must be BoxedAnyObject<FileItem>")
            .borrow::<FileItem>();
        compare_items(&a, &b, &config.borrow()).into()
    })
}

/// The quick file-type/attribute filters available from the header bar's
/// Sort & Filter menu, ANDed with the free-text search query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum QuickFilter {
    #[default]
    All,
    Images,
    Videos,
    Documents,
    Archives,
    Executables,
    LargeFiles,
    RecentlyModified,
}

impl QuickFilter {
    pub(crate) const ALL: [QuickFilter; 8] = [
        QuickFilter::All,
        QuickFilter::Images,
        QuickFilter::Videos,
        QuickFilter::Documents,
        QuickFilter::Archives,
        QuickFilter::Executables,
        QuickFilter::LargeFiles,
        QuickFilter::RecentlyModified,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            QuickFilter::All => "All Files",
            QuickFilter::Images => "Images",
            QuickFilter::Videos => "Videos",
            QuickFilter::Documents => "Documents",
            QuickFilter::Archives => "Archives",
            QuickFilter::Executables => "Executables",
            QuickFilter::LargeFiles => "Large Files (>100 MB)",
            QuickFilter::RecentlyModified => "Recently Modified (7d)",
        }
    }

    const IMAGE_EXTENSIONS: &'static [&'static str] =
        &["png", "jpg", "jpeg", "webp", "svg", "gif", "bmp", "ico"];
    const VIDEO_EXTENSIONS: &'static [&'static str] = &["mp4", "mkv", "avi", "mov", "webm"];
    const DOCUMENT_EXTENSIONS: &'static [&'static str] = &[
        "doc", "docx", "odt", "xlsx", "pptx", "csv", "md", "json", "xml", "pdf", "txt",
    ];
    const ARCHIVE_EXTENSIONS: &'static [&'static str] =
        &["zip", "tar", "gz", "7z", "xz", "bz2", "zst", "rar"];
}

/// Faz 14: whether `item` should be visible given the tab's hidden-files
/// toggle. `is_hidden` already covers both dotfiles and a directory's
/// `.hidden` listing (GIO's `standard::is-hidden`, see
/// `veyra-filesystem`'s `build_file_item`) — this just applies the toggle.
pub(crate) fn passes_hidden_filter(item: &FileItem, show_hidden: bool) -> bool {
    show_hidden || !item.metadata.is_hidden
}

/// Directories always pass every quick filter (they're navigation, not
/// filtered content) — only regular files are matched against the type
/// criteria below.
pub(crate) fn quick_filter_matches(
    item: &FileItem,
    filter: QuickFilter,
    now: DateTime<Utc>,
) -> bool {
    if filter == QuickFilter::All || item.kind().is_directory() {
        return true;
    }

    match filter {
        QuickFilter::All => true,
        QuickFilter::Images => {
            item.metadata.mime_type.starts_with("image/")
                || has_extension(item, QuickFilter::IMAGE_EXTENSIONS)
        }
        QuickFilter::Videos => {
            item.metadata.mime_type.starts_with("video/")
                || has_extension(item, QuickFilter::VIDEO_EXTENSIONS)
        }
        QuickFilter::Documents => {
            item.metadata.mime_type.starts_with("text/")
                || item.metadata.mime_type == "application/pdf"
                || has_extension(item, QuickFilter::DOCUMENT_EXTENSIONS)
        }
        QuickFilter::Archives => has_extension(item, QuickFilter::ARCHIVE_EXTENSIONS),
        QuickFilter::Executables => item.metadata.is_executable(),
        QuickFilter::LargeFiles => item.metadata.size_bytes > LARGE_FILE_THRESHOLD_BYTES,
        QuickFilter::RecentlyModified => item.metadata.modified.is_some_and(|modified| {
            now.signed_duration_since(modified) <= chrono::Duration::days(RECENTLY_MODIFIED_DAYS)
                && modified <= now
        }),
    }
}

fn has_extension(item: &FileItem, extensions: &[&str]) -> bool {
    item.path
        .as_local_path()
        .and_then(|p| p.extension())
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .is_some_and(|ext| extensions.contains(&ext.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use veyra_filesystem::{FileKind, FileMetadata, VeyraPath};

    fn item(name: &str, kind: FileKind) -> FileItem {
        let path = VeyraPath::Local(PathBuf::from(format!("/tmp/{name}")));
        FileItem {
            path: path.clone(),
            metadata: FileMetadata {
                name: name.to_string(),
                path,
                kind: kind.clone(),
                size_bytes: 0,
                modified: None,
                created: None,
                accessed: None,
                permissions: None,
                owner: None,
                group: None,
                mime_type: "application/octet-stream".to_string(),
                inode: None,
                is_hidden: false,
            },
            target_symlink: None,
        }
    }

    fn file_with(
        name: &str,
        size: u64,
        modified: Option<DateTime<Utc>>,
        mime: &str,
        executable: bool,
    ) -> FileItem {
        let mut f = item(name, FileKind::Regular);
        f.metadata.size_bytes = size;
        f.metadata.modified = modified;
        f.metadata.created = modified;
        f.metadata.accessed = modified;
        f.metadata.mime_type = mime.to_string();
        f.metadata.owner = Some("alice".to_string());
        if executable {
            f.metadata.permissions = Some(veyra_filesystem::FilePermissions::from_mode(0o755));
        }
        f
    }

    fn dir(name: &str) -> FileItem {
        item(name, FileKind::Directory)
    }

    // --- SortKey::Name / natural sort ---

    #[test]
    fn natural_sort_orders_numbers_numerically() {
        assert_eq!(natural_name_cmp("file2", "file10"), StdOrdering::Less);
        assert_eq!(natural_name_cmp("file10", "file2"), StdOrdering::Greater);
        assert_eq!(natural_name_cmp("File", "file"), StdOrdering::Equal);
    }

    #[test]
    fn name_sort_is_case_insensitive() {
        let config = SortConfig {
            key: SortKey::Name,
            order: SortOrder::Ascending,
            folders_first: false,
        };
        let a = file_with("Banana", 0, None, "text/plain", false);
        let b = file_with("apple", 0, None, "text/plain", false);
        assert_eq!(compare_items(&a, &b, &config), StdOrdering::Greater);
    }

    // --- folders_first ---

    #[test]
    fn folders_first_wins_regardless_of_key_or_order() {
        let config = SortConfig {
            key: SortKey::Size,
            order: SortOrder::Descending,
            folders_first: true,
        };
        let d = dir("zzz");
        let f = file_with("aaa", 999_999, None, "text/plain", false);
        assert_eq!(compare_items(&d, &f, &config), StdOrdering::Less);
    }

    #[test]
    fn folders_first_disabled_sorts_purely_by_key() {
        let config = SortConfig {
            key: SortKey::Name,
            order: SortOrder::Ascending,
            folders_first: false,
        };
        let d = dir("zzz");
        let f = file_with("aaa", 0, None, "text/plain", false);
        assert_eq!(compare_items(&d, &f, &config), StdOrdering::Greater);
    }

    // --- SortOrder ---

    #[test]
    fn descending_reverses_ascending_result() {
        let asc = SortConfig {
            key: SortKey::Size,
            order: SortOrder::Ascending,
            folders_first: false,
        };
        let desc = SortConfig {
            order: SortOrder::Descending,
            ..asc
        };
        let a = file_with("a", 10, None, "text/plain", false);
        let b = file_with("b", 20, None, "text/plain", false);
        assert_eq!(compare_items(&a, &b, &asc), StdOrdering::Less);
        assert_eq!(compare_items(&a, &b, &desc), StdOrdering::Greater);
    }

    // --- every SortKey, ascending ---

    #[test]
    fn sort_by_size() {
        let config = SortConfig {
            key: SortKey::Size,
            ..SortConfig::default()
        };
        let small = file_with("a", 10, None, "text/plain", false);
        let big = file_with("b", 20, None, "text/plain", false);
        assert_eq!(
            compare_items(
                &small,
                &big,
                &SortConfig {
                    folders_first: false,
                    ..config
                }
            ),
            StdOrdering::Less
        );
    }

    #[test]
    fn sort_by_type_groups_same_extension() {
        let config = SortConfig {
            key: SortKey::Type,
            folders_first: false,
            ..SortConfig::default()
        };
        let mut a = file_with("a.txt", 0, None, "text/plain", false);
        let mut b = file_with("b.txt", 0, None, "text/plain", false);
        a.path = VeyraPath::Local(PathBuf::from("/tmp/a.txt"));
        b.path = VeyraPath::Local(PathBuf::from("/tmp/b.txt"));
        let c = {
            let mut c = file_with("c.zip", 0, None, "application/zip", false);
            c.path = VeyraPath::Local(PathBuf::from("/tmp/c.zip"));
            c
        };
        // Same extension ties break by name.
        assert_eq!(compare_items(&a, &b, &config), StdOrdering::Less);
        // Different extension: "txt" < "zip".
        assert_eq!(compare_items(&a, &c, &config), StdOrdering::Less);
    }

    #[test]
    fn sort_by_modified_created_accessed() {
        let earlier = DateTime::<Utc>::UNIX_EPOCH;
        let later = earlier + chrono::Duration::days(1);
        let a = file_with("a", 0, Some(earlier), "text/plain", false);
        let b = file_with("b", 0, Some(later), "text/plain", false);

        for key in [SortKey::Modified, SortKey::Created, SortKey::Accessed] {
            let config = SortConfig {
                key,
                folders_first: false,
                ..SortConfig::default()
            };
            assert_eq!(compare_items(&a, &b, &config), StdOrdering::Less, "{key:?}");
        }
    }

    #[test]
    fn sort_by_owner() {
        let config = SortConfig {
            key: SortKey::Owner,
            folders_first: false,
            ..SortConfig::default()
        };
        let mut a = file_with("a", 0, None, "text/plain", false);
        a.metadata.owner = Some("alice".to_string());
        let mut b = file_with("b", 0, None, "text/plain", false);
        b.metadata.owner = Some("bob".to_string());
        assert_eq!(compare_items(&a, &b, &config), StdOrdering::Less);
    }

    // --- QuickFilter ---

    // --- passes_hidden_filter (Faz 14) ---

    #[test]
    fn hidden_item_is_filtered_out_when_show_hidden_is_false() {
        let mut hidden = file_with("secret", 0, None, "text/plain", false);
        hidden.metadata.is_hidden = true;
        assert!(!passes_hidden_filter(&hidden, false));
    }

    #[test]
    fn hidden_item_is_shown_when_show_hidden_is_true() {
        let mut hidden = file_with(".bashrc", 0, None, "text/plain", false);
        hidden.metadata.is_hidden = true;
        assert!(passes_hidden_filter(&hidden, true));
    }

    #[test]
    fn visible_item_always_passes_regardless_of_toggle() {
        let visible = file_with("readme.md", 0, None, "text/plain", false);
        assert!(passes_hidden_filter(&visible, false));
        assert!(passes_hidden_filter(&visible, true));
    }

    #[test]
    fn quick_filter_all_matches_everything() {
        let f = file_with("a.zip", 0, None, "application/zip", false);
        assert!(quick_filter_matches(&f, QuickFilter::All, Utc::now()));
    }

    #[test]
    fn quick_filter_directories_always_pass() {
        let d = dir("some_dir");
        for filter in QuickFilter::ALL {
            assert!(quick_filter_matches(&d, filter, Utc::now()), "{filter:?}");
        }
    }

    #[test]
    fn quick_filter_images_by_mime_and_extension() {
        let mut by_mime = file_with("photo.bin", 0, None, "image/png", false);
        by_mime.path = VeyraPath::Local(PathBuf::from("/tmp/photo.bin"));
        assert!(quick_filter_matches(
            &by_mime,
            QuickFilter::Images,
            Utc::now()
        ));

        let mut by_ext = file_with("photo.jpeg", 0, None, "application/octet-stream", false);
        by_ext.path = VeyraPath::Local(PathBuf::from("/tmp/photo.jpeg"));
        assert!(quick_filter_matches(
            &by_ext,
            QuickFilter::Images,
            Utc::now()
        ));

        let mut not_image = file_with("doc.pdf", 0, None, "application/pdf", false);
        not_image.path = VeyraPath::Local(PathBuf::from("/tmp/doc.pdf"));
        assert!(!quick_filter_matches(
            &not_image,
            QuickFilter::Images,
            Utc::now()
        ));
    }

    #[test]
    fn quick_filter_videos() {
        let mut v = file_with("clip.mkv", 0, None, "application/octet-stream", false);
        v.path = VeyraPath::Local(PathBuf::from("/tmp/clip.mkv"));
        assert!(quick_filter_matches(&v, QuickFilter::Videos, Utc::now()));
    }

    #[test]
    fn quick_filter_documents() {
        let mut d = file_with("notes.md", 0, None, "text/markdown", false);
        d.path = VeyraPath::Local(PathBuf::from("/tmp/notes.md"));
        assert!(quick_filter_matches(&d, QuickFilter::Documents, Utc::now()));
    }

    #[test]
    fn quick_filter_archives() {
        for ext in ["zip", "tar", "gz", "7z", "xz", "bz2", "zst", "rar"] {
            let mut a = file_with("bundle", 0, None, "application/octet-stream", false);
            a.path = VeyraPath::Local(PathBuf::from(format!("/tmp/bundle.{ext}")));
            assert!(
                quick_filter_matches(&a, QuickFilter::Archives, Utc::now()),
                "{ext}"
            );
        }
    }

    #[test]
    fn quick_filter_executables() {
        let exe = file_with("run", 0, None, "application/octet-stream", true);
        assert!(quick_filter_matches(
            &exe,
            QuickFilter::Executables,
            Utc::now()
        ));

        let not_exe = file_with("run", 0, None, "application/octet-stream", false);
        assert!(!quick_filter_matches(
            &not_exe,
            QuickFilter::Executables,
            Utc::now()
        ));
    }

    #[test]
    fn quick_filter_large_files() {
        let big = file_with(
            "iso",
            LARGE_FILE_THRESHOLD_BYTES + 1,
            None,
            "application/octet-stream",
            false,
        );
        assert!(quick_filter_matches(
            &big,
            QuickFilter::LargeFiles,
            Utc::now()
        ));

        let small = file_with(
            "iso",
            LARGE_FILE_THRESHOLD_BYTES,
            None,
            "application/octet-stream",
            false,
        );
        assert!(!quick_filter_matches(
            &small,
            QuickFilter::LargeFiles,
            Utc::now()
        ));
    }

    #[test]
    fn quick_filter_recently_modified() {
        let now = Utc::now();
        let recent = file_with(
            "a",
            0,
            Some(now - chrono::Duration::days(1)),
            "text/plain",
            false,
        );
        assert!(quick_filter_matches(
            &recent,
            QuickFilter::RecentlyModified,
            now
        ));

        let old = file_with(
            "a",
            0,
            Some(now - chrono::Duration::days(30)),
            "text/plain",
            false,
        );
        assert!(!quick_filter_matches(
            &old,
            QuickFilter::RecentlyModified,
            now
        ));

        let never_modified = file_with("a", 0, None, "text/plain", false);
        assert!(!quick_filter_matches(
            &never_modified,
            QuickFilter::RecentlyModified,
            now
        ));
    }

    // --- SortOrder helpers ---

    #[test]
    fn sort_order_gtk_roundtrip() {
        assert_eq!(
            SortOrder::from_gtk(SortOrder::Ascending.to_gtk()),
            SortOrder::Ascending
        );
        assert_eq!(
            SortOrder::from_gtk(SortOrder::Descending.to_gtk()),
            SortOrder::Descending
        );
    }
}
