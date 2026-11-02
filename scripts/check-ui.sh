#!/bin/sh
# Run offline native controls and tray checks on an isolated display/session bus.
set -eu
cd "$(dirname "$0")/.."
. scripts/check-tools.sh
export OPEN_METEO_TEST_SESSION=1
export GDK_BACKEND=x11 GSK_RENDERER=cairo GIO_USE_VFS=local
export ATSPI_DBUS_IMPLEMENTATION=dbus-daemon
test_runtime=$(mktemp -d "${TMPDIR:-/tmp}/open-meteo-ui-runtime.XXXXXX")
trap 'rm -rf "$test_runtime"' EXIT HUP INT TERM
export XDG_RUNTIME_DIR="$test_runtime"
timeout 60s dbus-run-session -- "$OPEN_METEO_CHECK_TOOLS_DIR/desktop-tests" gnome-setup
timeout 240s dbus-run-session -- xvfb-run -a -s '-screen 0 1280x1200x24' \
    "$OPEN_METEO_CHECK_TOOLS_DIR/desktop-tests" ui
