Name:           veyra
Version:        0.1.0
Release:        1%{?dist}
Summary:        Modern, high-performance Linux file manager built with Rust, GTK4 and Libadwaita

License:        GPL-3.0-or-later
URL:            https://github.com/ERAYQ1/Veyra-File-Manager
Source0:        %{url}/archive/refs/tags/v%{version}/Veyra-File-Manager-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gtk4-devel
BuildRequires:  libadwaita-devel
BuildRequires:  desktop-file-utils
BuildRequires:  libappstream-glib

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
%autosetup -n Veyra-File-Manager-%{version}

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
* Thu Aug 20 2026 Veyra Contributors <okuslug33@gmail.com> - 0.1.0-1
- Faz 59: new geometric Rust crab app icon plus a monochrome symbolic
  variant, packaged alongside the existing hicolor SVG.

* Wed Aug 19 2026 Veyra Contributors <okuslug33@gmail.com> - 0.1.0-1
- Initial native RPM packaging (Faz 46).
