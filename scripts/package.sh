#!/bin/sh
# Build local packages. No installation, signing, publishing, or tool downloads.
set -eu

project_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project_root"
package_version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)
package_format=${1:-}
app_id=io.github.open_meteo.OpenMeteo
output_dir="$project_root/dist"
machine_arch=$(uname -m)

need() { command -v "$1" >/dev/null 2>&1 || { echo "Required tool not found: $1" >&2; exit 1; }; }

case "$package_format" in
  deb) need dpkg-deb; need dpkg-shlibdeps ;;
  rpm) need rpmbuild ;;
  appimage) need linuxdeploy; need linuxdeploy-plugin-gtk.sh ;;
  arch) need tar; need zstd ;;
  arch-source) need tar; need sha256sum ;;
  *) echo 'Usage: scripts/package.sh {deb|rpm|appimage|arch|arch-source}' >&2; exit 2 ;;
esac

mkdir -p "$output_dir"
if [ "$package_format" = arch-source ]; then
  archive_dir="$output_dir/arch"
  mkdir -p "$archive_dir"
  tar --exclude='__pycache__' --exclude='*.pyc' --transform "s,^,open-meteo-$package_version/," \
    -czf "$archive_dir/open-meteo-$package_version.tar.gz" \
    Cargo.toml Cargo.lock src tests resources docs scripts packaging vendor Makefile README.md LICENSE AGENTS.md \
    Dockerfile .dockerignore .gitignore .github
  sed "s/^pkgver=.*/pkgver=$package_version/" packaging/arch/PKGBUILD > "$archive_dir/PKGBUILD"
  source_hash=$(sha256sum "$archive_dir/open-meteo-$package_version.tar.gz" | cut -d ' ' -f 1)
  sed -i "s/sha256sums=('SKIP')/sha256sums=('$source_hash')/" "$archive_dir/PKGBUILD"
  echo "Arch source package: $archive_dir"
  echo 'On Arch Linux: cd dist/arch && makepkg -si'
  exit 0
fi

if [ -n "${OPEN_METEO_BINARY:-}" ]; then
  binary=$(realpath "$OPEN_METEO_BINARY")
else
  cargo build --release --locked
  binary="$project_root/target/release/open-meteo"
