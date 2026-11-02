#!/usr/bin/env python3
"""Check the complete release inventory, checksums, metadata and packaged payloads."""
import argparse
import hashlib
import io
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]
APP_ID = "io.github.open_meteo.OpenMeteo"
REPOSITORY = "https://github.com/fossgap/open-meteo"
EXTENSION = "usr/share/gnome-shell/extensions/open-meteo-clock@fossgap.github.io"
PAYLOAD = {
    "usr/bin/open-meteo",
    f"usr/share/applications/{APP_ID}.desktop",
    f"usr/share/icons/hicolor/scalable/apps/{APP_ID}.svg",
    "usr/share/doc/open-meteo/README.md",
}
PAYLOAD.update(f"{EXTENSION}/{name}" for name in ("metadata.json", "extension.js", "stylesheet.css"))


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def run(*args):
    return subprocess.check_output(args, stderr=subprocess.PIPE)


def inventory(version):
    return {
        "deb": f"open-meteo_{version}_amd64.deb",
        "rpm": f"open-meteo-{version}-1.x86_64.rpm",
        "arch": f"open-meteo-{version}-1-x86_64.pkg.tar.zst",
        "appimage": f"Open_Meteo-{version}-x86_64.AppImage",
        "source": f"open-meteo-{version}.tar.gz",
        "recipe": "PKGBUILD",
    }


def verify_inventory(directory, files):
    expected = set(files.values())
    actual = {path.name for path in directory.iterdir()} - {"SHA256SUMS"}
    require(actual == expected, f"Unexpected release inventory: missing={expected - actual}, extra={actual - expected}")
    for name in expected:
        path = directory / name
        require(path.is_file() and not path.is_symlink() and path.stat().st_size > 0, f"Not a regular nonempty artifact: {name}")


def verify_checksums(directory, files):
    seen = set()
    manifest = directory / "SHA256SUMS"
    require(manifest.is_file() and not manifest.is_symlink(), "Missing regular SHA256SUMS file")
    for line in manifest.read_text().splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  ([A-Za-z0-9_.+-]+)", line)
        require(match is not None, f"Invalid checksum entry: {line}")
        checksum, name = match.groups()
        require(name in files.values() and name not in seen, f"Unexpected/duplicate checksum: {name}")
        require(digest((directory / name).read_bytes()) == checksum, f"Checksum mismatch: {name}")
        seen.add(name)
    require(seen == set(files.values()), "SHA256SUMS does not cover every artifact")


def tar_contents(data):
    result = {}
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:*") as archive:
        for member in archive:
            path = PurePosixPath(member.name)
            require(not path.is_absolute() and ".." not in path.parts, f"Unsafe archive path: {member.name}")
            require(member.isdir() or member.isfile(), f"Unexpected archive link/device: {member.name}")
            if member.isfile():
                name = str(path)
                require(name not in result, f"Duplicate archive file: {name}")
                result[name] = (archive.extractfile(member).read(), member.mode)
    return result


def verify_payload(contents, label):
    require(PAYLOAD <= contents.keys(), f"Missing {label} payload: {PAYLOAD - contents.keys()}")
    binary, mode = contents["usr/bin/open-meteo"]
    require(binary.startswith(b"\x7fELF") and mode & 0o111, f"Missing executable ELF in {label}")
    desktop = contents[f"usr/share/applications/{APP_ID}.desktop"][0].decode()
    require("Exec=open-meteo\n" in desktop and f"Icon={APP_ID}\n" in desktop, f"Invalid desktop integration in {label}")
    for name in ("metadata.json", "extension.js", "stylesheet.css"):
        require(contents[f"{EXTENSION}/{name}"][0] == (ROOT / "resources/gnome-extension" / name).read_bytes(),
                f"GNOME extension differs from source in {label}: {name}")
    with tempfile.TemporaryDirectory(prefix="open-meteo-desktop-") as temp:
        path = Path(temp) / f"{APP_ID}.desktop"
        path.write_text(desktop)
        run("desktop-file-validate", str(path))
    return digest(binary)


