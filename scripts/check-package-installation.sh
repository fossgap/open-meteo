#!/bin/sh
# Install only inside disposable containers; never install on the host.
set -eu
cd "$(dirname "$0")/.."
if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
    echo 'Usage: check-package-installation.sh debian|kali|fedora|arch|appimage [ARTIFACT_DIRECTORY]' >&2
    exit 2
fi
target=$1
case "$target" in
    debian) image=debian:bookworm-slim; format=deb ;;
    kali) image=kalilinux/kali-rolling:latest; format=deb ;;
    fedora) image=registry.fedoraproject.org/fedora:44; format=rpm ;;
    arch) image=archlinux:base; format=arch ;;
    appimage) image=ubuntu:24.04; format=appimage ;;
    *) echo "Unknown installation target: $target" >&2; exit 2 ;;
esac
[ "$(uname -m)" = x86_64 ] || { echo 'Release installation checks require x86_64.' >&2; exit 1; }
artifacts=$(realpath "${2:-dist/release}")
check_tools=$(realpath "${OPEN_METEO_CHECK_TOOLS_DIR:-target/check-tools}")
test -x "$check_tools/desktop-tests"
OPEN_METEO_CHECK_TOOLS_DIR="$check_tools" sh scripts/verify-packages.sh --checksums-only "$artifacts"
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)
results_dir="$PWD/target/package-installation/$target"
mkdir -p "$results_dir"
test_uid=$(id -u)
[ "$test_uid" -ne 0 ] || test_uid=1000
docker pull "$image"
docker image inspect "$image" --format '{{json .RepoTags}} {{json .RepoDigests}} {{.Id}}'
container=$(docker create --tmpfs /tmp:rw,exec --shm-size=1g \
    --env OPEN_METEO_INSTALL_TEST_CONTAINER=1 --env DEBIAN_FRONTEND=noninteractive \
    --env OPEN_METEO_TEST_UID="$test_uid" \
    --env OPEN_METEO_CHECK_TOOLS_DIR=/tools \
    --volume "$check_tools:/tools:ro" \
    --volume "$artifacts:/artifacts:ro" \
    --volume "$PWD/scripts:/workspace/scripts:ro" \
    --volume "$PWD/tests:/workspace/tests:ro" \
    --volume "$PWD/resources:/workspace/resources:ro" \
    --volume "$results_dir:/results" \
    "$image" sh /workspace/tests/package-install/install.sh "$format" "$version")
trap 'docker rm -f "$container" >/dev/null 2>&1 || true' EXIT HUP INT TERM
result=0
docker start -a "$container" || result=$?
[ "$result" -eq 0 ] || exit "$result"
[ "$(docker inspect --format '{{.State.ExitCode}}' "$container")" -eq 0 ]
