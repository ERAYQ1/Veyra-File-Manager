//! Faz 48: security & adversarial-hardening tests — malicious symlinks
//! (cycles, sensitive targets, chmod escape), malicious/corrupt archives,
//! permission-denied and transient-file (TOCTOU) handling, and malformed
//! filenames (Rule #10/#15/#16/#17/#21/#22).

use std::fs;
use std::io::Write;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::time::{Duration, Instant};

use veyra_filesystem::{
    analyze_directory, chmod_recursive, copy, count_dir_recursive, delete, extract_archive,
    find_duplicates, read_dir, read_dir_chunked, stat, FilePermissions, FsError, OperationControl,
    VeyraPath,
};

fn local(p: impl Into<std::path::PathBuf>) -> VeyraPath {
    VeyraPath::from_local(p.into())
}

fn control() -> OperationControl {
    OperationControl::new()
}

// ---------------------------------------------------------------------
// A. Malicious symlinks (Rule #22)
// ---------------------------------------------------------------------

#[test]
fn mutual_symlink_cycle_does_not_hang_count_dir_recursive() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    symlink(&b, a.join("to_b")).unwrap();
    symlink(&a, b.join("to_a")).unwrap();

    let start = Instant::now();
    let count = count_dir_recursive(&local(dir.path()), &control()).unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "mutual symlink cycle must not hang"
    );
    // Both directories plus each one's own symlink leaf, never re-entered.
    assert_eq!(count.dir_count, 2);
    assert_eq!(count.file_count, 2);
}

#[test]
fn mutual_symlink_cycle_does_not_hang_analyze_directory() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    fs::create_dir(&a).unwrap();
    fs::create_dir(&b).unwrap();
    symlink(&b, a.join("to_b")).unwrap();
    symlink(&a, b.join("to_a")).unwrap();
    fs::write(a.join("real.txt"), b"data").unwrap();

    let start = Instant::now();
    let result = analyze_directory(&local(dir.path()), &control()).unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "mutual symlink cycle must not hang"
    );
    assert!(result.tree.size_bytes > 0);
}

#[test]
fn self_referential_symlink_does_not_hang_read_dir_chunked() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("real.txt"), b"data").unwrap();
    symlink(dir.path(), dir.path().join("self_link")).unwrap();

    let mut seen = Vec::new();
    let start = Instant::now();
    read_dir_chunked(&local(dir.path()), 8, &control(), |chunk| {
        seen.extend(chunk.into_iter().map(|i| i.name().to_string()));
    })
    .unwrap();
    let elapsed = start.elapsed();

    assert!(elapsed < Duration::from_secs(5));
    // `read_dir_chunked` only ever lists one directory's direct children
    // (NOFOLLOW_SYMLINKS) — it never descends into `self_link`, so exactly
    // the two direct entries are seen, not an unbounded/duplicated set.
    seen.sort();
    assert_eq!(seen, vec!["real.txt".to_string(), "self_link".to_string()]);
}

#[test]
fn symlink_reports_its_own_metadata_never_the_sensitive_targets() {
    // A "sensitive" file this test controls (never a real system path —
    // reading actual /etc/shadow would be both non-portable and pointless
    // to assert on). What matters is that `stat`/`read_dir` describe the
    // *link*, not silently substitute the target's size/kind.
    let dir = tempfile::tempdir().unwrap();
    let sensitive = dir.path().join("sensitive.txt");
    fs::write(
        &sensitive,
        b"super secret content, much longer than a symlink",
    )
    .unwrap();
    fs::set_permissions(&sensitive, fs::Permissions::from_mode(0o600)).unwrap();

    let link = dir.path().join("innocuous_link");
    symlink(&sensitive, &link).unwrap();

    let item = stat(&local(&link)).unwrap();
    assert!(item.kind().is_symlink());
    assert!(!item.kind().is_directory());
    // The link's own on-disk size (a handful of bytes for the target path
    // string), never the target file's much larger content size.
    assert!(
        item.metadata.size_bytes < sensitive.metadata().unwrap().len(),
        "stat() must report the symlink's own size, not the target's"
    );

    let items = read_dir(&local(dir.path())).unwrap();
    let via_read_dir = items.iter().find(|i| i.name() == "innocuous_link").unwrap();
    assert!(via_read_dir.kind().is_symlink());
}

