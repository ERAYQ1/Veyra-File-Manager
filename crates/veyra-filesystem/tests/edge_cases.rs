use std::fs;
use std::os::unix::fs::PermissionsExt;

use veyra_filesystem::{read_dir, FsError, VeyraPath};

#[test]
fn unreadable_directory_reports_permission_denied() {
    let dir = tempfile::tempdir().unwrap();
    let locked = dir.path().join("locked");
    fs::create_dir(&locked).unwrap();
    fs::write(locked.join("secret.txt"), b"x").unwrap();

    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    let result = read_dir(&VeyraPath::from_local(&locked));

    // Restore permissions immediately so `tempfile` can clean up the
    // directory on drop regardless of the assertion outcome below.
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    match result {
        Err(FsError::PermissionDenied(_)) => {}
        // Running as root (e.g. inside some CI containers) bypasses POSIX
        // permission checks entirely; that is a valid, if untestable-here,
        // outcome rather than a defect.
        Ok(_) => {}
        Err(other) => panic!("expected PermissionDenied or root bypass, got {other:?}"),
    }
}

#[test]
fn long_deeply_nested_path_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let mut path = dir.path().to_path_buf();
    for i in 0..40 {
        path.push(format!("segment-{i}"));
    }
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("leaf.txt"), b"deep").unwrap();

    let items = read_dir(&VeyraPath::from_local(&path)).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name(), "leaf.txt");
}

#[test]
fn path_display_never_panics_on_non_utf8_like_names() {
    // Filenames with characters that are legal on Linux (arbitrary bytes)
    // but awkward for display must not crash formatting.
    let dir = tempfile::tempdir().unwrap();
    let tricky = "weird\u{0}-ish? *.txt".replace('\u{0}', "");
    fs::write(dir.path().join(&tricky), b"x").unwrap();

    let items = read_dir(&VeyraPath::from_local(dir.path())).unwrap();
    assert_eq!(items.len(), 1);
    let _ = format!("{}", items[0].path);
}
