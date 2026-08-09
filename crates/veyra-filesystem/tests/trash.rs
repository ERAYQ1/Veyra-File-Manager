use std::fs;
use std::path::{Path, PathBuf};

use veyra_filesystem::{delete, restore_from_trash, trash, VeyraPath};

/// Trashes a file, restores it, then permanently deletes it again — a full
/// round trip that leaves no residue in the real desktop trash can, while
/// still exercising the actual `g_file_trash` GIO call and the Trash-spec
/// based restore (not a mock of either).
#[test]
fn trash_then_restore_round_trip() {
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
