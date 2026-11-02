//! Release inventory, payload, license and executable integrity checks.
use crate::process::{input, run, text};
use anyhow::{Context, Result, ensure};
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Cursor, Read},
    os::unix::fs::PermissionsExt,
    path::{Component, Path},
    process::{Command, Stdio},
};

pub const APP_ID: &str = "io.github.open_meteo.OpenMeteo";
pub const REPOSITORY: &str = "https://github.com/fossgap/open-meteo";
pub const EXTENSION: &str = "usr/share/gnome-shell/extensions/open-meteo-clock@fossgap.github.io";
pub const LICENSE_FILES: [(&str, &str); 3] = [
    ("usr/share/licenses/open-meteo/LICENSE", "LICENSE"),
    (
        "usr/share/licenses/open-meteo/ksni-UNLICENSE",
        "vendor/ksni/UNLICENSE",
    ),
    ("usr/share/doc/open-meteo/copyright", "packaging/copyright"),
];
pub type Contents = BTreeMap<String, (Vec<u8>, u32)>;
pub type Inventory = BTreeMap<&'static str, String>;

pub fn version(root: &Path) -> Result<String> {
    fs::read_to_string(root.join("Cargo.toml"))?
        .lines()
        .find_map(|line| {
            line.strip_prefix("version = \"")
                .and_then(|s| s.strip_suffix('"'))
                .map(str::to_owned)
        })
        .context("Cargo.toml package version")
}

pub fn inventory(version: &str) -> Inventory {
    BTreeMap::from([
        ("deb", format!("open-meteo_{version}_amd64.deb")),
        ("rpm", format!("open-meteo-{version}-1.x86_64.rpm")),
        ("arch", format!("open-meteo-{version}-1-x86_64.pkg.tar.zst")),
        ("appimage", format!("Open_Meteo-{version}-x86_64.AppImage")),
        ("source", format!("open-meteo-{version}.tar.gz")),
        ("recipe", "PKGBUILD".into()),
    ])
}

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn verify_inventory(directory: &Path, files: &Inventory) -> Result<()> {
    let expected: BTreeSet<_> = files.values().cloned().collect();
    let actual = fs::read_dir(directory)?
        .map(|entry| Ok(entry?.file_name().to_string_lossy().into_owned()))
        .collect::<Result<BTreeSet<_>>>()?
        .into_iter()
        .filter(|n| n != "SHA256SUMS")
        .collect::<BTreeSet<_>>();
    ensure!(
        actual == expected,
        "Unexpected release inventory: expected {expected:?}, found {actual:?}"
    );
    for name in &expected {
        let metadata = fs::symlink_metadata(directory.join(name))?;
        ensure!(
            metadata.is_file() && metadata.len() > 0,
            "Not a regular nonempty artifact: {name}"
        );
    }
    Ok(())
}

pub fn verify_checksums(directory: &Path, files: &Inventory) -> Result<()> {
    let manifest = directory.join("SHA256SUMS");
    ensure!(
        fs::symlink_metadata(&manifest)?.is_file(),
        "Missing regular SHA256SUMS file"
    );
    let expected: BTreeSet<_> = files.values().map(String::as_str).collect();
    let mut seen = BTreeSet::new();
    for line in fs::read_to_string(manifest)?.lines() {
        let (checksum, name) = line.split_once("  ").context("Invalid checksum entry")?;
        ensure!(
            checksum.len() == 64
                && checksum
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                && !name.is_empty()
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.+-".contains(&b)),
            "Invalid checksum entry: {line}"
        );
        ensure!(
            expected.contains(name) && seen.insert(name.to_owned()),
            "Unexpected/duplicate checksum: {name}"
        );
        ensure!(
            digest(&fs::read(directory.join(name))?) == checksum,
            "Checksum mismatch: {name}"
        );
    }
    ensure!(
        seen.iter().map(String::as_str).collect::<BTreeSet<_>>() == expected,
        "SHA256SUMS does not cover every artifact"
    );
    Ok(())
}

pub fn write_checksums(directory: &Path, files: &Inventory) -> Result<()> {
    let mut names: Vec<_> = files.values().collect();
    names.sort();
    let mut manifest = String::new();
    for name in names {
        manifest.push_str(&format!(
            "{}  {name}\n",
            digest(&fs::read(directory.join(name))?)
        ));
    }
    fs::write(directory.join("SHA256SUMS"), manifest)?;
    Ok(())
}

