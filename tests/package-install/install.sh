#!/bin/sh
# Called only by scripts/check-package-installation.sh in an isolated container.
set -eu
if [ "${OPEN_METEO_INSTALL_TEST_CONTAINER:-}" != 1 ] || [ ! -f /.dockerenv ] || [ ! -d /artifacts ]; then
    echo 'Use scripts/check-package-installation.sh; this script installs container packages.' >&2
    exit 2
fi
format=${1:?Provide a package format}
version=${2:?Provide the expected version}
set -x
one_file() {
    if [ "$#" -ne 1 ] || [ ! -f "$1" ]; then
        echo 'Expected exactly one release package.' >&2
        exit 1
    fi
    printf '%s\n' "$1"
}
cat /etc/os-release
binary=/usr/bin/open-meteo
case "$format" in
    deb)
        package=$(one_file /artifacts/open-meteo_*_amd64.deb)
        [ "$(dpkg-deb -f "$package" Version)" = "$version" ]
        apt-get update
        apt-get -y --no-remove --no-install-recommends install "$package"
        [ "$(dpkg-query -W -f='${Version}' open-meteo)" = "$version" ]
        apt-get check
        ;;
    rpm)
        package=$(one_file /artifacts/open-meteo-[0-9]*.x86_64.rpm)
        dnf -y --setopt=install_weak_deps=False install "$package"
        [ "$(rpm -q --qf '%{VERSION}-%{RELEASE}' open-meteo)" = "$version-1" ]
        rpm -V open-meteo
        dnf check
        # runuser is a test helper, not an application runtime dependency.
        dnf -y --setopt=install_weak_deps=False install util-linux
        ;;
    arch)
        package=$(one_file /artifacts/open-meteo-[0-9]*-x86_64.pkg.tar.zst)
        pacman --noconfirm -Syu
        pacman --noconfirm -U "$package"
        [ "$(pacman -Q open-meteo)" = "open-meteo $version-1" ]
        pacman -Qk open-meteo
        ;;
    appimage)
        package=$(one_file /artifacts/Open_Meteo-*-x86_64.AppImage)
        install -m755 "$package" /opt/OpenMeteo.AppImage
        binary=/opt/OpenMeteo.AppImage
        export APPIMAGE_EXTRACT_AND_RUN=1
        ;;
    *) echo "Unknown package format: $format" >&2; exit 2 ;;
esac
# Check runtime linkage before test tools can accidentally supply missing libs.
useradd -m -o -u "${OPEN_METEO_TEST_UID:-1000}" meteotest
mkdir -p /results
chown meteotest:meteotest /results
if [ "$format" != appimage ]; then
    [ "$(runuser -u meteotest -- "$binary" --version)" = "Open Meteo $version" ]
fi
case "$format" in
    deb|appimage)
        apt-get update
        apt-get -y --no-install-recommends install python3-gi gir1.2-atspi-2.0 at-spi2-core \
            xvfb xauth dbus-x11 xdotool x11-utils imagemagick fonts-dejavu-core desktop-file-utils
        ;;
    rpm)
        dnf -y --setopt=install_weak_deps=False install python3-gobject-base gobject-introspection at-spi2-core xorg-x11-server-Xvfb \
            xorg-x11-xauth dbus-daemon xdotool xwininfo ImageMagick dejavu-sans-fonts desktop-file-utils
        ;;
    arch)
        pacman --noconfirm -S --needed python-gobject at-spi2-core xorg-server-xvfb \
            xorg-xauth dbus xdotool xorg-xwininfo imagemagick ttf-dejavu desktop-file-utils
        ;;
esac
if [ "$format" = appimage ]; then
    for library in libgtk-4-1 libadwaita-1-0; do
        [ "$(dpkg-query -W -f='${db:Status-Status}' "$library" 2>/dev/null || true)" != installed ]
    done
    echo 'APPIMAGE_HOST_HAS_NO_GTK4_OR_LIBADWAITA'
else
    desktop-file-validate /usr/share/applications/io.github.open_meteo.OpenMeteo.desktop
    test -s /usr/share/icons/hicolor/scalable/apps/io.github.open_meteo.OpenMeteo.svg
    # Minimal distro images may deliberately exclude /usr/share/doc at install.
    # The archive verifier already requires the README inside every package.
fi
[ "$(runuser -u meteotest -- "$binary" --version)" = "Open Meteo $version" ]
runuser -u meteotest -- "$binary" --help > /results/help.txt
runuser -u meteotest -- env OPEN_METEO_TEST_BINARY="$binary" OPEN_METEO_TEST_OUTPUT_DIR=/results/ui \
    sh /workspace/scripts/check-ui.sh
echo "PACKAGE_INSTALLATION_SUITE_PASSED: $format $version"
