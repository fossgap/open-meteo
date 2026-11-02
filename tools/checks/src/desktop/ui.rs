use super::{
    accessibility::{Accessibility, Role},
    common::*,
};
use crate::process::{input, text};
use anyhow::{Context, Result, ensure};
use glib::variant::{ObjectPath, ToVariant};
use serde_json::json;
use std::{fs, process::Command};

type Pixmaps = Vec<(i32, i32, Vec<u8>)>;

pub fn run() -> Result<()> {
    let h = Harness::new("ui-smoke", "XFCE")?;
    let _services = h.services()?;
    let a11y = Accessibility::new()?;
    let mut app = h.spawn(&["--demo"])?;
    a11y.text("Sample weather")?;
    let win = window()?;
    xdo(&["windowsize", &win, "960", "960"])?;
    xdo(&["windowfocus", &win])?;
    capture("desktop-light", &win, &h.output)?;
    let coverage = text(
        Command::new("convert")
            .arg(h.output.join("desktop-light.png"))
            .args([
                "-crop",
                "150x170+780+160",
                "+repage",
                "-colorspace",
                "sRGB",
                "-fx",
                "(b-r > 0.25) ? 1 : 0",
                "-format",
                "%[fx:mean]",
                "info:",
            ])
            .env("LC_ALL", "C"),
    )?;
    ensure!(
        coverage.trim().parse::<f64>()? > 0.015,
        "Weather icon did not render"
    );
    a11y.click("°F")?;
    a11y.expect("70°")?;
    a11y.click("°C")?;
    a11y.expect("21°")?;
    a11y.click("Today forecast")?;
    a11y.text("Maximum wind")?;
    capture("day-details", &win, &h.output)?;
    a11y.click("Today forecast")?;
    xdo(&["key", "--window", &win, "ctrl+l"])?;
    a11y.expect("Find a location")?;
    xdo(&["type", "--window", &win, "Paris"])?;
    a11y.expect("Paris, Île-de-France, France")?;
    capture("search", &win, &h.output)?;
    xdo(&["key", "--window", &win, "ctrl+a"])?;
    xdo(&["type", "--window", &win, "Atlantis"])?;
    a11y.text("Preview uses sample weather")?;
    xdo(&["key", "--window", &win, "Escape"])?;
    xdo(&["windowsize", &win, "390", "960"])?;
    capture("narrow-light", &win, &h.output)?;
    xdo(&["mousemove", "--window", &win, "340", "850"])?;
    xdo(&["click", "--repeat", "8", "--delay", "70", "5"])?;
    capture("narrow-days", &win, &h.output)?;
    xdo(&["windowsize", &win, "960", "960"])?;
    xdo(&["mousemove", "--window", &win, "300", "200"])?;
    xdo(&["click", "--repeat", "12", "--delay", "30", "4"])?;
    h.setting("SetColorScheme", Some(&(1u32,).to_variant()))?;
    pause(1000);
    capture("desktop-dark", &win, &h.output)?;
    action(&h.bus, true, "clock-setup")?;
    a11y.text("Extension installed. Run clock setup from a GNOME session to enable it.")?;
    capture("clock-setup", &win, &h.output)?;
    xdo(&["key", "Escape"])?;
    xdo(&["windowfocus", &win])?;
    tray_and_bridge(&h, &a11y, &win)?;
    a11y.click("Close")?;
    app.success()?;
    ensure!(
        !h.private
            .path()
            .join("config/open-meteo/settings.json")
            .exists(),
        "Preview must not persist settings"
    );

    h.setting("SetColorScheme", Some(&(2u32,).to_variant()))?;
    let mut app = h.spawn(&[])?;
    a11y.expect("Your weather, at a glance")?;
    let win = window()?;
    capture("welcome", &win, &h.output)?;
    quit(&win, &mut app)?;
    offline_and_languages(&h, &a11y)?;
    let extension_data = h.private.path().join("extension-data");
    let mut env = h.env.clone();
    env.insert(
        "XDG_DATA_HOME".into(),
        extension_data.to_string_lossy().into_owned(),
    );
    let mut setup = h.spawn_with(&["--install-gnome-extension"], &env)?;
    setup.success()?;
    for entry in fs::read_dir(h.root.join("resources/gnome-extension"))? {
        let entry = entry?;
        ensure!(
            fs::read(
                extension_data
                    .join("gnome-shell/extensions")
                    .join(UUID)
                    .join(entry.file_name())
            )? == fs::read(entry.path())?,
            "Embedded extension differs"
        );
    }
    let log = fs::read_to_string(h.output.join("ui-smoke.log"))?;
    for failure in [
        "panicked at",
        "CRITICAL",
        "Failed to load icon",
        "Failed to load module:",
        "Unrecognized image file format",
    ] {
        ensure!(
            !log.contains(failure),
            "Application log contains {failure}: {log}"
        );
    }
    println!(
        "UI checks passed: theme changes, units, search, narrow layout, tray registration/menu/restore/loss, first run, offline cache, persistence, translations, and quit."
    );
    Ok(())
}

