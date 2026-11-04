<p align="center">
  <img src="resources/io.github.open_meteo.OpenMeteo.svg" width="128" height="128" alt="Open Meteo logo: sun behind a cloud">
</p>

<h1 align="center">Open Meteo</h1>

<p align="center">
  <a href="https://github.com/fossgap/open-meteo/actions/workflows/test-ci.yml"><img src="https://github.com/fossgap/open-meteo/actions/workflows/test-ci.yml/badge.svg" alt="Test CI"></a>
  <a href="https://github.com/fossgap/open-meteo/actions/workflows/release-ci.yml"><img src="https://github.com/fossgap/open-meteo/actions/workflows/release-ci.yml/badge.svg" alt="Release CI"></a>
  <a href="https://github.com/fossgap/open-meteo/releases"><img src="https://img.shields.io/github/v/release/fossgap/open-meteo" alt="Latest release"></a>
  <a href="https://github.com/fossgap/open-meteo/releases"><img src="https://img.shields.io/github/downloads/fossgap/open-meteo/total" alt="Download count"></a>
  <a href="docs/packaging.md"><img src="https://img.shields.io/badge/platform-Linux%20x86__64-yellowgreen" alt="Platform: Linux x86_64"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-GPL--3.0--only-blue.svg" alt="License"></a>
</p>

> Current weather, the next 24 hours, and the week ahead, with quick access from your Linux system tray. Built with Rust, GTK 4, and libadwaita, part of [FOSSGap](https://github.com/fossgap).

[Features](#features) · [Install](#install) · [Use](#use) · [Building from source](#building-from-source)

<p align="center">
  <img src="docs/preview.png" alt="Open Meteo showing the Paris weather forecast">
</p>

## Features

- Current conditions, an hourly temperature chart, and a seven-day forecast
- City and postal-code search, with five recent locations remembered
- Weather icon and temperature in one clickable tray item, or inside the GNOME clock
- Saved forecasts for offline use, with clear timestamps
- Celsius / km/h and Fahrenheit / mph, with a configurable refresh interval
- Native keyboard controls, narrow layouts, and desktop light/dark themes
- English, French, German, Spanish, Simplified Chinese, Hindi, and Arabic

## Install

Download the **Linux x86_64** package for your distribution and `SHA256SUMS` from
[Releases](https://github.com/fossgap/open-meteo/releases).
[Verify the checksum](docs/packaging.md#verify-downloads), then run the
matching command from your downloads folder:

```sh
# Debian / Ubuntu
sudo apt install ./open-meteo_*_amd64.deb

# Fedora
sudo dnf install ./open-meteo-[0-9]*.x86_64.rpm

# Arch Linux
sudo pacman -U ./open-meteo-[0-9]*-x86_64.pkg.tar.zst

# AppImage — portable
chmod +x Open_Meteo-*-x86_64.AppImage
./Open_Meteo-*-x86_64.AppImage
```

After installing a native package, launch **Open Meteo** from your applications
menu or run `open-meteo`.

Native packages require GTK 4.8+ and libadwaita 1.2+; package managers install the
dependencies. AppImage bundles those libraries and requires glibc 2.36+ (Debian
12+ / Ubuntu 24.04+). For FUSE errors, use
`APPIMAGE_EXTRACT_AND_RUN=1 ./Open_Meteo-*-x86_64.AppImage`.
See [package details](docs/packaging.md) for compatibility and local builds.

## Use

1. Choose **Find a location**, search for your city, and select a result.
2. Scroll the hourly forecast sideways or expand a day for more detail.
3. Choose **Main menu → Refresh interval** for updates every 5, 15, 30, or 60
   minutes, or **Manual only**. The default is 15 minutes; **Ctrl+R** refreshes now.

Closing the window keeps the app running when a tray or GNOME clock integration
is available. Use **Quit** to exit, or turn off **Keep running in system tray**.
On GNOME, choose **Main menu → GNOME clock setup**; a new extension installation
may require signing out and back in.

Run `open-meteo --demo` for an offline preview with sample weather and no changes
to saved settings. See the [usage guide](docs/usage.md) for GNOME setup, languages,
shortcuts, refresh behavior, and storage.

## Building from source

Install **Rust 1.85+**, then the native build dependencies. On Debian / Ubuntu:

```sh
sudo apt install build-essential pkg-config libgtk-4-dev libadwaita-1-dev
git clone https://github.com/fossgap/open-meteo.git
cd open-meteo
cargo run --locked
```

See [development](docs/development.md) for other distributions and tests,
[packaging](docs/packaging.md), and [CI and releases](docs/ci-release.md).
Report bugs or suggest improvements in [Issues](https://github.com/fossgap/open-meteo/issues).

## Data and privacy

Forecasts and search use [Open-Meteo](https://open-meteo.com/), with location data
from [GeoNames](https://www.geonames.org/). This is an independent client, not
affiliated with Open-Meteo. Public endpoints need no account or API key for
non-commercial use; the provider's [terms](https://open-meteo.com/en/terms) apply.

No telemetry or automatic location detection. Searches send your query to the
geocoding service; forecasts send the selected coordinates to the weather service.
See [data and storage details](docs/usage.md#data-storage-and-privacy).

## License

Licensed under [GNU GPL v3.0 only](LICENSE) (`GPL-3.0-only`). Weather data and
dependencies retain their own terms, including the vendored [ksni Unlicense](vendor/ksni/UNLICENSE).
