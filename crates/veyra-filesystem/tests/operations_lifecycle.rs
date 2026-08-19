//! Faz 47: end-to-end operation chain — create, write, checksum, move,
//! rename, compress, extract, verify, trash, restore, permanently delete —
//! run as one continuous pipeline so a regression that only shows up when
//! operations compose (not in isolation) gets caught (Rule #34/#38).

use std::fs;

use sha2::{Digest, Sha256};
use veyra_filesystem::{
    create_archive, create_dir, create_file, delete, extract_archive, rename, restore_from_trash,
    trash_tracked, ArchiveFormat, OperationControl, VeyraPath,
};

fn local(p: impl Into<std::path::PathBuf>) -> VeyraPath {
    VeyraPath::from_local(p.into())
}

fn sha256_of(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Minimal local hex encoder so this test doesn't pull in a `hex` crate
/// dependency just for a checksum comparison.
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[test]
fn full_lifecycle_create_to_permanent_delete_preserves_data_at_every_step() {
    // GIO refuses to trash entries on "internal system mounts" like the
    // default tempdir location; use a directory under $HOME (matches the
    // existing trash.rs suite).
    let base = tempfile::Builder::new()
        .prefix("veyra-lifecycle-test-")
        .tempdir_in(std::env::var("HOME").expect("HOME must be set"))
        .unwrap();
    let root = base.path();

    // 1. Create a file, write content.
    let original = root.join("report.txt");
    create_file(&local(&original)).unwrap();
    let payload = b"Veyra Faz 47 lifecycle payload \xF0\x9F\xA6\x80".to_vec();
    fs::write(&original, &payload).unwrap();

    // 2. Compute a SHA-256 checksum to compare against at the end.
    let original_hash = sha256_of(&payload);

    // 3. Move into a subdirectory.
    let subdir = root.join("archive_src");
    create_dir(&local(&subdir)).unwrap();
    let moved = subdir.join("report.txt");
    veyra_filesystem::move_entry(&local(&original), &local(&moved), false).unwrap();
    assert!(!original.exists());
    assert_eq!(fs::read(&moved).unwrap(), payload);

    // 4. Rename.
    let renamed_path = rename(&local(&moved), "final_report.txt").unwrap();
    let renamed_local = renamed_path.as_local_path().unwrap().to_path_buf();
    assert!(renamed_local.ends_with("final_report.txt"));
    assert_eq!(fs::read(&renamed_local).unwrap(), payload);

    // 5. Compress into a zip archive.
    let archive_path = root.join("bundle.zip");
    let control = OperationControl::new();
    let outcome = create_archive(
        &local(&archive_path),
        &[local(&renamed_local)],
        ArchiveFormat::Zip,
        &control,
        |_progress| {},
    )
    .unwrap();
    assert!(
        outcome.errors.is_empty(),
        "compress errors: {:?}",
        outcome.errors
    );
    assert!(archive_path.exists());

    // 6. Extract the archive into a different directory.
    let extract_dir = root.join("extracted");
    let outcome = extract_archive(
        &local(&archive_path),
        &local(&extract_dir),
        &control,
        |_| {},
    )
    .unwrap();
    assert!(
        outcome.errors.is_empty(),
        "extract errors: {:?}",
        outcome.errors
    );
    assert!(!outcome.extracted.is_empty());

    // 7. Verify the extracted file's hash matches the original payload.
    let extracted_file = outcome
        .extracted
        .iter()
        .find(|p| p.file_name().and_then(|n| n.to_str()) == Some("final_report.txt"))
        .unwrap_or_else(|| {
            panic!(
                "final_report.txt missing from extracted set: {:?}",
                outcome.extracted
            )
        });
    let extracted_bytes = fs::read(extracted_file).unwrap();
    assert_eq!(sha256_of(&extracted_bytes), original_hash);

    // 8. Trash the extracted file (tracked, so we get back exactly where it
    //    landed for the Undo step).
    let trashed = trash_tracked(&local(extracted_file)).unwrap();
    assert!(!extracted_file.exists());

    // 9. Undo: restore it from trash.
    let restored = restore_from_trash(&trashed).unwrap();
    let restored_local = restored.as_local_path().unwrap();
    assert!(restored_local.exists());
    assert_eq!(sha256_of(&fs::read(restored_local).unwrap()), original_hash);

    // 10. Permanently delete.
    delete(&restored).unwrap();
    assert!(!restored_local.exists());
}

#[test]
fn tar_gz_round_trip_preserves_directory_structure_and_content() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("project");
    fs::create_dir_all(src.join("nested")).unwrap();
    fs::write(src.join("top.txt"), b"top level").unwrap();
    fs::write(src.join("nested/inner.txt"), b"nested content").unwrap();

    let archive_path = dir.path().join("project.tar.gz");
    let control = OperationControl::new();
    create_archive(
        &local(&archive_path),
        &[local(&src)],
        ArchiveFormat::TarGz,
        &control,
        |_| {},
    )
    .unwrap();

    let extract_dir = dir.path().join("out");
    let outcome = extract_archive(
        &local(&archive_path),
        &local(&extract_dir),
        &control,
        |_| {},
    )
    .unwrap();
    assert!(outcome.errors.is_empty());

    let nested = extract_dir.join("project/nested/inner.txt");
    assert_eq!(fs::read(&nested).unwrap(), b"nested content");
    let top = extract_dir.join("project/top.txt");
    assert_eq!(fs::read(&top).unwrap(), b"top level");
}

/// Every remaining supported archive format gets its own full compress ->
/// extract -> checksum round trip, so a format-specific regression (e.g. in
/// `xz2`/`zstd`/`sevenz-rust` bindings) can't hide behind the ZIP/TAR.GZ
/// coverage above.
#[test]
fn every_remaining_archive_format_round_trips_content_and_checksum() {
    for format in [
        ArchiveFormat::Tar,
        ArchiveFormat::TarXz,
        ArchiveFormat::TarZst,
        ArchiveFormat::SevenZip,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let payload = b"format round trip payload".to_vec();
        let source = dir.path().join("payload.bin");
        fs::write(&source, &payload).unwrap();
        let expected_hash = sha256_of(&payload);

        let archive_path = dir.path().join(format!("bundle{}", format.extension()));
        let control = OperationControl::new();
        create_archive(
            &local(&archive_path),
            &[local(&source)],
            format,
            &control,
            |_| {},
        )
        .unwrap_or_else(|e| panic!("{format:?} create_archive failed: {e}"));
        assert!(archive_path.exists(), "{format:?} archive was not written");

        let extract_dir = dir.path().join("out");
        let outcome = extract_archive(
            &local(&archive_path),
            &local(&extract_dir),
            &control,
            |_| {},
        )
        .unwrap_or_else(|e| panic!("{format:?} extract_archive failed: {e}"));
        assert!(
            outcome.errors.is_empty(),
            "{format:?} extract errors: {:?}",
            outcome.errors
        );

        let extracted = extract_dir.join("payload.bin");
        let extracted_bytes = fs::read(&extracted)
            .unwrap_or_else(|e| panic!("{format:?} extracted file missing: {e}"));
        assert_eq!(
            sha256_of(&extracted_bytes),
            expected_hash,
            "{format:?} round-tripped content does not match checksum"
        );
    }
}
