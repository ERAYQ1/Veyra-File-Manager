Name:           veyra
Version:        0.1.0
Release:        0
Summary:        Modern, high-performance Linux file manager built with Rust, GTK4 and Libadwaita
License:        GPL-3.0-or-later
Group:          System/X11/Utilities
URL:            https://github.com/ERAYQ1/Veyra-File-Manager
Source0:        %{url}/archive/refs/tags/v%{version}/Veyra-File-Manager-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  pkgconfig(gtk4)
BuildRequires:  pkgconfig(libadwaita-1)
BuildRequires:  desktop-file-utils
BuildRequires:  appstream-glib

Requires:       gtk4
Requires:       libadwaita
Requires:       glib2

Recommends:     polkit
Recommends:     gvfs
Recommends:     xdg-terminal-exec

%description
Veyra is a modern, fast and secure file manager for the Linux desktop,
built in Rust with GTK4 and Libadwaita. It targets professional-grade
functionality with a keyboard-first, accessible and responsive
interface, without compromising on memory safety or performance under
very large directories.

%prep
%setup -q -n Veyra-File-Manager-%{version}

%build
cargo build --release --locked --workspace

%install
%make_install PREFIX=%{_prefix}

%check
desktop-file-validate %{buildroot}%{_datadir}/applications/io.github.erayq1.Veyra.desktop
appstream-util validate-relax --nonet %{buildroot}%{_datadir}/metainfo/io.github.erayq1.Veyra.metainfo.xml

%files
%license LICENSE
%doc README.md
%{_bindir}/veyra
%{_datadir}/applications/io.github.erayq1.Veyra.desktop
%{_datadir}/metainfo/io.github.erayq1.Veyra.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/io.github.erayq1.Veyra.svg
%{_datadir}/icons/hicolor/symbolic/apps/io.github.erayq1.Veyra-symbolic.svg

%changelog
* Thu Aug 20 2026 Veyra Contributors <okuslug33@gmail.com> - 0.1.0-0
- Faz 59: redesigned the app icon around a minimal geometric Rust crab
  (colorful hicolor SVG plus a new monochrome symbolic variant for
  tray/dock/notifications), added the symbolic icon to %files.
- Faz 59: Command Palette is now fully localized (39 command titles + 5
  category labels, EN/TR), rename dialog warns on Unicode bidi-override
  spoofing attempts, bulk copy honors mid-file GIO cancellation, and the
  thumbnail L2 disk cache can be inspected and cleared from Preferences.
- Modernized breadcrumb, view-switcher, and card styling to current
  Libadwaita conventions; added AdwToast feedback for clipboard copy,
  Move to Trash (with inline Undo), and Undo/Redo.
