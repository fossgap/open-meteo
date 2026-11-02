#!/bin/sh
# Sourced by AppRun after the GTK hook. Use modules from the bundled GLib.
export LD_LIBRARY_PATH="$APPDIR/usr/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export GIO_MODULE_DIR="$APPDIR/usr/lib/gio/modules"
export GIO_EXTRA_MODULES="$GIO_MODULE_DIR"
if [ -z "${FONTCONFIG_FILE:-}" ] && [ -z "${FONTCONFIG_PATH:-}" ] && [ ! -f /etc/fonts/fonts.conf ]; then
    export FONTCONFIG_PATH="$APPDIR/usr/etc/fonts"
    export FONTCONFIG_FILE="$FONTCONFIG_PATH/fonts.conf"
fi