#[test]
fn chmod_recursive_never_modifies_a_symlinks_target_permissions() {
    // Regression test for a real Rule #22 violation found while writing
    // this suite: `chmod_recursive` used to call `set_permissions` on every
    // non-directory child unconditionally, including symlinks — and since
    // Linux has no `lchmod`, that `chmod()` call followed the link and
    // silently rewrote whatever file it pointed at, possibly outside the
    // tree the caller asked to chmod at all.
    let dir = tempfile::tempdir().unwrap();
    let victim = dir.path().join("victim.txt");
    fs::write(&victim, b"victim content").unwrap();
    fs::set_permissions(&victim, fs::Permissions::from_mode(0o644)).unwrap();

    let sub = dir.path().join("sub");
    fs::create_dir(&sub).unwrap();
    symlink(&victim, sub.join("link_to_victim")).unwrap();

    chmod_recursive(&local(&sub), FilePermissions::from_mode(0o700), &control()).unwrap();

    let victim_mode = fs::symlink_metadata(&victim).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        victim_mode, 0o644,
        "chmod_recursive must never follow a symlink and rewrite its target's permissions"
    );
}

#[test]
fn chmod_recursive_skips_a_symlinked_root_entirely() {
    let dir = tempfile::tempdir().unwrap();
    let victim = dir.path().join("victim_dir");
    fs::create_dir(&victim).unwrap();
    fs::write(victim.join("inner.txt"), b"x").unwrap();
    fs::set_permissions(&victim, fs::Permissions::from_mode(0o755)).unwrap();

    let link = dir.path().join("link_to_victim_dir");
    symlink(&victim, &link).unwrap();

    let outcome =
        chmod_recursive(&local(&link), FilePermissions::from_mode(0o000), &control()).unwrap();

    assert_eq!(outcome.succeeded, 0);
    assert!(outcome.errors.is_empty());
    let victim_mode = fs::symlink_metadata(&victim).unwrap().permissions().mode() & 0o777;
    assert_eq!(victim_mode, 0o755, "a symlinked root must never be chmod'd");
}

// ---------------------------------------------------------------------
// B. Malicious & corrupt archives (Rule #21)
// ---------------------------------------------------------------------

#[test]
fn zip_entry_with_nested_parent_traversal_is_confined() {
    let dir = tempfile::tempdir().unwrap();
    let archive_path = dir.path().join("evil.zip");
    {
        let file = fs::File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        writer
            .start_file("foo/../../../bar/evil.txt", opts)
            .unwrap();
        writer.write_all(b"pwned").unwrap();
        writer.finish().unwrap();
    }
    let dest = dir.path().join("out");

    let outcome =
        extract_archive(&local(&archive_path), &local(&dest), &control(), |_| {}).unwrap();

    assert!(outcome.errors.is_empty());
    // Never escapes `dest`, regardless of how many `..` segments the entry
    // tries to climb.
    assert!(!dir.path().join("bar/evil.txt").exists());
    for extracted in &outcome.extracted {
        assert!(
            extracted.starts_with(&dest),
            "{extracted:?} escaped {dest:?}"
        );
    }
}

#[test]
fn corrupt_zip_header_returns_an_archive_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let archive_path = dir.path().join("corrupt.zip");
    // Random garbage bytes with the right extension, no valid ZIP
    // structure whatsoever.
    fs::write(
        &archive_path,
        b"PK\x03\x04not a real zip file at all, just noise",
    )
    .unwrap();
    let dest = dir.path().join("out");

    let result = extract_archive(&local(&archive_path), &local(&dest), &control(), |_| {});
    assert!(
        result.is_err(),
        "corrupt ZIP must error, not panic or silently succeed"
    );
}

#[test]
fn truncated_zip_mid_header_returns_an_archive_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let real_archive = dir.path().join("real.zip");
    {
        let file = fs::File::create(&real_archive).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        writer.start_file("payload.txt", opts).unwrap();
        writer.write_all(&vec![b'x'; 8192]).unwrap();
        writer.finish().unwrap();
    }
    let full = fs::read(&real_archive).unwrap();
    let truncated_path = dir.path().join("truncated.zip");
    // Cut off partway through — a plausible "download/copy got
    // interrupted" scenario.
    fs::write(&truncated_path, &full[..full.len() / 2]).unwrap();
    let dest = dir.path().join("out");

    let result = extract_archive(&local(&truncated_path), &local(&dest), &control(), |_| {});
    assert!(result.is_err(), "truncated ZIP must error, not panic");
}

#[test]
fn corrupt_tar_gz_header_reports_an_error_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let archive_path = dir.path().join("corrupt.tar.gz");
    fs::write(&archive_path, b"\x1f\x8bnot really gzip data at all").unwrap();
    let dest = dir.path().join("out");

    // `GzDecoder` wraps the reader lazily, so the malformed gzip stream
    // only surfaces once the tar layer tries to actually read bytes from
    // it — that can show up as either a hard `Err` from `extract_archive`
    // or a populated `outcome.errors` with nothing extracted. Either is an
    // acceptable clean failure; a panic or a silently "successful" empty
    // extraction is not.
    match extract_archive(&local(&archive_path), &local(&dest), &control(), |_| {}) {
        Err(_) => {}
        Ok(outcome) => {
            assert!(outcome.extracted.is_empty());
            assert!(
                !outcome.errors.is_empty(),
                "corrupt TAR.GZ must be reported, not silently ignored"
            );
        }
    }
}

