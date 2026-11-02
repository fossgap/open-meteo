# Packages and verification

[README](../README.md) · [CI and releases](ci-release.md)

## Build packages

Build on the oldest supported distribution. The release container uses Debian 12
(GTK 4.8, libadwaita 1.2, glibc 2.36); a local build on a newer system can require
newer runtime libraries regardless of the Cargo feature minimums.

| Format | Command | Additional tools |
| --- | --- | --- |
| Debian / Ubuntu | `make deb` | `dpkg-deb`, `dpkg-shlibdeps` (`dpkg-dev`) |
| Fedora / RPM | `make rpm` | `rpmbuild` (`rpm-build` on Fedora, `rpm` on Debian) |
| Arch binary | `make arch` | GNU `tar`, `zstd` |
| AppImage | `make appimage` | `linuxdeploy`, `linuxdeploy-plugin-gtk.sh` |
| Arch source | `make arch-source` | GNU `tar`, `sha256sum` |

Binary commands build the release executable automatically. Set
`OPEN_METEO_BINARY=target/debug/open-meteo` to package an already-built executable
for local checks. Every binary package includes the application, desktop launcher,
SVG icon, and README. Debian dependencies are derived from the executable; RPM
records library dependencies as well as the explicit GTK/libadwaita requirements.
The Arch binary includes `.PKGINFO` and runtime dependencies for pacman.

Commands write to `dist/`, without installation or publication. The RPM build uses
its own temporary directory and database; it does not change the system RPM database.

`make arch-source` writes a versioned source archive and a checksummed PKGBUILD to
`dist/arch/`. On Arch, run `cd dist/arch && makepkg -si` as a normal user to build,
test, and install from source. This is separate from the prebuilt binary installed
by the release verification matrix.

## AppImage tools

Install the official [linuxdeploy](https://github.com/linuxdeploy/linuxdeploy) and
[GTK plugin](https://github.com/linuxdeploy/linuxdeploy-plugin-gtk) on `PATH`, or
explicitly fetch the checksum-pinned x86_64 tools used by CI:

```sh
sh scripts/fetch-appimage-tools.sh
PATH="$PWD/target/appimage-tools:$PATH" APPIMAGE_EXTRACT_AND_RUN=1 make appimage
```

The bootstrap verifies linuxdeploy `1-alpha-20251107-1` and GTK plugin revision
`7a3fbc31a9e5075073ff8790f26effbac5f84453` before execution, including cached copies.
The ordinary packaging command does not download tools. CI explicitly runs the
bootstrap. Downloads remain in `target/appimage-tools/`.

The GTK plugin bundles runtime libraries and resources. Packaging removes its
forced `GTK_THEME` and `GDK_BACKEND` settings so libadwaita keeps its stylesheet
and follows the desktop. FriBidi, HarfBuzz, and the Wayland client library are
explicitly bundled because linuxdeploy otherwise assumes the host provides them.
The Adwaita icon theme, MIME database, matching GIO modules, and a fallback
Fontconfig configuration are also bundled. The native UI test checks visible
weather-icon pixels and rejects icon-loading and module-loading errors.
The AppImage is tested without host GTK4/libadwaita on
Ubuntu 24.04; this does not establish compatibility with every Linux distribution.

To run without FUSE:

```sh
APPIMAGE_EXTRACT_AND_RUN=1 ./Open_Meteo-0.1.0-x86_64.AppImage
```

## Verify downloads

Download `SHA256SUMS` with the chosen package. For example, when only the DEB was
downloaded, select its exact manifest entry and check it:

```sh
grep '  open-meteo_0.1.0_amd64.deb$' SHA256SUMS | sha256sum -c -
```

Use the version/filename you actually downloaded. A successful hash check verifies
the bytes against the downloaded manifest; these artifacts are not separately signed.
If all release assets are present, run `sha256sum -c SHA256SUMS`.

For a full release produced by `sh scripts/ci.sh release`:

```sh
python3 scripts/verify-packages.py --checksums-only dist/release
make check-packages
sh scripts/check-package-installation.sh debian dist/release
```

The full verifier needs Python 3, `dpkg-deb`, `rpm`, `rpm2cpio`, `cpio`, `zstd`, `objcopy` (`binutils`), and
`desktop-file-validate`. It checks complete contents, package metadata, and matching
executables, and extracts the AppImage in a temporary directory without FUSE.
Use it only for locally built or trusted release artifacts because AppImage
extraction executes its runtime. The checksum-only mode requires only Python.

Installation checks run only in disposable containers. Available targets are
`debian`, `kali`, `fedora`, `arch`, and `appimage`; see the
[release matrix](ci-release.md#release-ci). UI tests use sample weather and a refused
proxy, with no live weather API dependency.

## License metadata

The application is licensed `GPL-3.0-only`, recorded in Cargo, RPM, and Arch
metadata. Every binary package includes the full license and ksni's Unlicense
under `/usr/share/licenses/open-meteo/`, plus copyright information under
`/usr/share/doc/open-meteo/`. The source archive includes both license texts.
Third-party libraries and weather data retain their own terms.

## GNOME clock and translations

Every native package includes `open-meteo-clock@fossgap.github.io` in
`/usr/share/gnome-shell/extensions`. The application also embeds the extension, so
`open-meteo --install-gnome-extension` works for AppImages and user/source installs.
Installation does not enable the extension or change desktop settings. Follow the
[GNOME setup instructions](../README.md#gnome-system-clock) to enable it.

The seven UTF-8 translation catalogs are embedded at compile time; packages need
no external locale files or gettext compilation step. The UI uses the desktop's
fonts. Install suitable fonts for Chinese and Devanagari if your system lacks them
(for example, `fonts-noto-core fonts-noto-cjk` on Debian/Ubuntu).
