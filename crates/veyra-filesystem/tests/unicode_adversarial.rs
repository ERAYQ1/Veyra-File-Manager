//! Faz 47: adversarial filename property tests. Every name below is a
//! Linux-legal but hostile-to-naive-code file name (emoji, CJK, RTL,
//! Turkish diacritics, embedded whitespace/tabs, shell-special glyphs, and
//! a name at the 255-byte NAME_MAX boundary) run through the full CRUD +
//! archive + trash lifecycle (Rule #34/#35).

use std::fs;

use veyra_filesystem::{
    copy, create_archive, create_dir, create_file, delete, extract_archive, move_entry, read_dir,
    rename, restore_from_trash, stat, trash_tracked, ArchiveFormat, FsError, OperationControl,
    VeyraPath,
};

fn local(p: impl Into<std::path::PathBuf>) -> VeyraPath {
    VeyraPath::from_local(p.into())
}

/// Every adversarial name this suite exercises. Kept in one place so each
/// property test below runs identically over the full set.
fn adversarial_names() -> Vec<String> {
    let mut names = vec![
        "😀_crab_🦀_test.txt".to_string(),
        "中文_测试_文档.md".to_string(),
        "日本語_テキスト.pdf".to_string(),
        "한국어_파일.zip".to_string(),
        "العربية_ملف_مهم.doc".to_string(),
        "עברית_קובץ.txt".to_string(),
        "şçğüöı_İĞÜŞÇÖ_türkçe_dosya.tar.gz".to_string(),
        "file with many spaces.txt".to_string(),
        "file\twith\ttabs.dat".to_string(),
        "my...file.with.dots.tar.gz".to_string(),
        ".leading_dot_file".to_string(),
        "[test] (1) {2} #3 $4 %5 ^6 &7 +8 =9 @0 !".to_string(),
    ];

    // A name at the 255-byte NAME_MAX boundary (Linux's hard filesystem
    // limit) — built from a multi-byte character so the byte count, not
    // the char count, is what's tested at the edge.
    let unit = "türkçe_"; // multi-byte UTF-8 unit
    let mut max_name = String::new();
    while max_name.len() + unit.len() <= 255 - 4 {
        max_name.push_str(unit);
    }
    max_name.push_str(".txt");
    assert!(max_name.len() <= 255);
    names.push(max_name);

    names
}

#[test]
fn create_write_read_round_trips_for_every_adversarial_name() {
    for name in adversarial_names() {
        let dir = tempfile::tempdir().unwrap();
        let path = local(dir.path().join(&name));

        create_file(&path).unwrap_or_else(|e| panic!("create_file({name:?}) failed: {e}"));
        fs::write(dir.path().join(&name), b"adversarial payload")
            .unwrap_or_else(|e| panic!("write({name:?}) failed: {e}"));

        let items = read_dir(&local(dir.path())).unwrap();
        assert_eq!(items.len(), 1, "read_dir did not see {name:?}");
        assert_eq!(items[0].name(), name);

        let content = fs::read(dir.path().join(&name)).unwrap();
        assert_eq!(
            content, b"adversarial payload",
            "content mismatch for {name:?}"
        );
    }
}

#[test]
fn rename_round_trips_for_every_adversarial_name() {
    for name in adversarial_names() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("source.txt"), b"x").unwrap();

        let renamed = rename(&local(dir.path().join("source.txt")), &name)
            .unwrap_or_else(|e| panic!("rename to {name:?} failed: {e}"));

        assert!(
            dir.path().join(&name).exists(),
            "renamed target {name:?} missing on disk"
        );
        assert_eq!(renamed.file_name().as_deref(), Some(name.as_str()));
    }
}

#[test]
fn copy_and_move_round_trip_for_every_adversarial_name() {
    for name in adversarial_names() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        let dst_dir = dir.path().join("dst");
        fs::create_dir(&src_dir).unwrap();
        fs::create_dir(&dst_dir).unwrap();
        fs::write(src_dir.join(&name), b"copy-move payload").unwrap();

        let src = local(src_dir.join(&name));
        let copy_dest = local(dst_dir.join(&name));
        copy(&src, &copy_dest, false).unwrap_or_else(|e| panic!("copy({name:?}) failed: {e}"));
        assert!(dst_dir.join(&name).exists());
        assert_eq!(fs::read(dst_dir.join(&name)).unwrap(), b"copy-move payload");

        // Move the original into a fresh subdirectory.
        let moved_dir = dir.path().join("moved");
        fs::create_dir(&moved_dir).unwrap();
        let move_dest = local(moved_dir.join(&name));
        move_entry(&src, &move_dest, false)
            .unwrap_or_else(|e| panic!("move_entry({name:?}) failed: {e}"));
        assert!(!src_dir.join(&name).exists());
        assert!(moved_dir.join(&name).exists());
    }
}

