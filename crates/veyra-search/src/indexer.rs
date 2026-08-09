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

/// Directories are rarely nested this deep; a hard cap protects against
/// symlink cycles or pathological trees turning indexing into unbounded
/// recursion.
const MAX_DEPTH: usize = 64;

/// Spawns a background thread that recursively indexes `root` into
/// `index`. Returns immediately; indexing continues in the background.
pub fn spawn_background_index(
    index: Arc<SearchIndex>,
    root: PathBuf,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        lower_priority();
        walk(&index, &root, 0);
    })
}

fn walk(index: &SearchIndex, dir: &Path, depth: usize) {
    if depth > MAX_DEPTH {
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
            walk(index, &path, depth + 1);
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
        walk(&index, temp.path(), 0);

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
        walk(&index, Path::new("/nonexistent/does/not/exist"), 0);

        let results = index
            .search(&crate::query::parse(""), chrono::Utc::now())
            .unwrap();
        assert!(results.is_empty());
    }
}
