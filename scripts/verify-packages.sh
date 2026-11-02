#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
if [ -n "${OPEN_METEO_CHECK_TOOLS_DIR:-}" ]; then
    exec "$OPEN_METEO_CHECK_TOOLS_DIR/verify-packages" "$@"
fi
exec cargo run --quiet --locked -p open-meteo-checks --bin verify-packages -- "$@"
