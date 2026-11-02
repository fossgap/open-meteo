use anyhow::{Context, Result, ensure};
use open_meteo_checks::packaging;
use std::path::PathBuf;

fn main() {
    if let Err(error) = execute() {
        eprintln!("Package verification failed: {error:#}");
        std::process::exit(1);
    }
}

fn execute() -> Result<()> {
    let mut checksums_only = false;
    let mut write_checksums = false;
    let mut directory = None;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--checksums-only" => checksums_only = true,
            "--write-checksums" => write_checksums = true,
            "--help" | "-h" => {
                println!(
                    "Usage: verify-packages [--checksums-only | --write-checksums] DIRECTORY\nRun from the repository root."
                );
                return Ok(());
            }
            _ => {
                ensure!(
                    !arg.starts_with('-') && directory.is_none(),
                    "Unexpected argument: {arg}"
                );
                directory = Some(PathBuf::from(arg));
            }
        }
    }
    ensure!(
        !(checksums_only && write_checksums),
        "Choose only one checksum mode"
    );
    let root = std::env::current_dir()?;
    let directory = directory
        .context("Provide the release directory")?
        .canonicalize()?;
    let version = packaging::version(&root)?;
    let inventory = packaging::inventory(&version);
    packaging::verify_inventory(&directory, &inventory)?;
    if !write_checksums {
        packaging::verify_checksums(&directory, &inventory)?;
    }
    if !checksums_only {
        packaging::verify_artifacts(&root, &directory, &version)?;
    }
    if write_checksums {
        packaging::write_checksums(&directory, &inventory)?;
        packaging::verify_checksums(&directory, &inventory)?;
    }
    println!("Release inventory and SHA-256 checksums verified.");
    Ok(())
}
