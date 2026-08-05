#
# spec file for package postgres-draco
#
# Copyright (c) 2026 Rodrigo Brito
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#

Name:           postgres-draco
Version:        2.1.0
Release:        1
Summary:        Cliente de banco de dados do ecossistema Lyra OS
License:        GPL-3.0-or-later
Group:          Productivity/Databases/Tools
URL:            https://github.com/britors/Draco
Source0:        %{name}-%{version}.tar.zst
Source1:        vendor.tar.zst
BuildRequires:  cargo
BuildRequires:  cargo-packaging
BuildRequires:  rust >= 1.85
BuildRequires:  pkgconfig
BuildRequires:  pkgconfig(webkit2gtk-4.1)
BuildRequires:  pkgconfig(openssl)
BuildRequires:  pkgconfig(librsvg-2.0)
BuildRequires:  desktop-file-utils
BuildRequires:  appstream-glib
BuildRequires:  zstd
Requires:       xdg-desktop-portal

%description
Draco é o cliente de banco de dados do ecossistema Lyra OS: explorador de
esquemas, editor SQL e ferramenta de administração para PostgreSQL. É um
aplicativo independente, utilizável em qualquer distribuição Linux moderna,
com integração visual prioritária ao Lyra (GNOME/Wayland).

Implementado em Rust, usando Tauri 2 com assets web locais e WebKitGTK 4.1.
Conexões via túnel SSH (incluindo jump host) são feitas em processo, sem
depender do binário `ssh`. Nenhuma senha ou passphrase é manuseada em texto
plano — armazenamento delegado ao Serviço de Segredos do sistema (GNOME
Keyring/KWallet), já integrado ao sistema.

%prep
# -a1 extracts Source0, then unpacks Source1 (vendor.tar.zst) on top of it; the vendor
# tarball produced by the cargo_vendor OBS service already includes .cargo/config.toml, so
# no manual step is needed to point cargo at the vendored crates.
%autosetup -a1 -p1

%build
cargo build --locked --offline --release -p draco-tauri

%install
install -Dm0755 target/release/draco %{buildroot}%{_bindir}/draco
install -Dm0644 data/org.lyraos.Draco.desktop \
    %{buildroot}%{_datadir}/applications/org.lyraos.Draco.desktop
install -Dm0644 data/org.lyraos.Draco.metainfo.xml \
    %{buildroot}%{_datadir}/metainfo/org.lyraos.Draco.metainfo.xml
install -Dm0644 src-tauri/icons/512x512.png \
    %{buildroot}%{_datadir}/icons/hicolor/512x512/apps/org.lyraos.Draco.png

desktop-file-validate %{buildroot}%{_datadir}/applications/org.lyraos.Draco.desktop
appstream-util validate-relax --nonet \
    %{buildroot}%{_datadir}/metainfo/org.lyraos.Draco.metainfo.xml

%check
# Live PostgreSQL and browser/display tests are ignored; unit, contract and Tauri smoke tests run
# against the vendored Rust dependencies without a server or GUI session.
cargo test --locked --offline --workspace

%files
%license LICENSE
%doc README.md
%{_bindir}/draco
%{_datadir}/applications/org.lyraos.Draco.desktop
%{_datadir}/metainfo/org.lyraos.Draco.metainfo.xml
%{_datadir}/icons/hicolor/512x512/apps/org.lyraos.Draco.png

%changelog
