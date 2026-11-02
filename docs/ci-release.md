# CI and releases

[Repository](https://github.com/fossgap/open-meteo) · [README](../README.md) · [Packaging](packaging.md)

## Test CI

[Test CI](../.github/workflows/test-ci.yml) runs on pushes to `main`, pull requests,
and manual dispatches. It builds the [Debian 12 container](../Dockerfile), using
Rust 1.90, GTK 4.8, libadwaita 1.2, and glibc 2.36. Docker layers are cached between
runs. The checks cover:

- Rust formatting, Clippy with warnings denied, and unit tests.
- ShellCheck, PKGBUILD syntax, and the desktop launcher.
- Negative release-integrity tests: corrupted downloads, incomplete/duplicate
  checksum entries, wrong versions, extra files, symlinks, and unsafe archive paths.
- Real GTK controls under Xvfb and a private D-Bus session: theme changes, units,
  search states, narrow layout, daily details, tray menus/hiding/restoration/loss,
  first launch, offline cache, preferences, and quit.
- Weather-symbol retention during temperature updates, GNOME bridge
  attach/detach/crash recovery, all six translations at wide and narrow sizes,
  and language selection/persistence.
- A real XFCE panel checks one button containing the icon and temperature on
  one line in light and dark themes, unit changes, a shared hover/click area,
  and cleanup. The native label properties and update signals are also checked.
  These run for the source binary and AppImage, with private settings.
- GNOME setup against a private D-Bus service and settings: immediate activation,
  first-install scheduling, repeated setup, updates, and failure states. The same
  checks run for source builds, AppImages, and installed native packages.
- A separate [GNOME Clock CI](../.github/workflows/gnome-ci.yml) runs GNOME 50 in
  an isolated Fedora 44 container: shared clock click target, a five-hour forecast
  in the calendar column, light/dark/RTL layouts, empty and saved states, unit and
  saved-weather updates, app restarts, and immediate enablement over GNOME's
  public D-Bus interface. Disabling restores GNOME's original weather section.
  The host desktop and installed extensions are untouched.
- The same GNOME environment verifies AppIndicator's native weather icon and
  temperature label inside one button, readable sizes, shared hit targets,
  updates and activation.

When a built AppImage is available inside the GNOME test container, set
`OPEN_METEO_TEST_BINARY` to its absolute path before running
`sh scripts/check-gnome.sh`. This additionally checks the real packaged tray,
Wayland activation of a minimized forecast window, and removal on quit. Its
screenshot is saved as `target/ci/gnome-appimage.png`. It also runs the packaged app
with the clock and AppIndicator extensions together, checking hourly forecasts,
calendar-card activation, tray removal/restoration, and cleanup. That capture is
`target/ci/gnome-clock-app.png`. Release CI requires this packaged GNOME check
before publication and retains its results in `gnome-package-evidence`.

The native tests use synthetic weather, temporary settings, mock desktop services,
and a refused local proxy. They do not require the weather API. Live API testing
remains an explicit `cargo run --locked -- --check-api` command.

Actions are pinned to verified commit hashes. Test/build/verification jobs have
read-only repository permissions and do not retain checkout credentials. UI logs
and screenshots are uploaded even after test failures, when available, for 14 days.

## Release CI

[Release CI](../.github/workflows/release-ci.yml) runs for `v*` tag pushes and manual
dispatches. It repeats Test CI, builds the release binary on Debian 12, and creates
six artifacts plus `SHA256SUMS`:

| Artifact | Contents |
| --- | --- |
| `open-meteo_<version>_amd64.deb` | Debian/Ubuntu application |
| `open-meteo-<version>-1.x86_64.rpm` | Fedora application |
| `open-meteo-<version>-1-x86_64.pkg.tar.zst` | Installable Arch application |
| `Open_Meteo-<version>-x86_64.AppImage` | Application and bundled GTK/libadwaita runtime |
| `open-meteo-<version>.tar.gz` | Versioned source, build files, tests, docs, and workflows |
| `PKGBUILD` | Arch source recipe containing that archive's SHA-256 |

The Rust `verify-packages` tool in `tools/checks/` requires this exact inventory, verifies versions,
architectures, dependency declarations, launcher/icon/docs payloads, and exact
GNOME extension files. License metadata and bundled license texts must match the
source, including ksni's Unlicense. The source archive must contain all translation catalogs. Native
package executables must match byte for byte. AppImage code, read-only data, and
build ID must match the native binary; linuxdeploy changes its ELF runtime path.
The verifier checks the AppImage's GTK libraries
and theme/backend hook, the source archive, and the PKGBUILD checksum before writing
`SHA256SUMS`. The AppImage then runs the full native UI suite.

The `linux-x86_64-release` Actions artifact retains the exact outputs for 14 days.
The separate `linux-verification-tools` artifact carries the Rust verifier and
desktop harness built on the same Debian 12 baseline. Installation and publication
jobs download these tools; they are not included in the application packages or
published downloads. Local `scripts/ci.sh release` saves them in `target/check-tools/`.
Every installation job downloads the release artifact and checks all checksums before
installing anything in its disposable container:

| Target | Container | Package |
| --- | --- | --- |
| Debian | Debian 12 Bookworm | DEB through APT |
| Kali | Kali rolling | DEB through APT |
| Fedora | Fedora 44 | RPM through DNF |
| Arch | Arch rolling | Binary package through pacman |
| AppImage | Ubuntu 24.04 without host GTK4/libadwaita | AppImage, extracted without FUSE |

Native packages must resolve their own dependencies and run `--version` before
graphical test tools are installed. Each job verifies the installed version and desktop
integration, then runs the native UI suite as an ordinary user. Containers receive
read-only artifact/test mounts, a writable evidence directory, and disposable
settings; host packages are untouched. Evidence is written as the invoking user's
UID so it remains accessible outside Docker.
Container image digests, installation logs, and UI captures are retained in
`package-verification-<target>` artifacts, including on failures.

## Publish a version

Update the Cargo version and lockfile, commit the intended release, then push a
matching tag. The packaging script derives artifact/PKGBUILD versions from Cargo.

```sh
git tag -a v0.1.0 -m 'Open Meteo 0.1.0'
git push origin v0.1.0
```

Mismatched tags fail before the container build. A separate publication job runs
only on tag **pushes**, after the build, GNOME clock tests, and all five installation jobs pass. It
rechecks the downloaded inventory/checksums and creates a release with generated
notes; it does not overwrite an existing release. Only this job gets
`contents: write`, using GitHub's provided token.

A manual dispatch, including a dispatch on a tag, **never publishes**:

```sh
gh workflow run release-ci.yml --repo fossgap/open-meteo --ref main
```

Release downloads become available after the tagged workflow passes every check
and its publication job succeeds.

## Reproduce locally

```sh
docker build -t open-meteo-ci .
sh scripts/ci.sh check
docker build -f tests/gnome.Dockerfile -t open-meteo-gnome .
docker run --rm --tmpfs /tmp:rw,exec --shm-size=1g open-meteo-gnome
sh scripts/ci.sh release
for target in debian kali fedora arch appimage; do
    sh scripts/check-package-installation.sh "$target" dist/release
done
```

Use a Docker-capable x86_64 Linux host with network access for build dependencies
and container package repositories. The release command requires a new output
directory; set `OPEN_METEO_CI_OUTPUT=dist/another-release` for subsequent builds.
`OPEN_METEO_CI_IMAGE` selects a custom build image. Source archives reflect the
Docker build context, so rebuild the image after edits. GitHub uses a clean checkout
of the triggering revision.

If Docker's storage is constrained, set `OPEN_METEO_CI_TARGET_DIR` to a dedicated
directory on a larger filesystem. It mounts the Cargo build output there instead
of storing it in the container layer; an empty directory triggers recompilation.
Use a dedicated cache because container builds run as root.

Native UI evidence is copied to `target/ci/`; installation captures go to
`target/package-installation/<target>/`. The commands above build and verify local
files. They do not publish releases.
