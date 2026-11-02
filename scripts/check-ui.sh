#!/bin/sh
# Run offline native controls and tray checks on an isolated display/session bus.
set -eu
cd "$(dirname "$0")/.."
export GDK_BACKEND=x11 GSK_RENDERER=cairo GIO_USE_VFS=local
export ATSPI_DBUS_IMPLEMENTATION=dbus-daemon
test_runtime=$(mktemp -d "${TMPDIR:-/tmp}/open-meteo-ui-runtime.XXXXXX")
trap 'rm -rf "$test_runtime"' EXIT HUP INT TERM
export XDG_RUNTIME_DIR="$test_runtime"
timeout 240s dbus-run-session -- xvfb-run -a -s '-screen 0 1280x1200x24' \
    "${PYTHON:-/usr/bin/python3}" tests/ui_smoke.py
