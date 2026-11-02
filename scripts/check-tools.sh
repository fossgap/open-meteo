#!/bin/sh
# Source from a check script after changing to the repository root.
if [ -z "${OPEN_METEO_CHECK_TOOLS_DIR:-}" ]; then
    cargo build --locked -p open-meteo-checks --features desktop
    OPEN_METEO_CHECK_TOOLS_DIR="$PWD/target/debug"
    export OPEN_METEO_CHECK_TOOLS_DIR
fi
test -x "$OPEN_METEO_CHECK_TOOLS_DIR/desktop-tests"
