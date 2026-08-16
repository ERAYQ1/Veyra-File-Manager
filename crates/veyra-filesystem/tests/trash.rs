use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use veyra_filesystem::{
    delete, empty_trash, list_trash, restore_from_trash, trash, trash_tracked, VeyraPath,
};

/// Every test in this file touches the real (or, for `empty_trash`, an
/// `XDG_DATA_HOME`-redirected) trash and/or process-wide environment
/// variables — serialized so concurrently-run tests in this binary never
/// observe or clobber each other's trash entries or env overrides.
static TRASH_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Trashes a file, restores it, then permanently deletes it again — a full
/// round trip that leaves no residue in the real desktop trash can, while
/// still exercising the actual `g_file_trash` GIO call and the Trash-spec
/// based restore (not a mock of either).
#[test]
fn trash_then_restore_round_trip() {
    let _guard = TRASH_TEST_LOCK.lock().unwrap();
    // GIO refuses to trash files on "internal system mounts" like `/tmp`
    // (tempfile's default location) since there's no meaningful per-mount
    // trash for them. Use a directory under $HOME instead, matching where
    // Veyra's real trash operations actually run.
    let dir = tempfile::Builder::new()
        .prefix("veyra-trash-test-")
        .tempdir_in(std::env::var("HOME").expect("HOME must be set"))
        .unwrap();
    let original = dir.path().join("to_be_trashed.txt");
    fs::write(&original, b"payload").unwrap();

    let path = VeyraPath::from_local(&original);
    trash(&path).unwrap();
    assert!(!original.exists());

    // `trash()` only writes the on-disk Trash/{files,info} structure; it
    // doesn't hand back where it put the file. Find it the same way a
    // trash-browsing UI reading Trash/info directly would: by matching the
    // recorded original path, which survives GIO renaming our file on a
    // name collision with an unrelated pre-existing trash entry.
    let trashed =
        find_in_trash(&original).expect("trashed file should be discoverable in Trash/info");

    let restored = restore_from_trash(&trashed).unwrap();
    assert!(restored.as_local_path().unwrap().exists());
    assert_eq!(
        fs::read(restored.as_local_path().unwrap()).unwrap(),
        b"payload"
    );

    delete(&restored).unwrap();
}

/// Faz 33: unlike `trash`, `trash_tracked` hands back exactly where it put
/// the file — no need to search `Trash/info` for it — and that location must
/// itself be a valid `restore_from_trash` argument, since that's the whole
/// point (the Undo/Redo engine's raw material for reversing a Trash).
#[test]
fn trash_tracked_returns_a_restorable_location() {
    let _guard = TRASH_TEST_LOCK.lock().unwrap();
    let dir = tempfile::Builder::new()
        .prefix("veyra-trash-tracked-test-")
        .tempdir_in(std::env::var("HOME").expect("HOME must be set"))
        .unwrap();
    let original = dir.path().join("tracked.txt");
    fs::write(&original, b"tracked payload").unwrap();

    let path = VeyraPath::from_local(&original);
    let trashed = trash_tracked(&path).unwrap();
    assert!(!original.exists());
    assert!(trashed.as_local_path().unwrap().exists());

    let restored = restore_from_trash(&trashed).unwrap();
    assert_eq!(restored, path);
    assert_eq!(fs::read(&original).unwrap(), b"tracked payload");

    delete(&path).unwrap();
}

/// Two same-named items trashed in a row must land at two distinct
/// `Trash/files` entries, each independently restorable back to its own
/// original location — otherwise the second trash would silently clobber
/// the first's data.
#[test]
fn trash_tracked_deduplicates_colliding_names() {
    let _guard = TRASH_TEST_LOCK.lock().unwrap();
    let dir_a = tempfile::Builder::new()
        .prefix("veyra-trash-tracked-a-")
        .tempdir_in(std::env::var("HOME").expect("HOME must be set"))
        .unwrap();
    let dir_b = tempfile::Builder::new()
        .prefix("veyra-trash-tracked-b-")
        .tempdir_in(std::env::var("HOME").expect("HOME must be set"))
        .unwrap();
    let original_a = dir_a.path().join("dup.txt");
    let original_b = dir_b.path().join("dup.txt");
    fs::write(&original_a, b"a").unwrap();
    fs::write(&original_b, b"b").unwrap();

    let path_a = VeyraPath::from_local(&original_a);
    let path_b = VeyraPath::from_local(&original_b);
    let trashed_a = trash_tracked(&path_a).unwrap();
    let trashed_b = trash_tracked(&path_b).unwrap();
    assert_ne!(trashed_a, trashed_b);

    let restored_a = restore_from_trash(&trashed_a).unwrap();
    let restored_b = restore_from_trash(&trashed_b).unwrap();
    assert_eq!(fs::read(restored_a.as_local_path().unwrap()).unwrap(), b"a");
    assert_eq!(fs::read(restored_b.as_local_path().unwrap()).unwrap(), b"b");

    delete(&path_a).unwrap();
    delete(&path_b).unwrap();
}

