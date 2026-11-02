mod accessibility;
mod common;
mod gnome_setup;
mod services;
mod tray;
mod ui;

use anyhow::{Result, bail, ensure};

pub fn run(command: &str) -> Result<()> {
    ensure!(
        std::env::var("OPEN_METEO_TEST_SESSION").as_deref() == Ok("1"),
        "Use scripts/check-ui.sh or scripts/check-tray.sh for an isolated session"
    );
    match command {
        "services" => services::run(),
        "ui" => ui::run(),
        "gnome-setup" => gnome_setup::run(),
        "tray-panel" => tray::panel(),
        "tray-label" => tray::label(),
        _ => bail!("Expected ui, gnome-setup, tray-panel, or tray-label"),
    }
}
