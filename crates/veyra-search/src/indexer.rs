//! Faz 9 background indexer: walks a directory tree off the caller's
//! thread, deprioritized so it never competes with interactive I/O for CPU
//! or disk bandwidth (Rule #11/#12: never block the UI thread; run heavy
//! work in the background). Uses plain `std::fs`, not GIO — indexing must
//! not depend on the same GLib main context the UI runs on (Rule #42:
//! decouple search from the UI).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::index::{IndexedEntry, SearchIndex};

/// Yield the CPU briefly after this many indexed entries, so a large tree
/// never monopolizes a core for long stretches.
const YIELD_EVERY: usize = 64;
const YIELD_FOR: Duration = Duration::from_millis(5);

/// The depth `walk`'s own tests use when they don't care about the
/// `max_depth` cutoff itself — deep enough that no test fixture tree
/// bottoms out on it by accident.
#[cfg(test)]
const MAX_DEPTH: usize = 64;

/// Spawns a background thread that recursively indexes `root` into `index`,
/// descending at most `max_depth` levels and skipping dotfiles/dot-
/// directories unless `include_hidden` is set. Returns immediately;
/// indexing continues in the background.
pub fn spawn_background_index(
    index: Arc<SearchIndex>,
    root: PathBuf,
    max_depth: usize,
    include_hidden: bool,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        lower_priority();
        walk(&index, &root, 0, max_depth, include_hidden);
    })
}

fn walk(index: &SearchIndex, dir: &Path, depth: usize, max_depth: usize, include_hidden: bool) {
    if depth > max_depth {
        return;
    }
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };

    let mut processed = 0usize;
    for dir_entry in read_dir.flatten() {
        let path = dir_entry.path();
        let Ok(metadata) = dir_entry.metadata() else {
            continue;
        };
        let name = dir_entry.file_name().to_string_lossy().into_owned();
        if !include_hidden && name.starts_with('.') {
            continue;
        }
        let is_dir = metadata.is_dir();
        let mime_type = if is_dir {
            "inode/directory".to_string()
        } else {
            mime_guess::from_path(&path)
                .first_or_octet_stream()
                .essence_str()
                .to_string()
        };
        let modified_unix = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64);

        let indexed = IndexedEntry {
            directory: dir,
            name: &name,
            path: &path,
            is_dir,
            size_bytes: metadata.len(),
            mime_type: &mime_type,
            is_executable: is_executable(&metadata),
            modified_unix,
        };
        if let Err(err) = index.index_entry(&indexed) {
            tracing::warn!(path = %path.display(), error = %err, "failed to index entry");
        }

        processed += 1;
        if processed % YIELD_EVERY == 0 {
            std::thread::sleep(YIELD_FOR);
        }

        if is_dir {
            walk(index, &path, depth + 1, max_depth, include_hidden);
        }
    }
}

#[cfg(unix)]
fn is_executable(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_metadata: &std::fs::Metadata) -> bool {
    false
}

/// Lowers the indexer thread's scheduling priority (`nice(19)`, the lowest
/// standard Unix niceness) so it never competes with interactive work for
/// CPU time. Best-effort: a failure here (e.g. already at the priority
/// floor) just means the indexer runs at normal priority instead of low —
/// never a reason to abort indexing.
#[cfg(unix)]
fn lower_priority() {
    // # Safety
    // `nice(2)` takes a plain integer and only ever adjusts the calling
    // thread's own scheduling priority; it dereferences no pointers and
    // cannot cause memory unsafety. Its only failure mode (`EPERM` at the
    // priority ceiling) is communicated via `errno`, which this function
    // deliberately ignores (best-effort, see doc comment above).
    unsafe {
        libc::nice(19);
    }
}

#[cfg(not(unix))]
fn lower_priority() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Arc;

    #[test]
    fn walks_nested_directories_and_indexes_every_file() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("a.txt"), b"hello").unwrap();
        let nested = temp.path().join("nested");
        fs::create_dir(&nested).unwrap();
        fs::write(nested.join("b.txt"), b"world").unwrap();

        let index = Arc::new(SearchIndex::open_in_memory().unwrap());
        walk(&index, temp.path(), 0, MAX_DEPTH, false);

        let results = index
            .search(&crate::query::parse(""), chrono::Utc::now())
            .unwrap();
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"a.txt"));
        assert!(names.contains(&"b.txt"));
        assert!(names.contains(&"nested"));
    }

    #[test]
    fn missing_root_directory_is_a_silent_no_op() {
        let index = Arc::new(SearchIndex::open_in_memory().unwrap());
        walk(
            &index,
            Path::new("/nonexistent/does/not/exist"),
            0,
            MAX_DEPTH,
            false,
        );

        let results = index
            .search(&crate::query::parse(""), chrono::Utc::now())
            .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn hidden_entries_are_skipped_unless_include_hidden_is_set() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join(".secret"), b"hidden").unwrap();
        fs::write(temp.path().join("visible.txt"), b"shown").unwrap();

        let index = Arc::new(SearchIndex::open_in_memory().unwrap());
        walk(&index, temp.path(), 0, MAX_DEPTH, false);
        let results = index
            .search(&crate::query::parse(""), chrono::Utc::now())
            .unwrap();
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"visible.txt"));
        assert!(!names.contains(&".secret"));

        let index = Arc::new(SearchIndex::open_in_memory().unwrap());
        walk(&index, temp.path(), 0, MAX_DEPTH, true);
        let results = index
            .search(&crate::query::parse(""), chrono::Utc::now())
            .unwrap();
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&".secret"));
    }

    #[test]
    fn max_depth_stops_recursion_at_the_configured_level() {
        let temp = tempfile::tempdir().unwrap();
        let level1 = temp.path().join("l1");
        let level2 = level1.join("l2");
        fs::create_dir_all(&level2).unwrap();
        fs::write(level2.join("deep.txt"), b"deep").unwrap();

        let index = Arc::new(SearchIndex::open_in_memory().unwrap());
        walk(&index, temp.path(), 0, 1, false);
        let results = index
            .search(&crate::query::parse(""), chrono::Utc::now())
            .unwrap();
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"l1"));
        assert!(!names.contains(&"deep.txt"));
    }
}
