#!/bin/sh
# Run offline native controls and tray checks on an isolated display/session bus.
set -eu
cd "$(dirname "$0")/.."
export GDK_BACKEND=x11 GSK_RENDERER=cairo GIO_USE_VFS=local
exec timeout 180s dbus-run-session -- xvfb-run -a -s '-screen 0 1280x1200x24' \
    "${PYTHON:-/usr/bin/python3}" tests/ui_smoke.py