/// `list_trash` must surface an entry that was just trashed, with the
/// physical `Trash/files/...` path a restore/delete call expects — reading
/// the real home trash (which may already hold unrelated pre-existing
/// entries), so this only asserts our own uniquely-named entry shows up,
/// never anything about the total count.
#[test]
fn list_trash_includes_freshly_trashed_entry() {
    let _guard = TRASH_TEST_LOCK.lock().unwrap();
    let dir = tempfile::Builder::new()
        .prefix("veyra-trash-test-")
        .tempdir_in(std::env::var("HOME").expect("HOME must be set"))
        .unwrap();
    let original = dir.path().join("list_me.txt");
    fs::write(&original, b"payload").unwrap();

    trash(&VeyraPath::from_local(&original)).unwrap();
    let trashed = find_in_trash(&original).expect("trashed file should be discoverable");

    let items = list_trash().unwrap();
    let found = items
        .iter()
        .find(|item| item.path == trashed)
        .expect("list_trash should include the freshly trashed entry");
    // GIO renames on a name collision with an unrelated pre-existing trash
    // entry (see `trash_then_restore_round_trip`), so match by prefix rather
    // than the exact original name.
    assert!(found.name().starts_with("list_me"));

    delete(&trashed).unwrap();
}

/// `restore_from_trash` must recreate a deleted original parent directory
/// rather than failing, so a trashed file survives its containing folder
/// itself being removed in the meantime.
#[test]
fn restore_recreates_missing_parent_directory() {
    let _guard = TRASH_TEST_LOCK.lock().unwrap();
    let dir = tempfile::Builder::new()
        .prefix("veyra-trash-test-")
        .tempdir_in(std::env::var("HOME").expect("HOME must be set"))
        .unwrap();
    let subdir = dir.path().join("nested");
    fs::create_dir(&subdir).unwrap();
    let original = subdir.join("orphaned.txt");
    fs::write(&original, b"payload").unwrap();

    trash(&VeyraPath::from_local(&original)).unwrap();
    let trashed = find_in_trash(&original).expect("trashed file should be discoverable");

    // The original parent directory is gone by the time we restore.
    fs::remove_dir_all(&subdir).unwrap();
    assert!(!subdir.exists());

    let restored = restore_from_trash(&trashed).unwrap();
    let restored_path = restored.as_local_path().unwrap();
    assert!(restored_path.exists());
    assert_eq!(fs::read(restored_path).unwrap(), b"payload");

    delete(&restored).unwrap();
}

/// `empty_trash` against an isolated `XDG_DATA_HOME`-redirected trash root
/// removes every `files/`/`info/` entry — never run against the real user
/// trash, to avoid destroying anything actually sitting in it. The trash
/// entry is built directly with `std::fs` (not `trash()`/GIO) because GLib
/// caches `g_get_user_data_dir()` on first use within the process, so a
/// later in-process `XDG_DATA_HOME` override wouldn't reliably redirect it.
#[test]
fn empty_trash_clears_isolated_trash_root() {
    let _guard = TRASH_TEST_LOCK.lock().unwrap();
    let isolated_data_home = tempfile::tempdir().unwrap();
    let previous_xdg_data_home = std::env::var("XDG_DATA_HOME").ok();
    // SAFETY: serialized by `TRASH_TEST_LOCK` — no other thread in this
    // process reads/writes `XDG_DATA_HOME` while this override is active.
    unsafe {
        std::env::set_var("XDG_DATA_HOME", isolated_data_home.path());
    }

    let files_dir = isolated_data_home.path().join("Trash/files");
    let info_dir = isolated_data_home.path().join("Trash/info");
    fs::create_dir_all(&files_dir).unwrap();
    fs::create_dir_all(&info_dir).unwrap();
    fs::write(files_dir.join("to_be_emptied.txt"), b"payload").unwrap();
    fs::write(
        info_dir.join("to_be_emptied.txt.trashinfo"),
        "[Trash Info]\nPath=/tmp/to_be_emptied.txt\nDeletionDate=2026-01-01T00:00:00\n",
    )
    .unwrap();

    assert_eq!(fs::read_dir(&files_dir).unwrap().count(), 1);
    assert_eq!(fs::read_dir(&info_dir).unwrap().count(), 1);

    let items = list_trash().unwrap();
    assert_eq!(items.len(), 1);

    empty_trash().unwrap();

    assert_eq!(fs::read_dir(&files_dir).unwrap().count(), 0);
    assert_eq!(fs::read_dir(&info_dir).unwrap().count(), 0);
    assert!(list_trash().unwrap().is_empty());

    // SAFETY: still serialized by `TRASH_TEST_LOCK`.
    unsafe {
        match previous_xdg_data_home {
            Some(value) => std::env::set_var("XDG_DATA_HOME", value),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
    }
}

fn trash_root() -> PathBuf {
    std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(std::env::var("HOME").unwrap()).join(".local/share"))
        .join("Trash")
}

fn find_in_trash(original_path: &Path) -> Option<VeyraPath> {
    let root = trash_root();
    let info_dir = root.join("info");
    let wanted = format!("Path={}", original_path.to_string_lossy());

    for entry in fs::read_dir(info_dir).ok()?.filter_map(Result::ok) {
        let contents = fs::read_to_string(entry.path()).ok()?;
        if contents.lines().any(|line| line == wanted) {
            let trashinfo_name = entry.file_name();
            let trashinfo_name = trashinfo_name.to_string_lossy();
            let item_name = trashinfo_name.strip_suffix(".trashinfo")?;
            return Some(VeyraPath::from_local(root.join("files").join(item_name)));
        }
    }
    None
}
