#!/bin/sh
# Reproduce GitHub CI locally; package installation/publication are separate.
set -eu
cd "$(dirname "$0")/.."
mode=${1:-check}
case "$mode" in check|release) ;; *) echo 'Usage: scripts/ci.sh check|release' >&2; exit 2;; esac
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)
if [ "$mode" = release ] && [ "${GITHUB_REF_TYPE:-}" = tag ]; then
    [ "${GITHUB_REF_NAME:-}" = "v$version" ] || { echo 'Release tag must match Cargo.toml version.' >&2; exit 1; }
fi
output=${OPEN_METEO_CI_OUTPUT:-dist/release}
if [ "$mode" = release ] && [ -e "$output" ]; then
    echo "Choose a new OPEN_METEO_CI_OUTPUT directory: $output" >&2
    exit 1
fi
set -- docker create --tmpfs /tmp:rw,exec --shm-size=1g
if [ -n "${OPEN_METEO_CI_TARGET_DIR:-}" ]; then
    mkdir -p "$OPEN_METEO_CI_TARGET_DIR"
    build_target=$(realpath "$OPEN_METEO_CI_TARGET_DIR")
    set -- "$@" --volume "$build_target:/workspace/target"
fi
container=$("$@" "${OPEN_METEO_CI_IMAGE:-open-meteo-ci}" sh scripts/ci-inside.sh "$mode")
trap 'docker rm -f "$container" >/dev/null 2>&1 || true' EXIT HUP INT TERM
result=0
docker start -a "$container" || result=$?
mkdir -p target/ci
docker cp "$container:/workspace/target/ci/." target/ci/ 2>/dev/null || true
[ "$result" -eq 0 ] || exit "$result"
[ "$(docker inspect --format '{{.State.ExitCode}}' "$container")" -eq 0 ]
if [ "$mode" = release ]; then
    mkdir -p "$output"
    docker cp "$container:/workspace/dist/release/." "$output/"
    python3 scripts/verify-packages.py --checksums-only "$output"
fi
