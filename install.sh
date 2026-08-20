#!/usr/bin/env bash
# Veyra File Manager — source build & install script.
#
# Builds Veyra from source and installs it system-wide via `make install`.
# Does NOT touch existing distro packages; it only builds this checkout.
#
# Usage:
#   ./install.sh              # detect distro, install build deps, build, install
#   ./install.sh --no-deps    # skip dependency installation (already have them)
#   ./install.sh --user       # install to ~/.local, no sudo required for install step
#   ./install.sh --uninstall  # remove a previous install.sh install
#   PREFIX=/usr/local ./install.sh
#
# Requires sudo for dependency installation and for `make install` (system
# paths under PREFIX, default /usr). Never run this script itself as root;
# it calls sudo only for the specific steps that need it. Pass --user to
# install under ~/.local instead, which needs no sudo for the install step.

set -euo pipefail

USER_INSTALL=0
PREFIX_SET=0
[ -n "${PREFIX:-}" ] && PREFIX_SET=1
PREFIX="${PREFIX:-/usr}"
INSTALL_DEPS=1
ACTION="install"

for arg in "$@"; do
    case "$arg" in
        --no-deps) INSTALL_DEPS=0 ;;
        --user) USER_INSTALL=1 ;;
        --uninstall) ACTION="uninstall" ;;
        -h|--help)
            sed -n '2,17p' "$0"
            exit 0
            ;;
        *)
            echo "Unknown option: $arg" >&2
            exit 1
            ;;
    esac
done

if [ "$USER_INSTALL" -eq 1 ] && [ "$PREFIX_SET" -eq 0 ]; then
    PREFIX="$HOME/.local"
fi

if [ "$(id -u)" -eq 0 ]; then
    echo "Do not run install.sh as root. It calls sudo itself where needed." >&2
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

if [ "$ACTION" = "uninstall" ]; then
    echo "==> Uninstalling Veyra (PREFIX=$PREFIX)"
    if [ "$USER_INSTALL" -eq 1 ]; then
        make PREFIX="$PREFIX" uninstall
    else
        sudo make PREFIX="$PREFIX" uninstall
    fi
    exit 0
fi

detect_distro() {
    if [ -r /etc/os-release ]; then
        # shellcheck disable=SC1091
        . /etc/os-release
        echo "${ID:-unknown}"
    else
        echo "unknown"
    fi
}

install_deps() {
    local distro
    distro="$(detect_distro)"
    echo "==> Detected distro: $distro"

    case "$distro" in
        arch|manjaro|endeavouros|cachyos|garuda)
            sudo pacman -Sy --needed --noconfirm rust gtk4 libadwaita
            ;;
        fedora|rhel|centos)
            sudo dnf install -y cargo rust gtk4-devel libadwaita-devel
            ;;
        debian|ubuntu|pop|linuxmint|zorin|elementary)
            sudo apt update
            sudo apt install -y cargo rustc libgtk-4-dev libadwaita-1-dev
            ;;
        opensuse*|suse)
            sudo zypper install -y cargo rust gtk4-devel libadwaita-devel
            ;;
        *)
            echo "!! Unrecognized distro '$distro' — skipping automatic dependency install."
            echo "   Install Rust 1.85+, GTK4 dev headers and Libadwaita dev headers manually,"
            echo "   then re-run with --no-deps."
            ;;
    esac
}

if [ "$INSTALL_DEPS" -eq 1 ]; then
    install_deps
else
    echo "==> Skipping dependency installation (--no-deps)"
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo not found on PATH. Install Rust (https://rustup.rs) or re-run without --no-deps." >&2
    exit 1
fi

echo "==> Building Veyra (release)"
cargo build --release --workspace --locked

if [ "$USER_INSTALL" -eq 1 ]; then
    echo "==> Installing to PREFIX=$PREFIX (user install, no sudo)"
    make PREFIX="$PREFIX" install
else
    echo "==> Installing to PREFIX=$PREFIX (requires sudo)"
    sudo make PREFIX="$PREFIX" install
fi

echo "==> Done. Launch with: veyra"