pub fn tar_contents(bytes: &[u8]) -> Result<Contents> {
    let reader: Box<dyn Read + '_> = if bytes.starts_with(&[0x1f, 0x8b]) {
        Box::new(GzDecoder::new(Cursor::new(bytes)))
    } else {
        Box::new(Cursor::new(bytes))
    };
    let mut archive = tar::Archive::new(reader);
    let mut contents = Contents::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        ensure!(
            !path.is_absolute()
                && path
                    .components()
                    .all(|c| matches!(c, Component::Normal(_) | Component::CurDir)),
            "Unsafe archive path: {}",
            path.display()
        );
        let kind = entry.header().entry_type();
        ensure!(
            kind.is_file() || kind.is_dir(),
            "Unexpected archive link/device: {}",
            path.display()
        );
        if kind.is_file() {
            let name = path
                .components()
                .filter_map(|c| match c {
                    Component::Normal(s) => Some(s.to_string_lossy()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("/");
            let mode = entry.header().mode()?;
            let mut data = Vec::new();
            entry.read_to_end(&mut data)?;
            ensure!(
                contents.insert(name.clone(), (data, mode)).is_none(),
                "Duplicate archive file: {name}"
            );
        }
    }
    Ok(contents)
}

pub fn payload_names() -> BTreeSet<String> {
    let mut names = BTreeSet::from([
        "usr/bin/open-meteo".into(),
        format!("usr/share/applications/{APP_ID}.desktop"),
        format!("usr/share/icons/hicolor/scalable/apps/{APP_ID}.svg"),
        "usr/share/doc/open-meteo/README.md".into(),
    ]);
    names.extend(LICENSE_FILES.map(|(installed, _)| installed.to_owned()));
    names.extend(
        ["metadata.json", "extension.js", "stylesheet.css"]
            .map(|name| format!("{EXTENSION}/{name}")),
    );
    names
}

pub fn verify_licenses(root: &Path, contents: &Contents, label: &str) -> Result<()> {
    for (installed, source) in LICENSE_FILES {
        let data = &contents
            .get(installed)
            .with_context(|| format!("Missing license file in {label}: {installed}"))?
            .0;
        ensure!(
            *data == fs::read(root.join(source))?,
            "License file differs in {label}: {installed}"
        );
    }
    Ok(())
}

pub fn verify_payload(root: &Path, contents: &Contents, label: &str) -> Result<String> {
    for name in payload_names() {
        ensure!(
            contents.contains_key(&name),
            "Missing {label} payload: {name}"
        );
    }
    verify_licenses(root, contents, label)?;
    let (binary, mode) = &contents["usr/bin/open-meteo"];
    ensure!(
        binary.starts_with(b"\x7fELF") && mode & 0o111 != 0,
        "Missing executable ELF in {label}"
    );
    let desktop = String::from_utf8(
        contents[&format!("usr/share/applications/{APP_ID}.desktop")]
            .0
            .clone(),
    )?;
    ensure!(
        desktop.contains("Exec=open-meteo\n") && desktop.contains(&format!("Icon={APP_ID}\n")),
        "Invalid desktop integration in {label}"
    );
    for name in ["metadata.json", "extension.js", "stylesheet.css"] {
        ensure!(
            contents[&format!("{EXTENSION}/{name}")].0
                == fs::read(root.join("resources/gnome-extension").join(name))?,
            "GNOME extension differs in {label}: {name}"
        );
    }
    let temp = tempfile::tempdir()?;
    let path = temp.path().join(format!("{APP_ID}.desktop"));
    fs::write(&path, desktop)?;
    run(Command::new("desktop-file-validate").arg(path))?;
    Ok(digest(binary))
}

pub fn executable_sections(binary: &[u8]) -> Result<BTreeMap<String, String>> {
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    fs::write(root.join("binary"), binary)?;
    let sections = [".text", ".rodata", ".note.gnu.build-id"];
    let mut command = Command::new("objcopy");
    for name in sections {
        command.args([
            "--dump-section",
            &format!("{name}={}", root.join(name).display()),
        ]);
    }
    run(command.arg(root.join("binary")).arg(root.join("copy")))?;
    sections
        .into_iter()
        .map(|name| {
            let data = fs::read(root.join(name))?;
            ensure!(!data.is_empty(), "Missing ELF section {name}");
            Ok((name.into(), digest(&data)))
        })
        .collect()
}

pub fn files_under(root: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            files.extend(files_under(&entry.path())?);
        } else if kind.is_file() {
            files.push(entry.path());
        }
    }
    Ok(files)
}

fn has_lines(text: &str, required: &[String], label: &str) -> Result<()> {
    for line in required {
        ensure!(
            text.lines().any(|l| l == line),
            "{label} metadata missing {line}"
        );
    }
    Ok(())
}

