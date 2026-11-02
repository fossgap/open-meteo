#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
mode=${1:-check}
case "$mode" in check|release) ;; *) exit 2;; esac
make check
shellcheck scripts/*.sh tests/package-install/*.sh packaging/*.sh
bash -n packaging/arch/PKGBUILD
desktop-file-validate resources/io.github.open_meteo.OpenMeteo.desktop
cargo build --locked --workspace --features open-meteo-checks/desktop
export OPEN_METEO_CHECK_TOOLS_DIR="$PWD/target/debug"
export OPEN_METEO_TEST_OUTPUT_DIR="$PWD/target/ci/source-ui"
sh scripts/check-ui.sh
sh scripts/check-tray.sh
if [ "$mode" = release ]; then
    cargo build --release --locked --workspace --features open-meteo-checks/desktop
    export OPEN_METEO_CHECK_TOOLS_DIR="$PWD/target/release"
    export OPEN_METEO_BINARY="$PWD/target/release/open-meteo"
    for format in deb rpm arch arch-source; do
        sh scripts/package.sh "$format"
    done
    sh scripts/fetch-appimage-tools.sh
    export PATH="$PWD/target/appimage-tools:$PATH" APPIMAGE_EXTRACT_AND_RUN=1
    sh scripts/package.sh appimage
    mkdir dist/release
    cp dist/*.deb dist/*.rpm dist/*.pkg.tar.zst dist/*.AppImage dist/arch/*.tar.gz dist/arch/PKGBUILD dist/release/
    sh scripts/verify-packages.sh --write-checksums dist/release
    appimage=$(find dist/release -name '*.AppImage')
    export OPEN_METEO_TEST_BINARY="$PWD/$appimage"
    export OPEN_METEO_TEST_OUTPUT_DIR="$PWD/target/ci/appimage-ui"
    sh scripts/check-ui.sh
    sh scripts/check-tray.sh
fi
