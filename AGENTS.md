# Repository Guidelines

## Project Structure & Module Organization

Open Meteo is a Rust 2024 Linux desktop app using GTK 4 and libadwaita.

- `src/`: window/actions (`ui.rs`), presentation (`components.rs`, `style.css`), weather API/model, storage, translations, tray, and GNOME integration.
- `resources/`: launcher, SVG icon, JSON translation catalogs, and GNOME Shell extension.
- `tools/checks/`: Rust package verification, desktop automation, and integrity tests. `tests/` contains GNOME JavaScript tests and offline fixtures. App unit tests live beside implementation code.
- `scripts/`, `packaging/`, `.github/workflows/`: verification, package recipes, and CI. Generated builds/packages belong in `target/` and `dist/`.
- `vendor/ksni/`: patched tray dependency; document changes in `OPEN-METEO.md` and retain its license.

## Build, Test, and Development Commands

Install Rust 1.85+, GTK 4.8+, libadwaita 1.2+, a C toolchain, and `pkg-config`; see `docs/development.md` for distribution commands.

- `cargo run --locked`: launch normally; append `-- --demo` for offline sample weather.
- `make build`: build the release binary.
- `make check`: check formatting, run Clippy with warnings denied, and execute Rust tests.
- `cargo build --locked && make check-ui`: build and exercise native controls under Xvfb/private D-Bus; requires the test dependencies in `docs/development.md`.
- `sh scripts/check-tray.sh`: verify the real XFCE panel and native label protocol.
- `cargo test --locked -p open-meteo-checks`: run release-integrity tests.
- `docker build -t open-meteo-ci . && sh scripts/ci.sh check`: reproduce container CI. Use `sh scripts/ci.sh release` to build and verify packages in a fresh output directory.

See `docs/ci-release.md` for GNOME and installation-matrix checks.

## Coding Style & Naming Conventions

Use `cargo fmt --all`, four-space Rust indentation, `snake_case` functions/modules, `PascalCase` types, and `SCREAMING_SNAKE_CASE` constants. Keep GTK operations on the UI thread and blocking network work off it. Follow surrounding JavaScript style; shell scripts must pass ShellCheck.

Keep translation catalogs complete and preserve named placeholders such as `{temperature}`. Preserve one horizontal weather-icon-plus-temperature tray item with a shared click target.

## Testing Guidelines

Use descriptive Rust `#[test]` names. Add regression coverage for changed behavior; no numeric coverage threshold is configured. Use offline fixtures and temporary settings. Run desktop checks through the isolation scripts. Live API checks are optional: `cargo run --locked -- --check-api`.

## Commit & Pull Request Guidelines

History uses concise imperative subjects, such as “Use one clickable weather and temperature indicator”; no prefix convention is required. Keep commits focused. Describe the problem, resulting behavior, validation, and limitations in pull requests. Link relevant issues and include actual screenshots for visible changes, covering light/dark themes and affected translations when applicable.