fi
[ -x "$binary" ] || { echo "Executable not found: $binary" >&2; exit 1; }
stage_dir=$(mktemp -d "$output_dir/.stage-$package_format.XXXXXX")
chmod 755 "$stage_dir"
trap 'rm -rf "$stage_dir"' EXIT HUP INT TERM
install -Dm755 "$binary" "$stage_dir/usr/bin/open-meteo"
install -Dm644 "resources/$app_id.desktop" "$stage_dir/usr/share/applications/$app_id.desktop"
install -Dm644 "resources/$app_id.svg" "$stage_dir/usr/share/icons/hicolor/scalable/apps/$app_id.svg"
install -Dm644 README.md "$stage_dir/usr/share/doc/open-meteo/README.md"
install -Dm644 packaging/copyright "$stage_dir/usr/share/doc/open-meteo/copyright"
install -Dm644 LICENSE "$stage_dir/usr/share/licenses/open-meteo/LICENSE"
install -Dm644 vendor/ksni/UNLICENSE "$stage_dir/usr/share/licenses/open-meteo/ksni-UNLICENSE"
for extension_file in resources/gnome-extension/*; do
  install -Dm644 "$extension_file" "$stage_dir/usr/share/gnome-shell/extensions/open-meteo-clock@fossgap.github.io/$(basename "$extension_file")"
done

case "$package_format" in
  deb)
    case "$machine_arch" in x86_64) deb_arch=amd64 ;; aarch64) deb_arch=arm64 ;; *) deb_arch="$machine_arch" ;; esac
    mkdir -p "$stage_dir/DEBIAN" "$stage_dir/dependency-check/debian"
    cat > "$stage_dir/dependency-check/debian/control" <<EOF
Source: open-meteo
Section: utils
Priority: optional
Maintainer: Open Meteo contributors <noreply@localhost>

Package: open-meteo
Architecture: any
Description: Native Linux weather app
EOF
    library_depends=$(cd "$stage_dir/dependency-check" && dpkg-shlibdeps -O -e"$binary" | sed -n 's/^shlibs:Depends=//p')
    [ -n "$library_depends" ] || { echo 'Could not determine runtime library dependencies' >&2; exit 1; }
    rm -rf "$stage_dir/dependency-check"
    cat > "$stage_dir/DEBIAN/control" <<EOF
Package: open-meteo
Version: $package_version
Section: utils
Priority: optional
Architecture: $deb_arch
Maintainer: Open Meteo contributors <noreply@localhost>
Homepage: https://github.com/fossgap/open-meteo
Depends: $library_depends, ca-certificates, adwaita-icon-theme
Description: Native Linux weather forecasts with system tray access
 Current conditions, 24-hour and seven-day forecasts, city search,
 and offline caching, using the Open-Meteo service.
EOF
    dpkg-deb --root-owner-group --build "$stage_dir" "$output_dir/open-meteo_${package_version}_$deb_arch.deb"
    ;;
  rpm)
    rpm_root="$stage_dir/rpmbuild"
    mkdir -p "$rpm_root/BUILD" "$rpm_root/BUILDROOT" "$rpm_root/RPMS" "$rpm_root/SOURCES" "$rpm_root/SPECS" "$rpm_root/SRPMS" "$rpm_root/rpmdb" "$rpm_root/tmp"
    tar -C "$stage_dir" -czf "$rpm_root/SOURCES/open-meteo.tar.gz" usr
    sed "s/@VERSION@/$package_version/g" packaging/rpm/open-meteo.spec > "$rpm_root/SPECS/open-meteo.spec"
    rpmbuild --define "_topdir $rpm_root" --define "_dbpath $rpm_root/rpmdb" \
      --define "_tmppath $rpm_root/tmp" --define '_build_id_links none' -bb "$rpm_root/SPECS/open-meteo.spec"
    find "$rpm_root/RPMS" -name '*.rpm' -exec cp '{}' "$output_dir/" \;
    ;;
  arch)
    cat > "$stage_dir/.PKGINFO" <<EOF
pkgname = open-meteo
pkgbase = open-meteo
pkgver = $package_version-1
pkgdesc = Native Linux weather forecasts with system tray access
url = https://github.com/fossgap/open-meteo
builddate = ${SOURCE_DATE_EPOCH:-$(date +%s)}
packager = FOSSGap Open Meteo contributors
size = $(du -sb "$stage_dir/usr" | cut -f1)
arch = $machine_arch
license = GPL-3.0-only
depend = glibc
depend = gtk4>=4.8
depend = libadwaita>=1.2
depend = ca-certificates
depend = adwaita-icon-theme
EOF
    tar --zstd --owner=0 --group=0 -cf "$output_dir/open-meteo-$package_version-1-$machine_arch.pkg.tar.zst" \
      -C "$stage_dir" .PKGINFO usr
    ;;
  appimage)
    # linuxdeploy's GTK plugin bundles schemas, icons, loaders and GTK modules.
    # Build on the oldest distribution you intend to support.
    cd "$output_dir"
    # These GTK dependencies are on linuxdeploy's generic exclusion list, but
    # minimal hosts need not have them. Explicit libraries override that list.
    set --
    for library in libfribidi.so.0 libharfbuzz.so.0 libwayland-client.so.0; do
      library_path=$(ldd "$binary" | awk -v name="$library" '$1 == name { print $3; exit }')
      [ -f "$library_path" ] || { echo "Cannot locate runtime library: $library" >&2; exit 1; }
      set -- "$@" --library "$library_path"
    done
    # Image formats and GIO modules must match the bundled GTK/GLib, even on
    # hosts with no MIME database or a newer system GLib.
    mkdir -p "$stage_dir/usr/lib/gio/modules" "$stage_dir/usr/etc" "$stage_dir/usr/share/doc/open-meteo/runtime"
    cp -a /usr/share/mime "$stage_dir/usr/share/"
    cp -aL /etc/fonts "$stage_dir/usr/etc/"
    cp -a /usr/share/icons/Adwaita "$stage_dir/usr/share/icons/"
    cp -a /usr/share/icons/hicolor/. "$stage_dir/usr/share/icons/hicolor/"
    gio_modules="$(pkg-config --variable=libdir gio-2.0)/gio/modules"
    for module in "$gio_modules/"*.so; do
      [ -f "$module" ] || continue
      cp -L "$module" "$stage_dir/usr/lib/gio/modules/"
      set -- "$@" --library "$module"
    done
    gio-querymodules "$stage_dir/usr/lib/gio/modules"
    for package in shared-mime-info fontconfig-config; do
      if [ -f "/usr/share/doc/$package/copyright" ]; then
        cp "/usr/share/doc/$package/copyright" "$stage_dir/usr/share/doc/open-meteo/runtime/$package.copyright"
      fi
    done
    DEPLOY_GTK_VERSION=4 VERSION="$package_version" linuxdeploy \
      --appdir "$stage_dir" --executable "$stage_dir/usr/bin/open-meteo" \
      --desktop-file "$stage_dir/usr/share/applications/$app_id.desktop" \
      --icon-file "$stage_dir/usr/share/icons/hicolor/scalable/apps/$app_id.svg" \
      --plugin gtk "$@"
    # The generic GTK plugin forces GTK_THEME and X11. GTK_THEME disables
    # libadwaita's stylesheet; retain its bundled resources, not those overrides.
    gtk_hook="$stage_dir/apprun-hooks/linuxdeploy-plugin-gtk.sh"
    [ -f "$gtk_hook" ] || { echo 'GTK runtime hook was not generated' >&2; exit 1; }
    sed -i '/^export GTK_THEME=/d; /^export GDK_BACKEND=/d' "$gtk_hook"
    install -m755 "$project_root/packaging/appimage-runtime.sh" "$stage_dir/apprun-hooks/zz-open-meteo.sh"
    VERSION="$package_version" linuxdeploy --appdir "$stage_dir" --output appimage
    ;;
esac
