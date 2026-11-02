use anyhow::Result;
use open_meteo_checks::{packaging::*, process::run};
use std::{fs, os::unix::fs::symlink, path::PathBuf, process::Command};

struct Release {
    directory: tempfile::TempDir,
    files: Inventory,
}
impl Release {
    fn new() -> Result<Self> {
        let directory = tempfile::tempdir()?;
        let files = inventory("0.1.0");
        for name in files.values() {
            fs::write(directory.path().join(name), b"release fixture")?;
        }
        write_checksums(directory.path(), &files)?;
        Ok(Self { directory, files })
    }
    fn check(&self) -> Result<()> {
        verify_inventory(self.directory.path(), &self.files)?;
        verify_checksums(self.directory.path(), &self.files)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.directory.path().join(name)
    }
}

#[test]
fn complete_release() -> Result<()> {
    Release::new()?.check()
}

#[test]
fn corrupted_download() -> Result<()> {
    let release = Release::new()?;
    fs::write(release.path(&release.files["deb"]), "corrupt")?;
    assert!(
        release
            .check()
            .unwrap_err()
            .to_string()
            .contains("Checksum mismatch")
    );
    Ok(())
}

#[test]
fn missing_or_duplicate_checksum() -> Result<()> {
    let release = Release::new()?;
    let manifest = fs::read_to_string(release.path("SHA256SUMS"))?;
    fs::write(
        release.path("SHA256SUMS"),
        manifest.lines().skip(1).collect::<Vec<_>>().join("\n"),
    )?;
    assert!(
        release
            .check()
            .unwrap_err()
            .to_string()
            .contains("every artifact")
    );
    fs::write(release.path("SHA256SUMS"), manifest.repeat(2))?;
    assert!(
        release
            .check()
            .unwrap_err()
            .to_string()
            .contains("duplicate checksum")
    );
    Ok(())
}

#[test]
fn checksum_cannot_escape_directory() -> Result<()> {
    let release = Release::new()?;
    fs::write(
        release.path("SHA256SUMS"),
        format!("{}  ../outside\n", digest(b"release fixture")),
    )?;
    assert!(
        release
            .check()
            .unwrap_err()
            .to_string()
            .contains("Invalid checksum")
    );
    Ok(())
}

#[test]
fn wrong_version_or_extra_artifact() -> Result<()> {
    let release = Release::new()?;
    fs::write(release.path("open-meteo_0.0.9_amd64.deb"), "stale")?;
    assert!(
        release
            .check()
            .unwrap_err()
            .to_string()
            .contains("inventory")
    );
    Ok(())
}

#[test]
fn missing_linked_or_empty_artifact() -> Result<()> {
    let release = Release::new()?;
    let path = release.path(&release.files["deb"]);
    fs::remove_file(&path)?;
    assert!(
        release
            .check()
            .unwrap_err()
            .to_string()
            .contains("inventory")
    );
    symlink(release.path(&release.files["rpm"]), &path)?;
    assert!(release.check().unwrap_err().to_string().contains("regular"));
    fs::remove_file(&path)?;
    fs::write(&path, "")?;
    assert!(
        release
            .check()
            .unwrap_err()
            .to_string()
            .contains("nonempty")
    );
    Ok(())
}

#[test]
fn linked_checksum_manifest_is_rejected() -> Result<()> {
    let release = Release::new()?;
    let manifest = release.path("SHA256SUMS");
    let external = tempfile::NamedTempFile::new()?;
    fs::copy(&manifest, external.path())?;
    fs::remove_file(&manifest)?;
    symlink(external.path(), &manifest)?;
    assert!(
        release
            .check()
            .unwrap_err()
            .to_string()
            .contains("regular SHA256SUMS")
    );
    Ok(())
}

fn archive_entry(name: &str, kind: tar::EntryType) -> Vec<u8> {
    let mut header = tar::Header::new_gnu();
    // Set the raw path so the test can exercise malicious archives that a safe
    // archive writer would otherwise refuse to create.
    header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
    header.set_entry_type(kind);
    header.set_size(0);
    header.set_mode(0o644);
    header.set_cksum();
    let mut archive = tar::Builder::new(Vec::new());
    archive.append(&header, &[][..]).unwrap();
    archive.into_inner().unwrap()
}

#[test]
fn unsafe_archive_members() {
    for (name, kind) in [
        ("../escape", tar::EntryType::Regular),
        ("/absolute", tar::EntryType::Regular),
        ("link", tar::EntryType::Symlink),
        ("device", tar::EntryType::Char),
    ] {
        assert!(
            tar_contents(&archive_entry(name, kind)).is_err(),
            "Accepted {name}"
        );
    }
}

#[test]
fn duplicate_archive_paths_are_rejected() {
    let mut bytes = archive_entry("file", tar::EntryType::Regular);
    let other = archive_entry("./file", tar::EntryType::Regular);
    bytes.truncate(512);
    bytes.extend(other);
    assert!(
        tar_contents(&bytes)
            .unwrap_err()
            .to_string()
            .contains("Duplicate archive file")
    );
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn missing_launcher_payload() {
    assert!(
        verify_payload(&root(), &Contents::new(), "test")
            .unwrap_err()
            .to_string()
            .contains("Missing test payload")
    );
}

#[test]
fn missing_or_changed_license_stops_release() -> Result<()> {
    let contents = LICENSE_FILES
        .into_iter()
        .map(|(name, source)| Ok((name.into(), (fs::read(root().join(source))?, 0o644))))
        .collect::<Result<Contents>>()?;
    verify_licenses(&root(), &contents, "test")?;
    for (name, _) in LICENSE_FILES {
        let mut bad = contents.clone();
        bad.remove(name);
        assert!(
            verify_licenses(&root(), &bad, "test")
                .unwrap_err()
                .to_string()
                .contains("Missing license file")
        );
        bad.insert(name.into(), (b"Wrong license".to_vec(), 0o644));
        assert!(
            verify_licenses(&root(), &bad, "test")
                .unwrap_err()
                .to_string()
                .contains("License file differs")
        );
    }
    Ok(())
}

#[test]
fn elf_metadata_changes_preserve_code_signature() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let original = fs::read("/bin/true")?;
    let binary = temp.path().join("binary");
    let note = temp.path().join("note");
    fs::write(&binary, &original)?;
    fs::write(&note, "packaging metadata")?;
    run(Command::new("objcopy")
        .arg("--add-section")
        .arg(format!(".ci-test={}", note.display()))
        .arg(&binary))?;
    let changed = fs::read(binary)?;
    assert_ne!(digest(&original), digest(&changed));
    assert_eq!(
        executable_sections(&original)?,
        executable_sections(&changed)?
    );
    Ok(())
}

#[test]
fn changed_executable_code_is_detected() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let original = fs::read("/bin/true")?;
    let binary = temp.path().join("binary");
    let section = temp.path().join("text");
    fs::write(&binary, &original)?;
    run(Command::new("objcopy")
        .arg("--dump-section")
        .arg(format!(".text={}", section.display()))
        .arg(&binary))?;
    let mut data = fs::read(&section)?;
    *data.last_mut().unwrap() ^= 1;
    fs::write(&section, data)?;
    run(Command::new("objcopy")
        .arg("--update-section")
        .arg(format!(".text={}", section.display()))
        .arg(&binary))?;
    assert_ne!(
        executable_sections(&original)?,
        executable_sections(&fs::read(binary)?)?
    );
    Ok(())
}
