#!/usr/bin/env bash
# Atomically bumps Veyra's SemVer across every packaging/config file that
# carries a version number, then optionally creates an annotated git tag.
#
# Usage:
#   scripts/bump_version.sh <new-version> [--notes "release description"] [--dry-run] [--tag]
#
# Example:
#   scripts/bump_version.sh 0.2.0 --notes "Faz 57: Release System" --tag

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

usage() {
    echo "Usage: $0 <new-version> [--notes \"text\"] [--dry-run] [--tag]" >&2
    exit 1
}

NEW_VERSION=""
NOTES="See CHANGELOG.md for details."
DRY_RUN=0
CREATE_TAG=0

if [[ $# -eq 0 ]]; then
    usage
fi
NEW_VERSION="$1"
shift

while [[ $# -gt 0 ]]; do
    case "$1" in
        --notes)
            NOTES="$2"
            shift 2
            ;;
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        --tag)
            CREATE_TAG=1
            shift
            ;;
        *)
            echo "error: unknown argument: $1" >&2
            usage
            ;;
    esac
done

SEMVER_RE='^[0-9]+\.[0-9]+\.[0-9]+$'
if [[ ! "$NEW_VERSION" =~ $SEMVER_RE ]]; then
    echo "error: '$NEW_VERSION' is not a valid SemVer MAJOR.MINOR.PATCH version (e.g. 0.2.0)" >&2
    exit 1
fi

CURRENT_VERSION="$(grep -m1 '^version = ' Cargo.toml | sed -E 's/version = "(.*)"/\1/')"
if [[ -z "$CURRENT_VERSION" ]]; then
    echo "error: could not read current version from Cargo.toml" >&2
    exit 1
fi

TODAY="$(date +%Y-%m-%d)"
DEB_DATE="$(date -R)"

echo "Bumping version: $CURRENT_VERSION -> $NEW_VERSION"

apply_edit() {
    local description="$1"
    shift
    if [[ "$DRY_RUN" -eq 1 ]]; then
        echo "[dry-run] would update: $description"
    else
        "$@"
        echo "updated: $description"
    fi
}

edit_cargo_toml() {
    sed -i -E "0,/^version = \"$CURRENT_VERSION\"$/s//version = \"$NEW_VERSION\"/" Cargo.toml
}

edit_metainfo() {
    local metainfo="data/io.github.erayq1.Veyra.metainfo.xml"
    local new_entry
    new_entry="    <release version=\"$NEW_VERSION\" date=\"$TODAY\">\n      <description>\n        <p>$NOTES</p>\n      </description>\n    </release>"
    sed -i "s|  <releases>|  <releases>\n${new_entry}|" "$metainfo"
}

edit_pkgbuild() {
    sed -i "s/^pkgver=.*/pkgver=$NEW_VERSION/" packaging/arch/PKGBUILD
}

edit_fedora_spec() {
    sed -i "s/^Version:.*/Version:        $NEW_VERSION/" packaging/fedora/veyra.spec
}

edit_opensuse_spec() {
    sed -i "s/^Version:.*/Version:        $NEW_VERSION/" packaging/opensuse/veyra.spec
}

edit_debian_changelog() {
    local changelog="packaging/debian/changelog"
    local entry
    entry="veyra ($NEW_VERSION-1) unstable; urgency=medium\n\n  * $NOTES\n\n -- Veyra Contributors <okuslug33@gmail.com>  $DEB_DATE\n"
    printf '%b\n' "$entry" | cat - "$changelog" > "$changelog.tmp"
    mv "$changelog.tmp" "$changelog"
}

apply_edit "Cargo.toml [workspace.package] version" edit_cargo_toml
apply_edit "data/io.github.erayq1.Veyra.metainfo.xml <releases>" edit_metainfo
apply_edit "packaging/arch/PKGBUILD pkgver" edit_pkgbuild
apply_edit "packaging/fedora/veyra.spec Version" edit_fedora_spec
apply_edit "packaging/opensuse/veyra.spec Version" edit_opensuse_spec
apply_edit "packaging/debian/changelog new entry" edit_debian_changelog

if [[ "$DRY_RUN" -eq 1 ]]; then
    echo "[dry-run] no files were modified."
    exit 0
fi

echo "Version bumped to $NEW_VERSION in all packaging files."

if [[ "$CREATE_TAG" -eq 1 ]]; then
    git tag -a "v$NEW_VERSION" -m "Release v$NEW_VERSION"
    echo "created annotated tag v$NEW_VERSION"
else
    echo "run 'git tag -a v$NEW_VERSION -m \"Release v$NEW_VERSION\"' to tag this release, or re-run with --tag"
fi
