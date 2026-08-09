%global debug_package %{nil}
%{!?_draco_binary:%global _draco_binary %{_draco_root}/target/release/draco}

Name:           draco
Version:        %{version}
Release:        1
Summary:        PostgreSQL workbench
License:        GPL-3.0-or-later
URL:            https://github.com/britors/Draco
BuildArch:      x86_64
Requires:       xdg-desktop-portal

%description
Draco is a PostgreSQL workbench for exploring, querying and administering
databases with a native Tauri/WebKitGTK desktop shell.

%prep

%build

%install
root="%{_draco_root}"
install -Dm0755 "%{_draco_binary}" \
    "%{buildroot}%{_bindir}/draco"
install -Dm0644 "${root}/data/org.lyraos.Draco.desktop" \
    "%{buildroot}%{_datadir}/applications/org.lyraos.Draco.desktop"
install -Dm0644 "${root}/data/org.lyraos.Draco.metainfo.xml" \
    "%{buildroot}%{_datadir}/metainfo/org.lyraos.Draco.metainfo.xml"
for size in 32 128 512; do
    install -Dm0644 "${root}/src-tauri/icons/${size}x${size}.png" \
        "%{buildroot}%{_datadir}/icons/hicolor/${size}x${size}/apps/org.lyraos.Draco.png"
done
install -Dm0644 "${root}/src-tauri/icons/128x128@2x.png" \
    "%{buildroot}%{_datadir}/icons/hicolor/256x256/apps/org.lyraos.Draco.png"
install -Dm0644 "${root}/LICENSE" \
    "%{buildroot}%{_datadir}/licenses/%{name}/LICENSE"
install -Dm0644 "${root}/README.md" \
    "%{buildroot}%{_docdir}/%{name}/README.md"

desktop-file-validate "%{buildroot}%{_datadir}/applications/org.lyraos.Draco.desktop"

%files
%license %{_datadir}/licenses/%{name}/LICENSE
%doc %{_docdir}/%{name}/README.md
%{_bindir}/draco
%{_datadir}/applications/org.lyraos.Draco.desktop
%{_datadir}/metainfo/org.lyraos.Draco.metainfo.xml
%{_datadir}/icons/hicolor/*/apps/org.lyraos.Draco.png

%changelog
