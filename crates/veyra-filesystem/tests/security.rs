//! Faz 29: end-to-end security-hardening tests (Rules #19-24) — path
//! traversal / Zip Slip across every archive format, symlink-cycle safety
//! in recursive directory counting, malicious filename rejection, and a
//! sanity check that argv-based process spawning never lets shell
//! metacharacters be reinterpreted.

use std::fs;
use std::io::Write;
use std::os::unix::fs::symlink;
use std::time::{Duration, Instant};

use veyra_filesystem::{count_dir_recursive, extract_archive, OperationControl, VeyraPath};

fn control() -> OperationControl {
    OperationControl::new()
}

// ---------------------------------------------------------------------
// A. Path Traversal / Zip Slip — every archive format must confine
// extraction to the destination directory, never escaping via `..` or an
// absolute path baked into an entry name.
// ---------------------------------------------------------------------

#[test]
fn zip_entry_with_absolute_path_is_confined() {
    let dir = tempfile::tempdir().unwrap();
    let archive_path = dir.path().join("abs.zip");
    {
        let file = fs::File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        writer.start_file("/etc/evil.txt", opts).unwrap();
        writer.write_all(b"pwned").unwrap();
        writer.finish().unwrap();
    }
    let dest = dir.path().join("out");

    let outcome = extract_archive(
        &VeyraPath::from_local(archive_path),
        &VeyraPath::from_local(dest.clone()),
        &control(),
        |_| {},
    )
    .unwrap();

    assert!(outcome.errors.is_empty());
    // Stripped of its leading slash and confined under `dest`, never
    // written to the real /etc.
    assert!(!std::path::Path::new("/etc/evil.txt").exists() || cfg!(windows));
    assert_eq!(fs::read(dest.join("etc/evil.txt")).unwrap(), b"pwned");
}

#[test]
fn tar_entry_with_parent_traversal_is_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let archive_path = dir.path().join("evil.tar");
    {
        let file = fs::File::create(&archive_path).unwrap();
        let mut builder = tar::Builder::new(file);
        let mut header = tar::Header::new_gnu();
        header.set_size(5);
        // `tar`'s own `set_path` refuses `..` components, so a real
        // malicious archive tool would write the raw name bytes directly
        // instead — done here the same way to prove Veyra's own
        // extraction-side check catches it regardless.
        let raw_name = b"../../../etc/evil.txt";
        header.as_old_mut().name[..raw_name.len()].copy_from_slice(raw_name);
        header.set_cksum();
        builder.append(&header, &b"pwned"[..]).unwrap();
        builder.into_inner().unwrap();
    }
    let dest = dir.path().join("nested/out");
    fs::create_dir_all(&dest).unwrap();

    let outcome = extract_archive(
        &VeyraPath::from_local(archive_path),
        &VeyraPath::from_local(dest.clone()),
        &control(),
        |_| {},
    )
    .unwrap();

    assert!(outcome.extracted.is_empty());
    assert_eq!(outcome.skipped.len(), 1);
    assert!(!dir.path().join("etc/evil.txt").exists());
}

#[test]
fn tar_symlink_entry_is_never_materialized() {
    let dir = tempfile::tempdir().unwrap();
    let archive_path = dir.path().join("symlink.tar");
    {
        let file = fs::File::create(&archive_path).unwrap();
        let mut builder = tar::Builder::new(file);
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_cksum();
        builder
            .append_link(&mut header, "evil_link", "/etc/passwd")
            .unwrap();
        builder.into_inner().unwrap();
    }
    let dest = dir.path().join("out");
    fs::create_dir_all(&dest).unwrap();

    let outcome = extract_archive(
        &VeyraPath::from_local(archive_path),
        &VeyraPath::from_local(dest.clone()),
        &control(),
        |_| {},
    )
    .unwrap();

    assert!(outcome.extracted.is_empty());
    assert!(!dest.join("evil_link").exists());
}

// ---------------------------------------------------------------------
// B. Symlink cycle / TOCTOU safety in recursive directory walks —
// `count_dir_recursive` must terminate quickly and must not descend into
// a symlinked directory, even a self-referential one.
// ---------------------------------------------------------------------

