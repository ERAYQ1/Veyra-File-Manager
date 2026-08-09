use std::fs;
use std::os::unix::fs::symlink;

use veyra_filesystem::{read_dir, FileKind, VeyraPath};

#[test]
fn valid_symlink_is_reported_not_broken() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("target.txt"), b"x").unwrap();
    symlink(dir.path().join("target.txt"), dir.path().join("link.txt")).unwrap();

    let items = read_dir(&VeyraPath::from_local(dir.path())).unwrap();
    let link = items.iter().find(|i| i.name() == "link.txt").unwrap();

    match link.kind() {
        FileKind::Symlink { is_broken, .. } => assert!(!is_broken),
        other => panic!("expected Symlink, got {other:?}"),
    }
    assert!(!link.kind().is_broken_symlink());
}

#[test]
fn broken_symlink_never_panics_and_is_flagged() {
    let dir = tempfile::tempdir().unwrap();
    symlink(
        dir.path().join("does-not-exist"),
        dir.path().join("dangling.txt"),
    )
    .unwrap();

    let items = read_dir(&VeyraPath::from_local(dir.path())).unwrap();
    let link = items.iter().find(|i| i.name() == "dangling.txt").unwrap();

    match link.kind() {
        FileKind::Symlink { is_broken, .. } => assert!(is_broken),
        other => panic!("expected Symlink, got {other:?}"),
    }
    assert!(link.kind().is_broken_symlink());
}

#[test]
fn symlink_to_directory_is_not_treated_as_directory() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("real_dir")).unwrap();
    symlink(dir.path().join("real_dir"), dir.path().join("dir_link")).unwrap();

    let items = read_dir(&VeyraPath::from_local(dir.path())).unwrap();
    let link = items.iter().find(|i| i.name() == "dir_link").unwrap();

    // Must remain classified as a symlink (never silently followed), even
    // though its target is a directory.
    assert!(link.kind().is_symlink());
    assert!(!link.kind().is_directory());
}