fn tray_and_bridge(h: &Harness, a11y: &Accessibility, win: &str) -> Result<()> {
    until("one combined weather tray item", || {
        Ok(h.registered()?.len() == 1)
    })?;
    let item = h.registered()?[0].clone();
    let (service, rest) = item.split_once('/').context("Tray address")?;
    let path = format!("/{rest}");
    let prop = |name| property(&h.bus, service, &path, ITEM, name);
    let title: String = value(&prop("Title")?)?;
    ensure!(
        title.contains("Paris") && title.contains("21°") && title.contains("sample"),
        "Tray title: {title}"
    );
    ensure!(
        value::<String>(&prop("IconName")?)?.is_empty(),
        "Display the combined weather and temperature image"
    );
    let pixmaps: Pixmaps = value(&prop("IconPixmap")?)?;
    ensure!(
        pixmaps.len() == 4
            && pixmaps
                .iter()
                .all(|(w, h, p)| p.len() == (*w * *h * 4) as usize),
        "Missing combined weather pixmaps"
    );
    for (width, height, pixels) in &pixmaps {
        let rgba: Vec<_> = pixels
            .chunks_exact(4)
            .flat_map(|p| [p[1], p[2], p[3], p[0]])
            .collect();
        input(
            Command::new("convert")
                .args([
                    "-size",
                    &format!("{width}x{height}"),
                    "-depth",
                    "8",
                    "rgba:-",
                ])
                .arg(h.output.join(format!("tray-combined-{width}.png"))),
            &rgba,
        )?;
    }
    a11y.click("°F")?;
    until("temperature changes with units", || {
        Ok(value::<Pixmaps>(&prop("IconPixmap")?)? != pixmaps)
    })?;
    ensure!(
        h.registered()? == [item.clone()],
        "Updates must retain one tray item"
    );
    let updated: Pixmaps = value(&prop("IconPixmap")?)?;
    for ((width, _, before), (_, _, after)) in pixmaps.iter().zip(&updated) {
        for row in 0..*width as usize {
            let start = row * *width as usize * 4;
            let end = start + (*width as f64 * 0.38) as usize * 4;
            ensure!(
                before[start..end] == after[start..end],
                "Keep the condition symbol beside the updated temperature"
            );
        }
    }
    a11y.click("°C")?;
    let menu: ObjectPath = value(&prop("Menu")?)?;
    let layout = call(
        &h.bus,
        service,
        menu.as_str(),
        "com.canonical.dbusmenu",
        "GetLayout",
        Some(&(0i32, -1i32, Vec::<String>::new()).to_variant()),
    )?
    .print(true)
    .to_string();
    ensure!(
        layout.contains("Next 7 days") && layout.contains("Next hours"),
        "Forecast menus: {layout}"
    );
    a11y.click("Close")?;
    until("close to tray", || mapped(win, "IsUnMapped"))?;
    call(
        &h.bus,
        service,
        &path,
        ITEM,
        "Activate",
        Some(&(0i32, 0i32).to_variant()),
    )?;
    until("restore from tray", || mapped(win, "IsViewable"))?;
    a11y.click("Close")?;
    until("hidden before tray host disappears", || {
        mapped(win, "IsUnMapped")
    })?;
    h.setting("RemoveTray", None)?;
    until("tray loss restores hidden window", || {
        mapped(win, "IsViewable")
    })?;
    let snapshot: serde_json::Value =
        serde_json::from_str(&first::<String>(&h.bridge("Attach")?)?)?;
    until("clock hides combined tray", || {
        Ok(value::<String>(&prop("Status")?)? == "Passive")
    })?;
    ensure!(
        snapshot["temperature"] == "21°C" && snapshot["location"] == "Paris",
        "Clock snapshot: {snapshot}"
    );
    a11y.click("Close")?;
    until("close to GNOME clock", || mapped(win, "IsUnMapped"))?;
    h.bridge("Show")?;
    until("GNOME forecast action", || mapped(win, "IsViewable"))?;
    a11y.click("Close")?;
    until("hidden before clock detaches", || mapped(win, "IsUnMapped"))?;
    h.bridge("Detach")?;
    until("clock detach restores tray", || {
        Ok(value::<String>(&prop("Status")?)? == "Active")
    })?;
    until("extension disable restores hidden window", || {
        mapped(win, "IsViewable")
    })?;
    let peer = gio::DBusConnection::for_address_sync(
        &std::env::var("DBUS_SESSION_BUS_ADDRESS")?,
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
        None,
        gio::Cancellable::NONE,
    )?;
    call(
        &peer,
        DEMO,
        "/io/github/open_meteo/OpenMeteo/Weather",
        "io.github.open_meteo.OpenMeteo.Weather1",
        "Attach",
        None,
    )?;
    a11y.click("Close")?;
    until("close with independent clock peer", || {
        mapped(win, "IsUnMapped")
    })?;
    peer.close_sync(gio::Cancellable::NONE)?;
    until("clock crash restores hidden window", || {
        mapped(win, "IsViewable")
    })?;
    Ok(())
}

