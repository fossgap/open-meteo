#!/bin/sh
# Explicit CI/bootstrap download; ordinary package.sh never downloads tools.
set -eu
cd "$(dirname "$0")/.."
[ "$(uname -m)" = x86_64 ] || { echo 'Pinned AppImage tools require x86_64.' >&2; exit 1; }
cache="$PWD/target/appimage-tools"
mkdir -p "$cache"
fetch() {
    destination=$1
    checksum=$2
    url=$3
    if [ ! -f "$destination" ]; then
        curl --fail --location --retry 3 "$url" -o "$destination.download"
        printf '%s  %s\n' "$checksum" "$destination.download" | sha256sum -c -
        mv "$destination.download" "$destination"
    fi
    printf '%s  %s\n' "$checksum" "$destination" | sha256sum -c -
    chmod 755 "$destination"
}
fetch "$cache/linuxdeploy" c20cd71e3a4e3b80c3483cef793cda3f4e990aca14014d23c544ca3ce1270b4d \
    https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20251107-1/linuxdeploy-x86_64.AppImage
fetch "$cache/linuxdeploy-plugin-gtk.sh" b0f4cbc684a0103a9651f0955b635eaea0096b3a66c0f5a2c2aa337960375171 \
    https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/7a3fbc31a9e5075073ff8790f26effbac5f84453/linuxdeploy-plugin-gtk.sh
echo "AppImage tools verified in $cache"
