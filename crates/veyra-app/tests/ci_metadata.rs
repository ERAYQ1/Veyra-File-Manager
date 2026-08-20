//! Verifies the GitHub Actions workflows under `.github/workflows/` stay
//! structurally intact: required job names are present, the multi-distro
//! test matrix lists Ubuntu/Fedora/Arch with their expected packages, and
//! release/flatpak workflows trigger on the expected events.

use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

fn read_workflow(name: &str) -> String {
    let root = workspace_root();
    let path = root.join(".github/workflows").join(name);
    fs::read_to_string(&path).unwrap_or_else(|_| panic!("read {}", path.display()))
}

/// Minimal structural YAML check: every non-blank, non-comment line must
/// either be a list item, a `key:` / `key: value` mapping entry, or a
/// continuation of a block scalar. This catches gross syntax breakage
/// (tabs, mismatched indentation of mapping keys, stray characters)
/// without pulling in a YAML parsing dependency.
fn assert_well_formed_yaml(contents: &str, file: &str) {
    assert!(!contents.contains('\t'), "{file} must not contain tabs");
    for (idx, line) in contents.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let is_list_item = trimmed.starts_with("- ") || trimmed == "-";
        let candidate = if is_list_item {
            trimmed.trim_start_matches("- ")
        } else {
            trimmed
        };
        let looks_like_mapping = candidate.contains(':') || candidate.starts_with('|');
        assert!(
            is_list_item || looks_like_mapping || line.starts_with(' ') || line.starts_with('-'),
            "{file}:{} does not look like valid YAML: {line:?}",
            idx + 1
        );
    }
}

#[test]
fn ci_workflow_is_well_formed() {
    let contents = read_workflow("ci.yml");
    assert_well_formed_yaml(&contents, "ci.yml");
}

#[test]
fn release_workflow_is_well_formed() {
    let contents = read_workflow("release.yml");
    assert_well_formed_yaml(&contents, "release.yml");
}

#[test]
fn flatpak_workflow_is_well_formed() {
    let contents = read_workflow("flatpak.yml");
    assert_well_formed_yaml(&contents, "flatpak.yml");
}

#[test]
fn ci_workflow_declares_required_jobs() {
    let contents = read_workflow("ci.yml");
    for job in ["lint:", "test-matrix:", "build-release:"] {
        assert!(contents.contains(job), "ci.yml missing required job {job}");
    }
}

#[test]
fn ci_workflow_triggers_on_push_and_pull_request_to_main() {
    let contents = read_workflow("ci.yml");
    assert!(contents.contains("push:"));
    assert!(contents.contains("pull_request:"));
    assert!(contents.contains("branches: [main]"));
}

#[test]
fn ci_workflow_runs_fmt_and_clippy() {
    let contents = read_workflow("ci.yml");
    assert!(contents.contains("cargo fmt --all -- --check"));
    assert!(contents.contains("cargo clippy --workspace --all-targets -- -D warnings"));
}

#[test]
fn ci_workflow_covers_multi_distro_test_matrix() {
    let contents = read_workflow("ci.yml");
    assert!(
        contents.contains("fedora:latest"),
        "missing Fedora container"
    );
    assert!(
        contents.contains("archlinux:latest"),
        "missing Arch Linux container"
    );

    // Ubuntu dependencies.
    for pkg in ["libgtk-4-dev", "libadwaita-1-dev", "libglib2.0-dev"] {
        assert!(
            contents.contains(pkg),
            "ci.yml missing Ubuntu package {pkg}"
        );
    }
    // Fedora dependencies.
    for pkg in [
        "gtk4-devel",
        "libadwaita-devel",
        "glib2-devel",
        "gcc",
        "rust",
        "cargo",
    ] {
        assert!(
            contents.contains(pkg),
            "ci.yml missing Fedora package {pkg}"
        );
    }
    // Arch Linux dependencies.
    for pkg in ["gtk4", "libadwaita", "glib2", "base-devel", "rust", "cargo"] {
        assert!(contents.contains(pkg), "ci.yml missing Arch package {pkg}");
    }

    assert!(contents.contains("cargo test --workspace --verbose"));
}

#[test]
fn ci_workflow_verifies_release_build_and_install() {
    let contents = read_workflow("ci.yml");
    assert!(contents.contains("cargo build --release --locked"));
    assert!(contents.contains("make DESTDIR=staging PREFIX=/usr install"));
    assert!(contents.contains("desktop-file-validate"));
    assert!(contents.contains("appstreamcli validate"));
}

#[test]
fn release_workflow_triggers_on_version_tags() {
    let contents = read_workflow("release.yml");
    assert!(contents.contains("tags: ['v*']"));
}

#[test]
fn release_workflow_builds_strips_and_packages() {
    let contents = read_workflow("release.yml");
    assert!(contents.contains("cargo build --release --locked"));
    assert!(contents.contains("strip target/release/veyra"));
    assert!(contents.contains("tar -czf"));
    assert!(contents.contains("sha256sum"));
    assert!(contents.contains("action-gh-release"));
}

#[test]
fn flatpak_workflow_validates_manifest_files() {
    let contents = read_workflow("flatpak.yml");
    assert!(contents.contains("io.github.erayq1.Veyra.json"));
    assert!(contents.contains("cargo-sources.json"));
    assert!(contents.contains("json.tool"));
}

#[test]
fn referenced_flatpak_manifest_files_exist() {
    let root = workspace_root();
    for path in [
        "build-aux/flatpak/io.github.erayq1.Veyra.json",
        "build-aux/flatpak/cargo-sources.json",
    ] {
        assert!(
            root.join(path).is_file(),
            "{path} referenced by flatpak.yml but missing"
        );
    }
}

#[test]
fn workflows_directory_contains_expected_files() {
    let root = workspace_root();
    let workflows_dir = root.join(".github/workflows");
    for name in ["ci.yml", "release.yml", "flatpak.yml"] {
        assert!(
            workflows_dir.join(name).is_file(),
            "expected workflow file {name} to exist"
        );
    }
}