fn quit(window: &str, app: &mut Process) -> Result<()> {
    xdo(&["windowfocus", window])?;
    xdo(&["key", "ctrl+q"])?;
    app.success()
}

fn offline_and_languages(h: &Harness, a11y: &Accessibility) -> Result<()> {
    let location = json!({"name":"Paris", "latitude":48.85341, "longitude":2.3488, "country":"France", "admin1":"Île-de-France"});
    let config = h.private.path().join("config/open-meteo");
    let cache = h.private.path().join("cache/open-meteo");
    fs::create_dir_all(&config)?;
    fs::create_dir_all(&cache)?;
    let settings = config.join("settings.json");
    write_json(
        &settings,
        &json!({"location":location,"recent":[location],"units":"celsius","keep_in_tray":true}),
    )?;
    let fixture = read_json(h.root.join("tests/fixtures/forecast.json"))?;
    write_json(
        cache.join("48.85341_2.34880.json"),
        &json!({"location":location,"fetched_at":fixture["current"]["time"],"forecast":fixture}),
    )?;
    let mut app = h.spawn(&[])?;
    a11y.text("Showing saved weather")?;
    ensure!(
        a11y.present("saved hourly forecast has ended"),
        "Expired hours must not be labelled current"
    );
    let win = window()?;
    capture("offline", &win, &h.output)?;
    a11y.click("°F")?;
    until("saved Fahrenheit preference", || {
        Ok(read_json(&settings)?["units"] == "fahrenheit")
    })?;
    xdo(&["windowfocus", &win])?;
    xdo(&["key", "--window", &win, "ctrl+l"])?;
    xdo(&["type", "--window", &win, "London"])?;
    a11y.expect("Couldn’t connect. Check your internet connection and try again.")?;
    xdo(&["key", "Escape"])?;
    until("search popover closes", || {
        Ok(a11y.find("Find a location").is_none())
    })?;
    a11y.click("Main menu")?;
    capture("main-menu", &win, &h.output)?;
    let menu = wait("language submenu", || {
        Ok(a11y
            .find("Language (restart required)")
            .or_else(|| a11y.nodes().into_iter().find(|n| n.is_role(Role::MenuItem))))
    })?;
    menu.activate()?;
    let german = wait("German language option", || {
        let named = a11y.find("Deutsch");
        let radios = a11y
            .nodes()
            .into_iter()
            .filter(|n| n.is_role(Role::RadioMenuItem))
            .collect::<Vec<_>>();
        Ok(named.or_else(|| (radios.len() == 8).then(|| radios[3].clone())))
    })
    .inspect_err(|_| {
        let _ = capture("language-failure", &win, &h.output);
        for node in a11y.nodes() {
            eprintln!("LANGUAGE NODE {:?} {:?}", node.name, node.role());
        }
    })?;
    capture("language-menu", &win, &h.output)?;
    german.activate()?;
    until("language selection saved", || {
        Ok(read_json(&settings)?["language"] == "de")
    })?;
    action(&h.bus, false, "quit")?;
    app.success()?;
    for language in ["fr", "de", "es", "zh", "hi", "ar"] {
        let messages = read_json(h.root.join(format!("resources/locales/{language}.json")))?;
        let mut env = h.env.clone();
        env.insert("LANGUAGE".into(), language.into());
        let mut app = h.spawn_with(&["--demo"], &env)?;
        a11y.expect(
            messages["Next 24 hours"]
                .as_str()
                .context("Hourly translation")?,
        )?;
        ensure!(
            a11y.find(messages["Next 7 days"].as_str().unwrap())
                .is_some(),
            "{language} daily heading"
        );
        let state: serde_json::Value =
            serde_json::from_str(&first::<String>(&h.bridge("GetSnapshot")?)?)?;
        ensure!(
            state["language"] == language && state["rtl"] == (language == "ar"),
            "Translated bridge: {state}"
        );
        ensure!(
            state["temperature"] == if language == "ar" { "٢١°C" } else { "21°C" },
            "Translated temperature: {state}"
        );
        let win = window()?;
        xdo(&["windowsize", &win, "960", "960"])?;
        capture(&format!("language-{language}"), &win, &h.output)?;
        xdo(&["windowsize", &win, "420", "960"])?;
        capture(&format!("language-{language}-narrow"), &win, &h.output)?;
        ensure!(
            geometry(&win)?["WIDTH"] == 420,
            "{language} expands narrow window"
        );
        let current = a11y
            .find(messages["Now"].as_str().unwrap())
            .context("Current hour")?
            .rect()?;
        ensure!(
            current.x >= 0 && current.x < 420,
            "{language} hourly strip must start at current hour"
        );
        quit(&win, &mut app)?;
    }
    let mut env = h.env.clone();
    env.insert("LANGUAGE".into(), "fr".into());
    let mut app = h.spawn_with(&[], &env)?;
    a11y.text("Gespeicherte Wetterdaten werden angezeigt.")?;
    quit(&window()?, &mut app)?;
    Ok(())
}