#[test]
fn recursive_count_does_not_follow_a_self_referential_symlink_cycle() {
    let dir = tempfile::tempdir().unwrap();
    let cyclic = dir.path().join("cyclic");
    fs::create_dir(&cyclic).unwrap();
    fs::write(cyclic.join("real.txt"), b"data").unwrap();
    // The trap: a symlink inside the directory that points right back at
    // its own parent, which would spin forever if it were ever followed.
    symlink(&cyclic, cyclic.join("self_link")).unwrap();

    let start = Instant::now();
    let count = count_dir_recursive(&VeyraPath::from_local(&cyclic), &control()).unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(5),
        "recursive count should terminate immediately, not spin on the symlink cycle"
    );
    // `self_link` is enumerated with NOFOLLOW_SYMLINKS, so it surfaces as a
    // symlink leaf (not a directory) and is never descended into: it's
    // counted alongside `real.txt` but contributes none of the cyclic
    // directory's own contents a second time.
    assert_eq!(count.file_count, 2); // real.txt + self_link (as a leaf)
    assert_eq!(count.dir_count, 0);
}

// ---------------------------------------------------------------------
// C. Malicious filenames — null-byte injection and overlong paths must be
// rejected without panicking.
// ---------------------------------------------------------------------

#[test]
fn null_byte_filename_is_rejected() {
    let result = veyra_core::security::validate_filename("innocuous\0.txt");
    assert_eq!(result, Err(veyra_core::security::FilenameError::NullByte));
}

#[test]
fn overlong_filename_is_rejected_without_panicking() {
    let long_name = "a".repeat(veyra_core::security::MAX_PATH_BYTES + 100);
    let result = veyra_core::security::validate_filename(&long_name);
    assert_eq!(result, Err(veyra_core::security::FilenameError::TooLong));
}

#[test]
fn bidi_override_is_detected_for_ui_warnings() {
    // "invoice.pdf" with a RTL override so it can be crafted to *display*
    // as something other than its real extension.
    let spoofed = "invoice\u{202E}fdp.exe";
    assert!(veyra_core::security::has_bidi_override(spoofed));
    assert!(!veyra_core::security::has_bidi_override("invoice.pdf"));
}

#[test]
fn invalid_and_overlong_archive_entry_names_are_skipped_not_panicking() {
    let dir = tempfile::tempdir().unwrap();
    let archive_path = dir.path().join("weird.zip");
    let long_name = format!("{}.txt", "a".repeat(5000));
    {
        let file = fs::File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        writer.start_file(long_name.as_str(), opts).unwrap();
        writer.write_all(b"x").unwrap();
        writer.finish().unwrap();
    }
    let dest = dir.path().join("out");

    let outcome = extract_archive(
        &VeyraPath::from_local(archive_path),
        &VeyraPath::from_local(dest.clone()),
        &control(),
        |_| {},
    )
    .unwrap();

    assert!(outcome.extracted.is_empty());
    assert_eq!(outcome.skipped.len(), 1);
}

// ---------------------------------------------------------------------
// D. Shell / command injection — argv-based spawning must treat a path
// containing shell metacharacters as one opaque argument, never letting a
// shell reinterpret it.
// ---------------------------------------------------------------------

#[test]
fn shell_metacharacters_in_a_path_argument_are_never_reinterpreted() {
    let dir = tempfile::tempdir().unwrap();
    let malicious_name = "file; touch pwned; echo".to_string();
    let malicious_path = dir.path().join(&malicious_name);
    fs::write(&malicious_path, b"x").unwrap();

    // `Command::new(bin).arg(path)` (the pattern every Veyra process-spawn
    // site uses — never `sh -c format!(...)`) passes the path as a single
    // argv element. Proving that here with `/bin/echo` demonstrates the
    // property Veyra's spawn call sites rely on: the semicolon is never
    // parsed as a command separator, so no `pwned` file gets created.
    let output = std::process::Command::new("echo")
        .arg(&malicious_path)
        .output()
        .expect("echo must be on PATH in the test environment");

    assert!(!dir.path().join("pwned").exists());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.trim().ends_with(&malicious_name));
}