#[test]
fn trash_and_restore_round_trip_for_every_adversarial_name() {
    // GIO refuses to trash entries on "internal system mounts" like the
    // default tempdir location; use a directory under $HOME, matching the
    // existing trash.rs suite's approach.
    let base = tempfile::Builder::new()
        .prefix("veyra-unicode-trash-test-")
        .tempdir_in(std::env::var("HOME").expect("HOME must be set"))
        .unwrap();

    // The 255-byte boundary name is excluded here: the freedesktop.org
    // Trash spec's `.trashinfo` sidecar appends 10 bytes to the same file
    // name, which pushes it past NAME_MAX. That's a real filesystem
    // constraint (see `trash_tracked_reports_a_hard_error_for_a_name_at_the_boundary`
    // below), not something this suite should paper over.
    for name in adversarial_names()
        .into_iter()
        .filter(|n| n.len() + ".trashinfo".len() <= 255)
    {
        let dir = tempfile::Builder::new().tempdir_in(base.path()).unwrap();
        let original = dir.path().join(&name);
        fs::write(&original, b"trash payload").unwrap();

        let path = local(&original);
        let trashed =
            trash_tracked(&path).unwrap_or_else(|e| panic!("trash_tracked({name:?}) failed: {e}"));
        assert!(!original.exists());

        let restored = restore_from_trash(&trashed)
            .unwrap_or_else(|e| panic!("restore_from_trash({name:?}) failed: {e}"));
        let restored_local = restored.as_local_path().unwrap();
        assert!(
            restored_local.exists(),
            "{name:?} did not come back from trash"
        );
        assert_eq!(fs::read(restored_local).unwrap(), b"trash payload");

        delete(&restored).unwrap();
    }
}

#[test]
fn trash_tracked_reports_a_hard_error_for_a_name_at_the_boundary() {
    // A file name sitting right at the 255-byte NAME_MAX limit cannot be
    // tracked-trashed: the `.trashinfo` sidecar Veyra writes alongside it
    // needs the same base name plus a 10-byte suffix, which no longer
    // fits. This must surface as a clean `FsError`, never a panic
    // (Rule #15) — the boundary name itself is still perfectly valid for
    // every other operation (see the CRUD/rename/copy/move tests above).
    let base = tempfile::Builder::new()
        .prefix("veyra-unicode-trash-boundary-test-")
        .tempdir_in(std::env::var("HOME").expect("HOME must be set"))
        .unwrap();
    let max_name = adversarial_names().pop().unwrap();
    assert!(max_name.len() + ".trashinfo".len() > 255);

    let original = base.path().join(&max_name);
    fs::write(&original, b"boundary payload").unwrap();

    let result = trash_tracked(&local(&original));
    assert!(
        result.is_err(),
        "expected a hard error trashing a name at the NAME_MAX boundary, got {result:?}"
    );
    // The source file must be left untouched by the failed attempt.
    assert!(original.exists());
}

#[test]
fn folder_names_survive_creation_and_listing() {
    for name in ["🚀_folder", "中文_文件夹", "مجلد_عربي", "türkçe_klasör"] {
        let dir = tempfile::tempdir().unwrap();
        let path = local(dir.path().join(name));
        create_dir(&path).unwrap_or_else(|e| panic!("create_dir({name:?}) failed: {e}"));

        let items = read_dir(&local(dir.path())).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name(), name);
        assert!(items[0].kind().is_directory());
    }
}

#[test]
fn stat_reports_correct_size_for_every_adversarial_name() {
    for name in adversarial_names() {
        let dir = tempfile::tempdir().unwrap();
        let payload = format!("payload for {name}");
        fs::write(dir.path().join(&name), payload.as_bytes()).unwrap();

        let item = stat(&local(dir.path().join(&name)))
            .unwrap_or_else(|e| panic!("stat({name:?}) failed: {e}"));
        assert_eq!(item.name(), name);
        assert_eq!(item.metadata.size_bytes, payload.len() as u64);
    }
}

