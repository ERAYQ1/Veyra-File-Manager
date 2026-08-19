//! Verifies packaging metadata (PKGBUILD, RPM specs, Debian changelog) stays
//! in sync with the workspace version, and that every filesystem path the
//! packaging files reference actually exists under `data/`.

use std::fs;
use std::path::{Path, PathBuf};

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
                    let value = rest.trim().trim_matches('"');
                    return value.to_string();
                }
            }
        }
    }
    panic!("workspace.package.version not found in Cargo.toml");
}

fn extract_field<'a>(contents: &'a str, key: &str) -> Option<&'a str> {
    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(key) {
            let rest = rest.trim_start();
            if let Some(rest) = rest.strip_prefix('=') {
                return Some(rest.trim().trim_matches('\'').trim_matches('"'));
            }
        }
    }
    None
}

/// RPM spec fields use `Key:   value` (no `=`), unlike shell/TOML assignments.
fn extract_rpm_field<'a>(contents: &'a str, key: &str) -> Option<&'a str> {
    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(key) {
            return Some(rest.trim());
        }
    }
    None
}

#[test]
fn pkgbuild_version_matches_workspace() {
    let root = workspace_root();
    let expected = workspace_version(&root);
    let pkgbuild = fs::read_to_string(root.join("packaging/arch/PKGBUILD")).expect("read PKGBUILD");
    let pkgver = extract_field(&pkgbuild, "pkgver").expect("pkgver field in PKGBUILD");
    assert_eq!(
        pkgver, expected,
        "PKGBUILD pkgver must match workspace version"
    );
}

#[test]
fn fedora_spec_version_matches_workspace() {
    let root = workspace_root();
    let expected = workspace_version(&root);
    let spec =
        fs::read_to_string(root.join("packaging/fedora/veyra.spec")).expect("read fedora spec");
    let version = extract_rpm_field(&spec, "Version:").expect("Version field in fedora spec");
    assert_eq!(
        version, expected,
        "Fedora spec Version must match workspace version"
    );
}

#[test]
fn opensuse_spec_version_matches_workspace() {
    let root = workspace_root();
    let expected = workspace_version(&root);
    let spec =
        fs::read_to_string(root.join("packaging/opensuse/veyra.spec")).expect("read opensuse spec");
    let version = extract_rpm_field(&spec, "Version:").expect("Version field in opensuse spec");
    assert_eq!(
        version, expected,
        "openSUSE spec Version must match workspace version"
    );
}

#[test]
fn debian_changelog_version_matches_workspace() {
    let root = workspace_root();
    let expected = workspace_version(&root);
    let changelog =
        fs::read_to_string(root.join("packaging/debian/changelog")).expect("read debian changelog");
    let first_line = changelog
        .lines()
        .next()
        .expect("changelog has a first line");
    // Format: "veyra (0.1.0-1) unstable; urgency=medium"
    let start = first_line
        .find('(')
        .expect("changelog version opening paren");
    let end = first_line
        .find(')')
        .expect("changelog version closing paren");
    let version_field = &first_line[start + 1..end];
    let upstream_version = version_field
        .split('-')
        .next()
        .expect("debian version has upstream component");
    assert_eq!(
        upstream_version, expected,
        "Debian changelog upstream version must match workspace version"
    );
}

#[test]
fn debian_control_syntax_is_well_formed() {
    let root = workspace_root();
    let control = fs::read_to_string(root.join("packaging/debian/control")).expect("read control");
    let mut paragraphs = control.split("\n\n");
    let source_para = paragraphs.next().expect("Source paragraph");
    assert!(source_para.contains("Source: veyra"));
    assert!(source_para.contains("Build-Depends:"));
    let package_para = paragraphs.next().expect("Package paragraph");
    assert!(package_para.contains("Package: veyra"));
    assert!(package_para.contains("Depends:"));
    assert!(package_para.contains("Description:"));
}

#[test]
fn packaging_files_reference_existing_data_paths() {
    let root = workspace_root();
    let data_paths = [
        "data/io.github.erayq1.Veyra.desktop",
        "data/io.github.erayq1.Veyra.metainfo.xml",
        "data/icons/hicolor/scalable/apps/io.github.erayq1.Veyra.svg",
    ];
    for path in data_paths {
        assert!(
            root.join(path).is_file(),
            "packaging references {path} but it does not exist"
        );
    }

    for spec_path in [
        "packaging/fedora/veyra.spec",
        "packaging/opensuse/veyra.spec",
    ] {
        let spec =
            fs::read_to_string(root.join(spec_path)).unwrap_or_else(|_| panic!("read {spec_path}"));
        assert!(spec.contains("io.github.erayq1.Veyra.desktop"));
        assert!(spec.contains("io.github.erayq1.Veyra.metainfo.xml"));
        assert!(spec.contains("io.github.erayq1.Veyra.svg"));
    }
}

#[test]
fn makefile_defines_standard_targets() {
    let root = workspace_root();
    let makefile = fs::read_to_string(root.join("Makefile")).expect("read Makefile");
    for target in ["all:", "build:", "install:", "uninstall:", "clean:"] {
        assert!(
            makefile.contains(target),
            "Makefile missing target {target}"
        );
    }
    assert!(makefile.contains("PREFIX ?= /usr"));
    assert!(makefile.contains("DESTDIR"));
}