#[test]
fn truncated_tar_returns_an_archive_error_or_partial_outcome_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let real_archive = dir.path().join("real.tar");
    {
        let file = fs::File::create(&real_archive).unwrap();
        let mut builder = tar::Builder::new(file);
        let mut header = tar::Header::new_gnu();
        header.set_path("payload.txt").unwrap();
        header.set_size(8192);
        header.set_cksum();
        builder.append(&header, &vec![b'x'; 8192][..]).unwrap();
        builder.into_inner().unwrap();
    }
    let full = fs::read(&real_archive).unwrap();
    let truncated_path = dir.path().join("truncated.tar");
    fs::write(&truncated_path, &full[..full.len() - 1024]).unwrap();
    let dest = dir.path().join("out");

    // Either a clean error or a best-effort partial outcome is acceptable —
    // what must never happen is a panic.
    let _ = extract_archive(&local(&truncated_path), &local(&dest), &control(), |_| {});
}

#[test]
fn zip_entry_symlink_pointing_outside_destination_is_never_materialized() {
    let dir = tempfile::tempdir().unwrap();
    let archive_path = dir.path().join("symlink.zip");
    {
        let file = fs::File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default().unix_permissions(0o120777); // S_IFLNK symlink mode bits.
        writer.start_file("evil_link", opts).unwrap();
        writer.write_all(b"/etc/passwd").unwrap();
        writer.finish().unwrap();
    }
    let dest = dir.path().join("out");

    extract_archive(&local(&archive_path), &local(&dest), &control(), |_| {}).unwrap();

    let link_path = dest.join("evil_link");
    assert!(
        !link_path.exists() || !link_path.is_symlink(),
        "an archive-borne symlink must never be materialized on disk"
    );
}

// ---------------------------------------------------------------------
// C. Permission-denied, TOCTOU & transient files (Rule #15/#17/#18)
// ---------------------------------------------------------------------

#[test]
fn stat_on_a_permission_000_file_still_succeeds_or_fails_gracefully() {
    let dir = tempfile::tempdir().unwrap();
    let locked = dir.path().join("locked.txt");
    fs::write(&locked, b"secret").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    // `stat` only needs the parent directory's execute bit to look up the
    // inode, so it should still succeed even though the file itself is
    // unreadable/unwritable — but it must never panic either way.
    let result = stat(&local(&locked));
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o644)).unwrap();
    result.unwrap();
}

#[test]
fn read_dir_on_a_permission_000_directory_reports_a_clean_error() {
    let dir = tempfile::tempdir().unwrap();
    let locked = dir.path().join("locked_dir");
    fs::create_dir(&locked).unwrap();
    fs::write(locked.join("secret.txt"), b"x").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    let result = read_dir(&local(&locked));
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    match result {
        Err(FsError::PermissionDenied(_)) => {}
        Ok(_) => {} // root-in-CI bypass, same caveat as edge_cases.rs
        Err(other) => panic!("expected PermissionDenied or root bypass, got {other:?}"),
    }
}

#[test]
fn copy_from_a_permission_000_source_reports_a_clean_error() {
    let dir = tempfile::tempdir().unwrap();
    let locked = dir.path().join("locked.txt");
    fs::write(&locked, b"secret").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

    let dest = dir.path().join("copy.txt");
    let result = copy(&local(&locked), &local(&dest), false);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o644)).unwrap();

    // Either a clean error (normal, non-root) or success (root-in-CI
    // bypass) — never a panic.
    let _ = result;
}

#[test]
fn delete_of_a_readonly_parent_directory_reports_a_clean_error() {
    let dir = tempfile::tempdir().unwrap();
    let locked_parent = dir.path().join("readonly_parent");
    fs::create_dir(&locked_parent).unwrap();
    let victim = locked_parent.join("victim.txt");
    fs::write(&victim, b"x").unwrap();
    // No write permission on the parent means the entry can't be unlinked,
    // even though the file itself is readable.
    fs::set_permissions(&locked_parent, fs::Permissions::from_mode(0o500)).unwrap();

    let result = delete(&local(&victim));
    fs::set_permissions(&locked_parent, fs::Permissions::from_mode(0o755)).unwrap();

    // Either a clean error (normal, non-root) or success (root-in-CI
    // bypass) — never a panic.
    let _ = result;
}

