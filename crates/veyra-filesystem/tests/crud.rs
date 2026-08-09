use std::fs;

use veyra_filesystem::{
    copy, create_dir, create_file, delete, move_entry, read_dir, rename, FsError, VeyraPath,
};

fn local(p: impl Into<std::path::PathBuf>) -> VeyraPath {
    VeyraPath::from_local(p.into())
}

#[test]
fn create_file_then_fails_on_duplicate() {
    let dir = tempfile::tempdir().unwrap();
    let path = local(dir.path().join("new.txt"));

    create_file(&path).unwrap();
    assert!(dir.path().join("new.txt").is_file());

    let err = create_file(&path).unwrap_err();
    assert!(matches!(err, FsError::AlreadyExists(_)));
}

#[test]
fn create_dir_then_fails_on_duplicate() {
    let dir = tempfile::tempdir().unwrap();
    let path = local(dir.path().join("subdir"));

    create_dir(&path).unwrap();
    assert!(dir.path().join("subdir").is_dir());

    let err = create_dir(&path).unwrap_err();
    assert!(matches!(err, FsError::AlreadyExists(_)));
}

#[test]
fn rename_moves_entry_within_same_directory() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("old.txt"), b"content").unwrap();

    let renamed = rename(&local(dir.path().join("old.txt")), "new.txt").unwrap();

    assert!(!dir.path().join("old.txt").exists());
    assert!(dir.path().join("new.txt").exists());
    assert_eq!(renamed.file_name().as_deref(), Some("new.txt"));
}

#[test]
fn rename_supports_unicode_target_name() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), b"x").unwrap();

    let renamed = rename(&local(dir.path().join("a.txt")), "başlık türkçe.txt").unwrap();

    assert!(dir.path().join("başlık türkçe.txt").exists());
    assert_eq!(renamed.file_name().as_deref(), Some("başlık türkçe.txt"));
}

#[test]
fn copy_duplicates_file_content() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("source.txt"), b"payload").unwrap();

    copy(
        &local(dir.path().join("source.txt")),
        &local(dir.path().join("dest.txt")),
        false,
    )
    .unwrap();

    assert!(dir.path().join("source.txt").exists());
    assert_eq!(fs::read(dir.path().join("dest.txt")).unwrap(), b"payload");
}

#[test]
fn copy_without_overwrite_fails_when_destination_exists() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("source.txt"), b"a").unwrap();
    fs::write(dir.path().join("dest.txt"), b"b").unwrap();

    let err = copy(
        &local(dir.path().join("source.txt")),
        &local(dir.path().join("dest.txt")),
        false,
    )
    .unwrap_err();

    assert!(matches!(err, FsError::AlreadyExists(_)));
}

#[test]
fn copy_with_overwrite_replaces_destination() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("source.txt"), b"new").unwrap();
    fs::write(dir.path().join("dest.txt"), b"old").unwrap();

    copy(
        &local(dir.path().join("source.txt")),
        &local(dir.path().join("dest.txt")),
        true,
    )
    .unwrap();

    assert_eq!(fs::read(dir.path().join("dest.txt")).unwrap(), b"new");
}

#[test]
fn move_entry_relocates_file() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("dest_dir")).unwrap();
    fs::write(dir.path().join("source.txt"), b"payload").unwrap();

    move_entry(
        &local(dir.path().join("source.txt")),
        &local(dir.path().join("dest_dir/source.txt")),
        false,
    )
    .unwrap();

    assert!(!dir.path().join("source.txt").exists());
    assert!(dir.path().join("dest_dir/source.txt").exists());
}

#[test]
fn delete_removes_regular_file() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("file.txt"), b"x").unwrap();

    delete(&local(dir.path().join("file.txt"))).unwrap();

    assert!(!dir.path().join("file.txt").exists());
}

#[test]
fn delete_removes_nested_directory_tree() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("a/b/c")).unwrap();
    fs::write(dir.path().join("a/root.txt"), b"1").unwrap();
    fs::write(dir.path().join("a/b/mid.txt"), b"2").unwrap();
    fs::write(dir.path().join("a/b/c/leaf.txt"), b"3").unwrap();

    delete(&local(dir.path().join("a"))).unwrap();

    assert!(!dir.path().join("a").exists());
}

#[test]
fn delete_nonexistent_path_returns_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let err = delete(&local(dir.path().join("ghost.txt"))).unwrap_err();
    assert!(matches!(err, FsError::NotFound(_)));
}

#[test]
fn transient_file_deleted_mid_read_is_reported_not_panicked() {
    // Regression guard for Rule #17/#20: a file that disappears between
    // discovery and operation must surface as a typed error, never a panic.
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("here_now.txt"), b"x").unwrap();

    let items = read_dir(&local(dir.path())).unwrap();
    assert_eq!(items.len(), 1);

    fs::remove_file(dir.path().join("here_now.txt")).unwrap();

    let err = delete(&items[0].path).unwrap_err();
    assert!(matches!(err, FsError::NotFound(_)));
}
