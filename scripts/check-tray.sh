#!/bin/sh
# A real XFCE StatusNotifier host in a disposable display, bus and settings tree.
set -eu
cd "$(dirname "$0")/.."
command -v xfce4-panel >/dev/null
test_runtime=
cleanup() {
    [ -n "$test_runtime" ] || return 0
    for mount in doc gvfs; do
        if mountpoint -q "$test_runtime/$mount"; then
            fusermount3 -u "$test_runtime/$mount" || true
        fi
    done
    rm -rf "$test_runtime"
}
trap cleanup EXIT HUP INT TERM
export ATSPI_DBUS_IMPLEMENTATION=dbus-daemon
export OPEN_METEO_TRAY_TEST_SESSION=1
for theme in Adwaita Adwaita:dark; do
    test_runtime=$(mktemp -d "${TMPDIR:-/tmp}/open-meteo-tray-runtime.XXXXXX")
    export XDG_RUNTIME_DIR="$test_runtime" OPEN_METEO_PANEL_THEME="$theme"
    timeout 90s dbus-run-session -- xvfb-run -a -s '-screen 0 960x1100x24' \
        "${PYTHON:-/usr/bin/python3}" tests/tray_panel.py
    cleanup
    test_runtime=
done
