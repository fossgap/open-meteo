Name: open-meteo
Version: @VERSION@
Release: 1%{?dist}
Summary: Native Linux weather forecasts with system tray access
URL: https://github.com/fossgap/open-meteo
License: LicenseRef-Not-Specified
Source0: open-meteo.tar.gz
Requires: gtk4 >= 4.8
Requires: libadwaita >= 1.2
Requires: ca-certificates
Requires: adwaita-icon-theme
%global debug_package %{nil}
# Cargo already strips release executables. Preserve the staged binary bytes.
%global __os_install_post %{nil}

%description
Current conditions, 24-hour and seven-day forecasts, city search, and offline
caching, using the Open-Meteo service.

%prep
%setup -q -c

%build

%install
mkdir -p %{buildroot}
cp -a usr %{buildroot}/

%files
%{_bindir}/open-meteo
%{_datadir}/applications/io.github.open_meteo.OpenMeteo.desktop
%{_datadir}/icons/hicolor/scalable/apps/io.github.open_meteo.OpenMeteo.svg
%doc %{_datadir}/doc/open-meteo/README.md
%{_datadir}/gnome-shell/extensions/open-meteo-clock@fossgap.github.io/
