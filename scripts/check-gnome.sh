#!/bin/sh
# GNOME 50's headless test runner isolates settings, extensions and session bus.
set -eu
cd "$(dirname "$0")/.."
command -v gnome-shell-test-tool >/dev/null
test_root=$(mktemp -d "${TMPDIR:-/tmp}/open-meteo-gnome.XXXXXX")
cleanup() {
    for mount in doc gvfs; do
        if mountpoint -q "$test_root/runtime/$mount"; then
            fusermount3 -u "$test_root/runtime/$mount" || true
        fi
    done
    rm -rf "$test_root"
}
trap cleanup EXIT HUP INT TERM
mkdir -m700 "$test_root/runtime"
python3 - "$test_root/extension.zip" <<'PY'
from pathlib import Path
import sys
import zipfile
with zipfile.ZipFile(sys.argv[1], 'w') as archive:
    for path in Path('resources/gnome-extension').iterdir():
        archive.write(path, path.name)
PY
export XDG_RUNTIME_DIR="$test_root/runtime" LIBGL_ALWAYS_SOFTWARE=1 ATSPI_DBUS_IMPLEMENTATION=dbus-daemon
export OPEN_METEO_GNOME_SCREENSHOT="$PWD/target/ci/gnome-clock.png"
export OPEN_METEO_GNOME_INDICATOR_SCREENSHOT="$PWD/target/ci/gnome-indicator.png"
export OPEN_METEO_GNOME_APPIMAGE_SCREENSHOT="$PWD/target/ci/gnome-appimage.png"
mkdir -p target/ci
# The nested shell must expand the address created by dbus-run-session.
# shellcheck disable=SC2016
timeout 120s dbus-run-session -- sh -c '
    export DBUS_SYSTEM_BUS_ADDRESS="$DBUS_SESSION_BUS_ADDRESS"
    exec gnome-shell-test-tool --headless --disable-animations --extension "$1" "$2"
' sh "$test_root/extension.zip" "$PWD/tests/gnome_clock.js"
python3 - "$test_root/indicator.zip" <<'PY'
from pathlib import Path
import sys
import zipfile
root = Path('/usr/share/gnome-shell/extensions/appindicatorsupport@rgcjonas.gmail.com')
assert root.is_dir(), 'Install gnome-shell-extension-appindicator in the test image'
with zipfile.ZipFile(sys.argv[1], 'w') as archive:
    for source in root.rglob('*'):
        if source.is_file():
            archive.write(source, str(source.relative_to(root)))
PY
# shellcheck disable=SC2016
timeout 120s dbus-run-session -- sh -c '
    export DBUS_SYSTEM_BUS_ADDRESS="$DBUS_SESSION_BUS_ADDRESS"
    exec gnome-shell-test-tool --headless --disable-animations --extension "$1" "$2"
' sh "$test_root/indicator.zip" "$PWD/tests/gnome_indicator.js"
if [ -n "${OPEN_METEO_TEST_BINARY:-}" ]; then
    export OPEN_METEO_TEST_BINARY
    # shellcheck disable=SC2016
    timeout 120s dbus-run-session -- sh -c '
        export DBUS_SYSTEM_BUS_ADDRESS="$DBUS_SESSION_BUS_ADDRESS"
        exec gnome-shell-test-tool --headless --disable-animations --extension "$1" "$2"
    ' sh "$test_root/indicator.zip" "$PWD/tests/gnome_appimage.js"
fi