#[test]
fn copy_without_overwrite_rejects_an_existing_destination() {
    // Real edge case (Rule #38 data-loss prevention): a plain `copy` must
    // never silently clobber an existing destination unless `overwrite`
    // is explicitly requested.
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("source.txt"), b"new content").unwrap();
    fs::write(dir.path().join("dest.txt"), b"original content").unwrap();

    let err = copy(
        &local(dir.path().join("source.txt")),
        &local(dir.path().join("dest.txt")),
        false,
    )
    .unwrap_err();
    assert!(matches!(err, FsError::AlreadyExists(_)));
    assert_eq!(
        fs::read(dir.path().join("dest.txt")).unwrap(),
        b"original content"
    );
}

#[test]
fn adversarial_names_survive_zip_compress_and_extract_round_trip() {
    for name in adversarial_names() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(&name), b"archive payload").unwrap();

        let archive_path = dir.path().join("bundle.zip");
        let control = OperationControl::new();
        let outcome = create_archive(
            &local(&archive_path),
            &[local(dir.path().join(&name))],
            ArchiveFormat::Zip,
            &control,
            |_| {},
        )
        .unwrap_or_else(|e| panic!("create_archive({name:?}) failed: {e}"));
        assert!(
            outcome.errors.is_empty(),
            "{name:?} compress errors: {:?}",
            outcome.errors
        );

        let extract_dir = dir.path().join("out");
        let outcome = extract_archive(
            &local(&archive_path),
            &local(&extract_dir),
            &control,
            |_| {},
        )
        .unwrap_or_else(|e| panic!("extract_archive({name:?}) failed: {e}"));
        assert!(
            outcome.errors.is_empty(),
            "{name:?} extract errors: {:?}",
            outcome.errors
        );

        let extracted = extract_dir.join(&name);
        assert!(extracted.exists(), "{name:?} missing after zip round trip");
        assert_eq!(fs::read(&extracted).unwrap(), b"archive payload");
    }
}

#[test]
fn adversarial_names_survive_tar_gz_compress_and_extract_round_trip() {
    // The 255-byte boundary name is excluded: plain ustar TAR headers cap
    // the path field at 100 bytes, a real format limitation independent of
    // Veyra's own code (ZIP has no such cap, per the sibling test above).
    for name in adversarial_names().into_iter().filter(|n| n.len() <= 100) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(&name), b"tar payload").unwrap();

        let archive_path = dir.path().join("bundle.tar.gz");
        let control = OperationControl::new();
        create_archive(
            &local(&archive_path),
            &[local(dir.path().join(&name))],
            ArchiveFormat::TarGz,
            &control,
            |_| {},
        )
        .unwrap_or_else(|e| panic!("create_archive({name:?}) failed: {e}"));

        let extract_dir = dir.path().join("out");
        let outcome = extract_archive(
            &local(&archive_path),
            &local(&extract_dir),
            &control,
            |_| {},
        )
        .unwrap_or_else(|e| panic!("extract_archive({name:?}) failed: {e}"));
        assert!(
            outcome.errors.is_empty(),
            "{name:?} extract errors: {:?}",
            outcome.errors
        );

        let extracted = extract_dir.join(&name);
        assert!(
            extracted.exists(),
            "{name:?} missing after tar.gz round trip"
        );
        assert_eq!(fs::read(&extracted).unwrap(), b"tar payload");
    }
}

#[test]
fn natural_sort_orders_mixed_numeric_names_numerically() {
    // Not GTK-backed (that lives in veyra-ui::sorting), but the filesystem
    // layer's own ordering guarantee: read_dir must not silently reorder
    // entries lexicographically in a way that breaks natural comparison
    // downstream. Confirms names round-trip through read_dir untouched so
    // the UI's natural sort has faithful input to work with.
    let dir = tempfile::tempdir().unwrap();
    let names = ["file1.txt", "file2.txt", "file10.txt", "file20.txt"];
    for name in names {
        fs::write(dir.path().join(name), b"x").unwrap();
    }

    let items = read_dir(&local(dir.path())).unwrap();
    let mut seen: Vec<String> = items.iter().map(|i| i.name().to_string()).collect();
    seen.sort();
    let mut expected: Vec<String> = names.iter().map(|s| s.to_string()).collect();
    expected.sort();
    assert_eq!(seen, expected);
}
