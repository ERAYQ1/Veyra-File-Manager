use std::fs;

use veyra_filesystem::{read_dir, FileKind, VeyraPath};

fn path_of(dir: &tempfile::TempDir) -> VeyraPath {
    VeyraPath::from_local(dir.path())
}

#[test]
fn lists_regular_files_and_directories() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), b"hello").unwrap();
    fs::create_dir(dir.path().join("subdir")).unwrap();

    let mut items = read_dir(&path_of(&dir)).unwrap();
    items.sort_by(|a, b| a.name().cmp(b.name()));

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].name(), "a.txt");
    assert_eq!(items[0].kind(), &FileKind::Regular);
    assert_eq!(items[0].metadata.size_bytes, 5);
    assert_eq!(items[1].name(), "subdir");
    assert!(items[1].kind().is_directory());
}

#[test]
fn handles_unicode_and_space_and_special_characters() {
    let dir = tempfile::tempdir().unwrap();
    let names = [
        "türkçe dosya adı.txt",
        "emoji 🚀 file.txt",
        "with spaces and (parens).txt",
        "日本語のファイル.txt",
    ];
    for name in names {
        fs::write(dir.path().join(name), b"x").unwrap();
    }

    let items = read_dir(&path_of(&dir)).unwrap();
    assert_eq!(items.len(), names.len());
    for name in names {
        assert!(
            items.iter().any(|item| item.name() == name),
            "missing entry for {name:?}"
        );
    }
}

#[test]
fn detects_hidden_files_via_gio() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(".hidden_file"), b"x").unwrap();
    fs::write(dir.path().join("visible_file"), b"x").unwrap();

    let items = read_dir(&path_of(&dir)).unwrap();
    let hidden = items.iter().find(|i| i.name() == ".hidden_file").unwrap();
    let visible = items.iter().find(|i| i.name() == "visible_file").unwrap();

    assert!(hidden.metadata.is_hidden);
    assert!(!visible.metadata.is_hidden);
}

#[test]
fn reports_executable_flag_from_permission_bits() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("run.sh");
    fs::write(&script, b"#!/bin/sh\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

    let items = read_dir(&path_of(&dir)).unwrap();
    let entry = items.iter().find(|i| i.name() == "run.sh").unwrap();

    assert!(entry.metadata.is_executable());
    assert_eq!(
        entry.metadata.permissions.unwrap().symbolic_string(),
        "rwxr-xr-x"
    );
}

#[test]
fn empty_directory_yields_empty_list() {
    let dir = tempfile::tempdir().unwrap();
    let items = read_dir(&path_of(&dir)).unwrap();
    assert!(items.is_empty());
}

#[test]
fn nonexistent_directory_returns_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let missing = VeyraPath::from_local(dir.path().join("does-not-exist"));
    let err = read_dir(&missing).unwrap_err();
    assert!(matches!(err, veyra_filesystem::FsError::NotFound(_)));
}
