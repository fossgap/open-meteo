# Development

[README](../README.md) · [Usage](usage.md) · [Packaging](packaging.md) · [CI and releases](ci-release.md)

## Building from source

You need **Rust 1.85 or newer**, a C toolchain, `pkg-config`, **GTK 4.8 or newer**,
and **libadwaita 1.2 or newer**. Install a current Rust toolchain with
[rustup](https://www.rust-lang.org/tools/install) if your distribution's Rust
package is older.

Install the native development libraries for your distribution:

```sh
# Debian 12+ / Ubuntu 24.04+
sudo apt install build-essential pkg-config libgtk-4-dev libadwaita-1-dev

# Fedora
sudo dnf install gcc pkgconf-pkg-config gtk4-devel libadwaita-devel

# Arch Linux
sudo pacman -S --needed rust base-devel pkgconf gtk4 libadwaita
```

From a checkout:

```sh
git clone https://github.com/fossgap/open-meteo.git
cd open-meteo
cargo run --locked
```

For a release build:

```sh
cargo build --release --locked
./target/release/open-meteo
```

Install the binary, launcher, and icon for your user:

```sh
make install PREFIX="$HOME/.local"
```

Ensure `~/.local/bin` is on your `PATH`. Use `make uninstall PREFIX="$HOME/.local"`
to remove the installed files; saved preferences and forecasts remain.
`DESTDIR` is supported for staged installations.

```sh
make deb           # Debian / Ubuntu package
make rpm           # Fedora package
make arch          # Installable Arch package
make appimage      # Requires linuxdeploy and its GTK plugin
make arch-source   # Source archive and checksummed PKGBUILD
```

Packages go to `dist/`. These commands do not install or publish anything.
See [packaging](packaging.md) for build tools, compatibility, and verification.
The [CI and release guide](ci-release.md) describes the Debian build container,
installation matrix, retained artifacts, and tag-based release publication.

## Checks and contributions

```sh
make check

# Optional live check: queries Paris and validates the returned forecast.
cargo run --locked -- --check-api
```

Unit tests cover units, missing readings, malformed data, time-zone/DST handling,
forecast windows, refresh scheduling, recent locations, and cache corruption/isolation. Release
verification tests reject corrupt downloads, missing or duplicate checksums,
stale artifacts, unsafe archive paths, and incomplete payloads.

[Test CI](../.github/workflows/test-ci.yml) runs formatting, Clippy, Rust and package
verifier tests, shell lint, desktop-entry validation, and offline native UI checks.
[Release CI](../.github/workflows/release-ci.yml) repeats those checks, builds packages,
and installs them in fresh Debian, Kali, Fedora, Arch, and Ubuntu/AppImage containers
before a tagged release can be published. Manual runs build and verify without
publishing. Logs and screenshots are retained for 14 days.

```sh
docker build -t open-meteo-ci .
sh scripts/ci.sh check
sh scripts/ci.sh release
```

See [CI and releases](ci-release.md) to reproduce the installation matrix.

The Rust native UI checks use GLib/GIO and the AT-SPI accessibility bus, Xvfb,
`dbus-run-session`, `xdotool`, `xwininfo`, and ImageMagick's `import` command. On
Debian/Ubuntu, install `at-spi2-core xvfb dbus-x11
xdotool x11-utils imagemagick libglib2.0-bin`, then run:

```sh
cargo build --locked
sh scripts/check-ui.sh
```

This test uses temporary settings, sample data, a mock desktop settings/tray
service, and a refused local proxy. It exercises native controls, theme changes,
search, unit and refresh-interval persistence, offline fallback, tray hiding/restoration, and tray
loss, and retention of the condition icon when tray temperatures change. The
same command tests GNOME setup with a private session bus: immediate activation,
first-install scheduling, repeat setup, updates, and disabled/broken extensions.
The scripts build the Rust harness in `tools/checks/` automatically. Screenshots
and logs go to `target/ci/ui-smoke/` (gitignored).
Set `OPEN_METEO_TEST_BINARY` to test a packaged executable and
`OPEN_METEO_TEST_OUTPUT_DIR` to keep its captures separate. For an AppImage on a
FUSE-less host, also set `APPIMAGE_EXTRACT_AND_RUN=1`.

Run package-integrity regression tests with `cargo test --locked -p open-meteo-checks`.
`sh scripts/verify-packages.sh dist/release` builds and runs the Rust release
verifier. CI passes prebuilt tools through `OPEN_METEO_CHECK_TOOLS_DIR`, so
installation containers need no Rust compiler or language bindings.

With `xfce4-panel` and `xfconf` installed, `sh scripts/check-tray.sh` also checks
the actual panel layout in light and dark themes: one button containing the
weather symbol on the left and temperature on the right, unit updates, a shared
hover area, clicks across the whole item, and cleanup.
It uses a private display, D-Bus session, and XFCE settings directory.
The same command verifies native icon-and-label properties, update signals,
and activation against the real Rust tray service.

The small [ksni label extension](../vendor/ksni/OPEN-METEO.md) is vendored with its
upstream license so the icon and native temperature label share one tray item.

| Source | Responsibility |
| --- | --- |
| `src/ui.rs` | Window, actions, search, background-request coordination |
| `src/components.rs`, `src/style.css` | Forecast presentation and theme colors |
| `src/api.rs` | HTTP requests, timeouts, and user-facing network errors |
| `src/model.rs` | Typed data, validation, units, weather codes, and dates |
| `src/storage.rs` | Preferences and per-location offline cache |
| `src/tray.rs` | StatusNotifier icon, menu, and desktop-host lifecycle |

Report bugs through [GitHub Issues](https://github.com/fossgap/open-meteo/issues),
including your distribution, desktop session, app version, and reproduction steps.
Contributions should pass `make check`, the native UI checks when relevant, and
package verification for packaging changes. See [CI and releases](ci-release.md)
for the complete validation workflow.