pub fn verify_artifacts(root: &Path, directory: &Path, version: &str) -> Result<()> {
    let files = inventory(version);
    let deb = directory.join(&files["deb"]);
    let fields = text(Command::new("dpkg-deb").arg("-f").arg(&deb))?;
    has_lines(
        &fields,
        &[
            "Package: open-meteo".into(),
            format!("Version: {version}"),
            "Architecture: amd64".into(),
            format!("Homepage: {REPOSITORY}"),
        ],
        "DEB",
    )?;
    let depends = fields
        .lines()
        .find(|l| l.starts_with("Depends:"))
        .context("DEB dependencies")?;
    for name in [
        "libgtk-4-1",
        "libadwaita-1-0",
        "ca-certificates",
        "adwaita-icon-theme",
    ] {
        ensure!(depends.contains(name), "DEB dependency missing: {name}");
    }
    let deb_contents = tar_contents(&run(Command::new("dpkg-deb")
        .arg("--fsys-tarfile")
        .arg(deb))?)?;
    let binary_hash = verify_payload(root, &deb_contents, "DEB")?;
    let code_signature = executable_sections(&deb_contents["usr/bin/open-meteo"].0)?;

    let rpm = directory.join(&files["rpm"]);
    let fields = text(
        Command::new("rpm")
            .args([
                "-qp",
                "--qf",
                "%{NAME}\n%{VERSION}\n%{ARCH}\n%{URL}\n%{LICENSE}\n",
            ])
            .arg(&rpm),
    )?;
    ensure!(
        fields.lines().collect::<Vec<_>>()
            == ["open-meteo", version, "x86_64", REPOSITORY, "GPL-3.0-only"],
        "RPM identity/version/license mismatch"
    );
    let names = text(Command::new("rpm").arg("-qpl").arg(&rpm))?;
    let names: BTreeSet<_> = names
        .lines()
        .map(|s| s.trim_start_matches('/').to_owned())
        .collect();
    ensure!(
        payload_names().is_subset(&names),
        "RPM payload is incomplete"
    );
    let requires = text(Command::new("rpm").args(["-qp", "--requires"]).arg(&rpm))?;
    for name in [
        "gtk4",
        "libadwaita",
        "ca-certificates",
        "adwaita-icon-theme",
    ] {
        ensure!(
            requires
                .lines()
                .any(|l| l == name || l.starts_with(&format!("{name} "))),
            "RPM dependency missing: {name}"
        );
    }
    let payload = run(Command::new("rpm2cpio").arg(rpm))?;
    let mut rpm_contents = Contents::new();
    for name in payload_names() {
        let data = input(
            Command::new("cpio").args(["--extract", "--to-stdout", &format!("./{name}")]),
            &payload,
        )?;
        rpm_contents.insert(name, (data, 0o755));
    }
    ensure!(
        verify_payload(root, &rpm_contents, "RPM")? == binary_hash,
        "RPM executable differs from DEB"
    );

    let arch = tar_contents(&run(Command::new("zstd")
        .arg("-dc")
        .arg(directory.join(&files["arch"])))?)?;
    let fields = String::from_utf8(
        arch.get(".PKGINFO")
            .context("Missing Arch package metadata")?
            .0
            .clone(),
    )?;
    has_lines(
        &fields,
        &[
            "pkgname = open-meteo".into(),
            format!("pkgver = {version}-1"),
            "arch = x86_64".into(),
            format!("url = {REPOSITORY}"),
            "license = GPL-3.0-only".into(),
            "depend = gtk4>=4.8".into(),
            "depend = libadwaita>=1.2".into(),
            "depend = ca-certificates".into(),
            "depend = adwaita-icon-theme".into(),
        ],
        "Arch",
    )?;
    ensure!(
        verify_payload(root, &arch, "Arch")? == binary_hash,
        "Arch executable differs from DEB"
    );

    let temp = tempfile::tempdir()?;
    let image = temp.path().join("OpenMeteo.AppImage");
    fs::copy(directory.join(&files["appimage"]), &image)?;
    fs::set_permissions(&image, fs::Permissions::from_mode(0o755))?;
    ensure!(
        Command::new(&image)
            .arg("--appimage-extract")
            .current_dir(temp.path())
            .stdout(Stdio::null())
            .status()?
            .success(),
        "AppImage extraction failed"
    );
    let appdir = temp.path().join("squashfs-root");
    let mut contents = Contents::new();
    for name in payload_names() {
        let path = appdir.join(&name);
        if path.is_file() {
            contents.insert(
                name,
                (fs::read(&path)?, fs::metadata(&path)?.permissions().mode()),
            );
        }
    }
    verify_payload(root, &contents, "AppImage")?;
    ensure!(
        executable_sections(&contents["usr/bin/open-meteo"].0)? == code_signature,
        "AppImage executable code/data/build ID differs from DEB"
    );
    ensure!(appdir.join("AppRun").is_file(), "Missing AppImage launcher");
    let libraries: Vec<_> =
        fs::read_dir(appdir.join("usr/lib"))?.collect::<std::io::Result<Vec<_>>>()?;
    for library in [
        "libgtk-4.so",
        "libadwaita-1.so",
        "libfribidi.so",
        "libharfbuzz.so",
        "libwayland-client.so",
    ] {
        ensure!(
            libraries
                .iter()
                .any(|p| p.file_name().to_string_lossy().starts_with(library)),
            "AppImage is missing {library}"
        );
    }
    let hook = fs::read_to_string(appdir.join("apprun-hooks/linuxdeploy-plugin-gtk.sh"))?;
    ensure!(
        !hook
            .lines()
            .any(|l| l.starts_with("export GTK_THEME=") || l.starts_with("export GDK_BACKEND=")),
        "AppImage overrides native theme/display selection"
    );
    for resource in [
        "usr/share/mime/mime.cache",
        "usr/etc/fonts/fonts.conf",
        "usr/share/icons/Adwaita/index.theme",
        "apprun-hooks/zz-open-meteo.sh",
    ] {
        ensure!(
            appdir.join(resource).is_file(),
            "AppImage resource missing: {resource}"
        );
    }
    let icons = files_under(&appdir.join("usr/share/icons/Adwaita"))?;
    for icon in [
        "weather-few-clouds-symbolic.svg",
        "weather-clear-symbolic.svg",
        "weather-showers-symbolic.svg",
    ] {
        ensure!(
            icons
                .iter()
                .any(|p| p.file_name().is_some_and(|n| n == icon)),
            "AppImage weather icon missing: {icon}"
        );
    }
    ensure!(
        fs::read_to_string(appdir.join("apprun-hooks/zz-open-meteo.sh"))?
            .contains("export GIO_MODULE_DIR=\"$APPDIR/usr/lib/gio/modules\""),
        "AppImage must isolate its GIO modules"
    );

    let source_bytes = fs::read(directory.join(&files["source"]))?;
    let source = tar_contents(&source_bytes)?;
    let prefix = format!("open-meteo-{version}/");
    let mut exact_files = [
        "Cargo.toml",
        "Cargo.lock",
        "Makefile",
        "Dockerfile",
        "README.md",
        "LICENSE",
        "AGENTS.md",
        ".dockerignore",
        ".gitignore",
    ]
    .map(|p| root.join(p))
    .to_vec();
    for directory in [
        "src",
        "tools/checks",
        "tests",
        "scripts",
        "packaging",
        "resources",
        "vendor/ksni",
        ".github",
        "docs",
    ] {
        exact_files.extend(files_under(&root.join(directory))?);
    }
    for absolute in exact_files {
        if absolute
            .components()
            .any(|c| c.as_os_str() == "__pycache__")
            || absolute.extension().is_some_and(|e| e == "pyc")
        {
            continue;
        }
        let relative = absolute.strip_prefix(root)?;
        let name = format!("{prefix}{}", relative.display());
        ensure!(
            source
                .get(&name)
                .is_some_and(|entry| entry.0 == fs::read(&absolute).unwrap_or_default()),
            "Source archive missing/differs: {}",
            relative.display()
        );
    }
    ensure!(
        source.keys().all(|n| n.starts_with(&prefix)),
        "Source archive must have a single versioned root"
    );
    let manifest = String::from_utf8(source[&format!("{prefix}Cargo.toml")].0.clone())?;
    has_lines(
        &manifest,
        &[
            format!("version = \"{version}\""),
            "license = \"GPL-3.0-only\"".into(),
        ],
        "Source",
    )?;
    let recipe = fs::read_to_string(directory.join(&files["recipe"]))?;
    has_lines(
        &recipe,
        &[
            format!("pkgver={version}"),
            "license=('GPL-3.0-only')".into(),
        ],
        "PKGBUILD",
    )?;
    ensure!(
        recipe.contains(&format!("sha256sums=('{}')", digest(&source_bytes))),
        "PKGBUILD source checksum mismatch"
    );
    println!(
        "Verified DEB, RPM, Arch, AppImage, source and PKGBUILD for {version}; executable SHA-256 {binary_hash}"
    );
    Ok(())
}
