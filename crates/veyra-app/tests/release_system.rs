//! Verifies the Faz 57 release management system: SemVer parsing rules,
//! `scripts/generate_release_notes.py` Conventional Commits categorization,
//! and that every packaging file carries the same synchronized version
//! number as the workspace.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

fn workspace_version(root: &Path) -> String {
    let cargo_toml = fs::read_to_string(root.join("Cargo.toml")).expect("read root Cargo.toml");
    let mut in_workspace_package = false;
    for line in cargo_toml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_workspace_package = trimmed == "[workspace.package]";
            continue;
        }
        if in_workspace_package {
            if let Some(rest) = trimmed.strip_prefix("version") {
                let rest = rest.trim_start();
                if let Some(rest) = rest.strip_prefix('=') {
                    return rest.trim().trim_matches('"').to_string();
                }
            }
        }
    }
    panic!("workspace.package.version not found in Cargo.toml");
}

fn extract_rpm_field<'a>(contents: &'a str, key: &str) -> Option<&'a str> {
    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(key) {
            return Some(rest.trim());
        }
    }
    None
}

/// Minimal SemVer `MAJOR.MINOR.PATCH` validator mirroring the regex used by
/// `scripts/bump_version.sh` (`^[0-9]+\.[0-9]+\.[0-9]+$`).
fn is_valid_semver(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
}

#[test]
fn semver_validator_accepts_well_formed_versions() {
    for version in ["0.1.0", "0.2.0", "1.0.0", "10.20.30"] {
        assert!(is_valid_semver(version), "{version} should be valid SemVer");
    }
}

#[test]
fn semver_validator_rejects_malformed_versions() {
    for version in [
        "1.0", "1.0.0.0", "v1.0.0", "1.0.x", "abc", "", "1..0", "-1.0.0",
    ] {
        assert!(
            !is_valid_semver(version),
            "{version} should be rejected as invalid SemVer"
        );
    }
}

#[test]
fn bump_version_script_rejects_invalid_semver() {
    let root = workspace_root();
    let script = root.join("scripts/bump_version.sh");
    let output = Command::new("bash")
        .arg(&script)
        .arg("not-a-version")
        .arg("--dry-run")
        .current_dir(&root)
        .output()
        .expect("run bump_version.sh");
    assert!(
        !output.status.success(),
        "bump_version.sh must reject an invalid SemVer argument"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not a valid SemVer"));
}

#[test]
fn bump_version_script_dry_run_does_not_modify_files() {
    let root = workspace_root();
    let script = root.join("scripts/bump_version.sh");
    let before = workspace_version(&root);

    let output = Command::new("bash")
        .arg(&script)
        .arg("9.9.9")
        .arg("--dry-run")
        .current_dir(&root)
        .output()
        .expect("run bump_version.sh --dry-run");
    assert!(output.status.success(), "dry-run should succeed");

    let after = workspace_version(&root);
    assert_eq!(before, after, "dry-run must not modify Cargo.toml");
}

#[test]
fn release_notes_script_categorizes_conventional_commits() {
    let root = workspace_root();
    let script = root.join("scripts/generate_release_notes.py");

    let commits_file = std::env::temp_dir().join("veyra_release_notes_test_commits.txt");
    fs::write(
        &commits_file,
        "feat(search): add fuzzy matching\n\
         perf: cut scan time in half\n\
         sec: mask credentials in logs\n\
         security: harden polkit policy\n\
         privacy: strip exif on export\n\
         pkg(flatpak): bump runtime to 47\n\
         packaging: add opensuse spec\n\
         test: add adversarial unicode cases\n\
         fix: crash on empty selection\n\
         docs: update README\n",
    )
    .expect("write temp commits file");

    let output = Command::new("python3")
        .arg(&script)
        .arg("--version")
        .arg("0.2.0")
        .arg("--commits-file")
        .arg(&commits_file)
        .output()
        .expect("run generate_release_notes.py");

    let _ = fs::remove_file(&commits_file);

    assert!(
        output.status.success(),
        "generate_release_notes.py failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("🚀 New Features"));
    assert!(stdout.contains("add fuzzy matching"));
    assert!(stdout.contains("⚡ Performance & Scale"));
    assert!(stdout.contains("cut scan time in half"));
    assert!(stdout.contains("🛡️ Security & Privacy"));
    assert!(stdout.contains("mask credentials in logs"));
    assert!(stdout.contains("harden polkit policy"));
    assert!(stdout.contains("strip exif on export"));
    assert!(stdout.contains("📦 Packaging & Distros"));
    assert!(stdout.contains("bump runtime to 47"));
    assert!(stdout.contains("add opensuse spec"));
    assert!(stdout.contains("🧪 Testing & Quality"));
    assert!(stdout.contains("add adversarial unicode cases"));
    assert!(stdout.contains("🐛 Bug Fixes"));
    assert!(stdout.contains("crash on empty selection"));
    assert!(stdout.contains("🔧 Other Changes"));
    assert!(stdout.contains("update README"));
}

#[test]
fn release_notes_script_requires_a_commit_source() {
    let root = workspace_root();
    let script = root.join("scripts/generate_release_notes.py");
    let output = Command::new("python3")
        .arg(&script)
        .arg("--version")
        .arg("0.2.0")
        .output()
        .expect("run generate_release_notes.py without a source");
    assert!(!output.status.success());
}

#[test]
fn all_packaging_versions_are_synchronized_with_workspace() {
    let root = workspace_root();
    let expected = workspace_version(&root);

    let pkgbuild = fs::read_to_string(root.join("packaging/arch/PKGBUILD")).expect("PKGBUILD");
    let pkgver = pkgbuild
        .lines()
        .find_map(|l| l.strip_prefix("pkgver="))
        .expect("pkgver field");
    assert_eq!(pkgver, expected);

    let fedora_spec =
        fs::read_to_string(root.join("packaging/fedora/veyra.spec")).expect("fedora spec");
    assert_eq!(
        extract_rpm_field(&fedora_spec, "Version:").expect("fedora Version field"),
        expected
    );

    let opensuse_spec =
        fs::read_to_string(root.join("packaging/opensuse/veyra.spec")).expect("opensuse spec");
    assert_eq!(
        extract_rpm_field(&opensuse_spec, "Version:").expect("opensuse Version field"),
        expected
    );

    let deb_changelog =
        fs::read_to_string(root.join("packaging/debian/changelog")).expect("debian changelog");
    let first_line = deb_changelog.lines().next().expect("first line");
    let start = first_line.find('(').expect("opening paren");
    let end = first_line.find(')').expect("closing paren");
    let upstream_version = first_line[start + 1..end]
        .split('-')
        .next()
        .expect("upstream version component");
    assert_eq!(upstream_version, expected);

    let metainfo = fs::read_to_string(root.join("data/io.github.erayq1.Veyra.metainfo.xml"))
        .expect("metainfo.xml");
    let latest_release_version = metainfo
        .lines()
        .find_map(|l| {
            let trimmed = l.trim();
            trimmed
                .strip_prefix("<release version=\"")
                .and_then(|rest| rest.split('"').next())
        })
        .expect("first <release version=\"...\"> entry in metainfo.xml");
    assert_eq!(
        latest_release_version, expected,
        "metainfo.xml's newest <release> entry must match the workspace version"
    );
}
