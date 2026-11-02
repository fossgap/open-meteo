mod api;
mod components;
mod model;
mod storage;
mod tray;
mod ui;

use adw::prelude::*;
use gtk::{gio, glib};

const APP_ID: &str = "io.github.open_meteo.OpenMeteo";

fn main() -> glib::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "Open Meteo {}\n\nUsage: open-meteo [--demo | --check-api | --version]\n\n  --demo       Preview with clearly labelled sample weather; no network or saved settings\n  --check-api  Check live location search and forecast access, then exit\n  --version    Print the version",
            env!("CARGO_PKG_VERSION")
        );
        return glib::ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--version") {
        println!("Open Meteo {}", env!("CARGO_PKG_VERSION"));
        return glib::ExitCode::SUCCESS;
    }
    if args == ["--check-api"] {
        return match check_api() {
            Ok(()) => glib::ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{error}");
                glib::ExitCode::FAILURE
            }
        };
    }
    let demo = args == ["--demo"];
    if !args.is_empty() && !demo {
        eprintln!("Unknown option. Run open-meteo --help for usage.");
        return glib::ExitCode::FAILURE;
    }
    let app = adw::Application::builder()
        .application_id(if demo {
            "io.github.open_meteo.OpenMeteo.Demo"
        } else {
            APP_ID
        })
        .flags(gio::ApplicationFlags::empty())
        .build();
    app.connect_activate(move |app| {
        if let Some(window) = app.active_window() {
            window.present();
            return;
        }
        ui::build(app, demo);
    });
    app.run_with_args::<&str>(&[])
}

fn check_api() -> Result<(), String> {
    let api = api::WeatherClient::new()?;
    let results = api.search("Paris")?;
    let location = results
        .first()
        .ok_or("Location search returned no results")?;
    let forecast = api.forecast(location)?;
    println!(
        "{}: {} hourly readings, {} days, timezone {}. API check passed.",
        location.name,
        forecast.hourly.time.len(),
        forecast.daily.time.len(),
        forecast.timezone
    );
    Ok(())
}