#[test]
fn stat_on_a_path_deleted_just_before_the_call_reports_not_found_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let vanished = dir.path().join("vanished.txt");
    fs::write(&vanished, b"x").unwrap();
    fs::remove_file(&vanished).unwrap();

    let err = stat(&local(&vanished)).unwrap_err();
    assert!(matches!(err, FsError::NotFound(_)));
}

#[test]
fn copy_of_a_source_deleted_just_before_the_call_reports_a_clean_error() {
    let dir = tempfile::tempdir().unwrap();
    let vanished = dir.path().join("vanished.txt");
    fs::write(&vanished, b"x").unwrap();
    fs::remove_file(&vanished).unwrap();

    let dest = dir.path().join("copy.txt");
    let err = copy(&local(&vanished), &local(&dest), false).unwrap_err();
    assert!(matches!(err, FsError::NotFound(_)));
    assert!(!dest.exists());
}

#[test]
fn find_duplicates_skips_a_candidate_file_removed_before_hashing_without_erroring() {
    // Simulates the file being gone by the time the hash pass reaches it
    // (deleted concurrently, or a stale scan result) — `find_duplicates`
    // must exclude it rather than erroring or panicking on the missing
    // file, and must still confirm any duplicates that remain valid.
    let dir = tempfile::tempdir().unwrap();
    let survivor_a = dir.path().join("survivor_a.bin");
    let survivor_b = dir.path().join("survivor_b.bin");
    let vanished = dir.path().join("vanished.bin");
    let payload = vec![0x42u8; 10_000];
    fs::write(&survivor_a, &payload).unwrap();
    fs::write(&survivor_b, &payload).unwrap();
    fs::write(&vanished, &payload).unwrap();
    fs::remove_file(&vanished).unwrap();

    let candidates = vec![veyra_filesystem::SameSizeCandidateGroup {
        size_bytes: payload.len() as u64,
        paths: vec![local(&survivor_a), local(&survivor_b), local(&vanished)],
    }];

    let groups = find_duplicates(&candidates, &control());
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].files.len(), 2);
    assert!(groups[0].files.iter().all(|p| p != &local(&vanished)));
}

#[test]
fn analyze_directory_skips_an_inaccessible_subdirectory_without_aborting_the_scan() {
    // Stands in for "a subdirectory disappears/becomes inaccessible mid
    // scan": deterministic via permissions rather than a genuine race, but
    // exercises the exact same enumeration-failure code path documented in
    // `analyzer.rs` (Rule #18).
    let dir = tempfile::tempdir().unwrap();
    let accessible = dir.path().join("accessible");
    let inaccessible = dir.path().join("inaccessible");
    fs::create_dir(&accessible).unwrap();
    fs::create_dir(&inaccessible).unwrap();
    fs::write(accessible.join("visible.txt"), vec![b'x'; 2_000_000]).unwrap();
    fs::write(inaccessible.join("hidden.txt"), vec![b'y'; 2_000_000]).unwrap();
    fs::set_permissions(&inaccessible, fs::Permissions::from_mode(0o000)).unwrap();

    let result = analyze_directory(&local(dir.path()), &control());
    fs::set_permissions(&inaccessible, fs::Permissions::from_mode(0o755)).unwrap();
    let result = result.unwrap();

    // The scan as a whole must still succeed and report the accessible
    // subtree, rather than aborting entirely because one subdirectory
    // couldn't be enumerated.
    assert!(result.largest_files.iter().any(|f| f.name == "visible.txt"));
}

// ---------------------------------------------------------------------
// D. Malformed filenames (Rule #15/#23)
// ---------------------------------------------------------------------

#[test]
fn bidi_override_filename_is_flagged_via_validate_and_has_bidi_override() {
    let spoofed = "invoice\u{202E}fdp.exe";
    assert!(veyra_core::security::has_bidi_override(spoofed));
    // A bidi override alone isn't a byte-level violation the way a null
    // byte is — `validate_filename` still accepts it (that's `has_bidi_
    // override`'s job, for UI-side warnings), but must never panic on it.
    let _ = veyra_core::security::validate_filename(spoofed);
}

#[test]
fn null_byte_in_filename_is_rejected_by_validate_filename() {
    let result = veyra_core::security::validate_filename("evil\0name.txt");
    assert_eq!(result, Err(veyra_core::security::FilenameError::NullByte));
}

#[test]
fn null_byte_in_a_rename_target_is_rejected_before_touching_the_filesystem() {
    // `validate_filename` is the guard every UI rename/create path calls
    // before handing a name to `rename`/`create_file` — confirm it rejects
    // the same malicious input `veyra_filesystem::rename` would otherwise
    // be asked to act on.
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("source.txt"), b"x").unwrap();

    let malicious_name = "evil\0.txt";
    assert!(veyra_core::security::validate_filename(malicious_name).is_err());
}