def executable_sections(binary):
    """linuxdeploy changes ELF runtime paths; executable code/data must stay equal."""
    with tempfile.TemporaryDirectory(prefix="open-meteo-elf-") as temp:
        root = Path(temp)
        source = root / "binary"
        source.write_bytes(binary)
        sections = [".text", ".rodata", ".note.gnu.build-id"]
        options = [option for name in sections for option in ["--dump-section", f"{name}={root / name}"]]
        run("objcopy", *options, str(source), str(root / "copy"))
        for name in sections:
            require((root / name).is_file() and (root / name).stat().st_size > 0, f"Missing ELF section {name}")
        return {name: digest((root / name).read_bytes()) for name in sections}


def verify_artifacts(directory, files, version):
    deb = str(directory / files["deb"])
    fields = run("dpkg-deb", "-f", deb).decode()
    for field in ["Package: open-meteo", f"Version: {version}", "Architecture: amd64", f"Homepage: {REPOSITORY}"]:
        require(field in fields.splitlines(), f"DEB metadata missing {field}")
    depends = next((line for line in fields.splitlines() if line.startswith("Depends:")), "")
    for name in ["libgtk-4-1", "libadwaita-1-0", "ca-certificates", "adwaita-icon-theme"]:
        require(name in depends, f"DEB dependency missing: {name}")
    deb_contents = tar_contents(run("dpkg-deb", "--fsys-tarfile", deb))
    binary_hash = verify_payload(deb_contents, "DEB")
    code_signature = executable_sections(deb_contents["usr/bin/open-meteo"][0])

    rpm = str(directory / files["rpm"])
    fields = run("rpm", "-qp", "--qf", "%{NAME}\n%{VERSION}\n%{ARCH}\n%{URL}\n", rpm).decode().splitlines()
    require(fields == ["open-meteo", version, "x86_64", REPOSITORY], "RPM identity/version mismatch")
    names = {name.lstrip("/") for name in run("rpm", "-qpl", rpm).decode().splitlines()}
    require(PAYLOAD <= names, "RPM payload is incomplete")
    requires = run("rpm", "-qp", "--requires", rpm).decode().splitlines()
    for name in ["gtk4", "libadwaita", "ca-certificates", "adwaita-icon-theme"]:
        require(any(line == name or line.startswith(name + " ") for line in requires), f"RPM dependency missing: {name}")
    payload = run("rpm2cpio", rpm)
    binary = subprocess.run(["cpio", "--extract", "--to-stdout", "./usr/bin/open-meteo"], input=payload, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True).stdout
    require(digest(binary) == binary_hash, "RPM executable differs from DEB")
    for name in ("metadata.json", "extension.js", "stylesheet.css"):
        data = subprocess.run(["cpio", "--extract", "--to-stdout", f"./{EXTENSION}/{name}"], input=payload,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True).stdout
        require(data == (ROOT / "resources/gnome-extension" / name).read_bytes(), f"RPM extension differs: {name}")

    arch = tar_contents(run("zstd", "-dc", str(directory / files["arch"])))
    require(".PKGINFO" in arch, "Missing Arch package metadata")
    fields = arch[".PKGINFO"][0].decode().splitlines()
    for field in ["pkgname = open-meteo", f"pkgver = {version}-1", "arch = x86_64", f"url = {REPOSITORY}", "depend = gtk4>=4.8", "depend = libadwaita>=1.2", "depend = ca-certificates", "depend = adwaita-icon-theme"]:
        require(field in fields, f"Arch metadata missing {field}")
    require(verify_payload(arch, "Arch") == binary_hash, "Arch executable differs from DEB")

    with tempfile.TemporaryDirectory(prefix="open-meteo-appimage-") as temp:
        root = Path(temp)
        appimage = root / "OpenMeteo.AppImage"
        shutil.copyfile(directory / files["appimage"], appimage)
        appimage.chmod(0o755)  # Actions artifact transport does not retain modes.
        subprocess.run([str(appimage), "--appimage-extract"], cwd=root, stdout=subprocess.DEVNULL, check=True)
        appdir = root / "squashfs-root"
        contents = {name: ((appdir / name).read_bytes(), (appdir / name).stat().st_mode) for name in PAYLOAD if (appdir / name).is_file()}
        verify_payload(contents, "AppImage")
        require(executable_sections(contents["usr/bin/open-meteo"][0]) == code_signature, "AppImage executable code/data/build ID differs from DEB")
        require((appdir / "AppRun").is_file(), "Missing AppImage launcher")
        for library in ["libgtk-4.so*", "libadwaita-1.so*", "libfribidi.so*", "libharfbuzz.so*", "libwayland-client.so*"]:
            require(any((appdir / "usr/lib").glob(library)), f"AppImage is missing {library}")
        hook = (appdir / "apprun-hooks/linuxdeploy-plugin-gtk.sh").read_text()
        require(not re.search(r"^export (GTK_THEME|GDK_BACKEND)=", hook, re.M), "AppImage overrides native theme/display selection")
        for resource in ["usr/share/mime/mime.cache", "usr/etc/fonts/fonts.conf", "usr/share/icons/Adwaita/index.theme", "apprun-hooks/zz-open-meteo.sh"]:
            require((appdir / resource).is_file(), f"AppImage resource missing: {resource}")
        for icon in ["weather-few-clouds-symbolic.svg", "weather-clear-symbolic.svg", "weather-showers-symbolic.svg"]:
            require(any((appdir / "usr/share/icons/Adwaita").rglob(icon)), f"AppImage weather icon missing: {icon}")
        runtime = (appdir / "apprun-hooks/zz-open-meteo.sh").read_text()
        require('export GIO_MODULE_DIR="$APPDIR/usr/lib/gio/modules"' in runtime, "AppImage must isolate its GIO modules")

    source = tar_contents((directory / files["source"]).read_bytes())
    prefix = f"open-meteo-{version}/"
    required = {prefix + name for name in ["Cargo.toml", "Cargo.lock", "src/main.rs", "tests/ui_smoke.py", "tests/fixtures/forecast.json", "scripts/package.sh", "scripts/verify-packages.py", "packaging/arch/PKGBUILD", "packaging/rpm/open-meteo.spec", "Dockerfile", "Makefile", "README.md", ".github/workflows/test-ci.yml", ".github/workflows/release-ci.yml"]}
    require(required <= source.keys(), f"Source archive missing files: {required - source.keys()}")
    for absolute in [ROOT / "src/gnome_setup.rs", ROOT / "tests/gnome_setup.py",
                     ROOT / "src/tray.rs", ROOT / "tests/tray_panel.py", ROOT / "tests/tray_label.py", ROOT / "scripts/check-tray.sh",
                     ROOT / "tests/gnome_indicator.js", ROOT / "tests/gnome_appimage.js", ROOT / "scripts/check-gnome.sh",
                     *(p for p in (ROOT / "vendor/ksni").rglob("*") if p.is_file()),
                     *(ROOT / "resources/locales").glob("*.json"), *(ROOT / "resources/gnome-extension").glob("*")]:
        path = absolute.relative_to(ROOT)
        require(prefix + str(path) in source, f"Source archive missing {path}")
        require(source[prefix + str(path)][0] == (ROOT / path).read_bytes(), f"Source archive differs: {path}")
    require(all(name.startswith(prefix) for name in source), "Source archive must have a single versioned root")
    require(f'version = "{version}"' in source[prefix + "Cargo.toml"][0].decode().splitlines(), "Source version mismatch")
    recipe = (directory / files["recipe"]).read_text()
    require(f"pkgver={version}" in recipe.splitlines(), "PKGBUILD version mismatch")
    require(f"sha256sums=('{digest((directory / files['source']).read_bytes())}')" in recipe, "PKGBUILD source checksum mismatch")
    print(f"Verified DEB, RPM, Arch, AppImage, source and PKGBUILD for {version}; executable SHA-256 {binary_hash}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--checksums-only", action="store_true", help="Verify inventory/checksums without extraction tools")
    mode.add_argument("--write-checksums", action="store_true", help="Create SHA256SUMS after inspecting all artifacts")
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    version = re.search(r'^version = "([^"]+)"$', (ROOT / "Cargo.toml").read_text(), re.M).group(1)
    files = inventory(version)
    directory = args.directory.resolve()
    verify_inventory(directory, files)
    if not args.write_checksums:
        verify_checksums(directory, files)
    if not args.checksums_only:
        verify_artifacts(directory, files, version)
    if args.write_checksums:
        (directory / "SHA256SUMS").write_text("".join(f"{digest((directory / name).read_bytes())}  {name}\n" for name in sorted(files.values())))
        verify_checksums(directory, files)
    print("Release inventory and SHA-256 checksums verified.")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError, tarfile.TarError) as error:
        raise SystemExit(f"Package verification failed: {error}") from error
