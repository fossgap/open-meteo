use super::{
    accessibility::{Accessibility, Rect, Role},
    common::*,
};
use crate::process::{run as execute, text};
use anyhow::{Context, Result, ensure};
use glib::variant::ToVariant;
use std::{cell::RefCell, collections::BTreeMap, fs, path::Path, process::Command, rc::Rc};

pub fn label() -> Result<()> {
    let h = Harness::new("tray-label", "ubuntu:GNOME")?;
    let _services = h.services()?;
    let a11y = Accessibility::new()?;
    let mut app = h.spawn(&["--demo"])?;
    a11y.expect("21°")?;
    until("one native labeled item", || Ok(h.registered()?.len() == 1))?;
    let item = h.registered()?[0].clone();
    let (service, rest) = item.split_once('/').context("Tray address")?;
    let path = format!("/{rest}");
    let prop = |name| property(&h.bus, service, &path, ITEM, name);
    until("native temperature label", || {
        Ok(value::<String>(&prop("XAyatanaLabel")?)? == "21°")
    })?;
    let icon: String = value(&prop("IconName")?)?;
    ensure!(
        icon.starts_with("weather-") && prop("IconPixmap")?.n_children() == 0,
        "Use a full-size themed icon without a second bitmap temperature"
    );
    ensure!(
        !value::<String>(&prop("XAyatanaLabelGuide")?)?.is_empty()
            && !value::<bool>(&prop("ItemIsMenu")?)?,
        "Native label and shared activation metadata"
    );
    let labels = Rc::new(RefCell::new(Vec::<String>::new()));
    let changed = Rc::new(RefCell::new(Vec::<String>::new()));
    let signal_labels = labels.clone();
    let label_sub = h.bus.subscribe_to_signal(
        Some(service),
        Some(ITEM),
        Some("XAyatanaNewLabel"),
        Some(&path),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            signal_labels
                .borrow_mut()
                .push(first(signal.parameters).unwrap());
        },
    );
    let signal_changed = changed.clone();
    let change_sub = h.bus.subscribe_to_signal(
        Some(service),
        Some("org.freedesktop.DBus.Properties"),
        Some("PropertiesChanged"),
        Some(&path),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            let properties: BTreeMap<String, glib::Variant> =
                value(&signal.parameters.child_value(1)).unwrap();
            if let Some(label) = properties
                .get("XAyatanaLabel")
                .and_then(|v| v.get::<String>())
            {
                signal_changed.borrow_mut().push(label);
            }
        },
    );
    a11y.click("°F")?;
    until("native label update signal", || {
        Ok(value::<String>(&prop("XAyatanaLabel")?)? == "70°"
            && labels.borrow().iter().any(|l| l == "70°"))
    })?;
    until("label property update", || {
        Ok(changed.borrow().iter().any(|l| l == "70°"))
    })?;
    ensure!(
        value::<String>(&prop("IconName")?)? == icon && h.registered()? == [item.clone()],
        "Updates preserve the same single icon"
    );
    let win = window()?;
    a11y.click("Close")?;
    until("hide forecast", || mapped(&win, "IsUnMapped"))?;
    call(
        &h.bus,
        service,
        &path,
        ITEM,
        "Activate",
        Some(&(0i32, 0i32).to_variant()),
    )?;
    until("shared activation", || mapped(&win, "IsViewable"))?;
    drop(label_sub);
    drop(change_sub);
    xdo(&["windowfocus", &win])?;
    xdo(&["key", "ctrl+q"])?;
    app.success()?;
    println!(
        "TRAY_LABEL_SUITE_PASSED: one native icon + temperature item, update signals, and activation"
    );
    Ok(())
}

const CONFIG: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<channel name="xfce4-panel" version="1.0">
  <property name="configver" type="int" value="2"/>
  <property name="panels" type="array"><value type="int" value="1"/>
    <property name="panel-1" type="empty">
      <property name="position" type="string" value="p=6;x=0;y=0"/>
      <property name="position-locked" type="bool" value="true"/>
      <property name="length" type="uint" value="100"/>
      <property name="size" type="uint" value="32"/>
      <property name="plugin-ids" type="array"><value type="int" value="1"/></property>
    </property>
  </property>
  <property name="plugins" type="empty">
    <property name="plugin-1" type="string" value="systray">
      <property name="icon-size" type="int" value="24"/>
      <property name="single-row" type="bool" value="true"/>
    </property>
  </property>
</channel>"#;

fn buttons(a11y: &Accessibility) -> Vec<Rect> {
    let mut buttons = a11y
        .nodes()
        .into_iter()
        .filter_map(|n| {
            if n.application != "xfce4-panel"
                || !n.is_role(Role::Button)
                || !n.showing().unwrap_or(false)
            {
                return None;
            }
            n.rect().ok().filter(|r| r.width >= 20 && r.height >= 20)
        })
        .collect::<Vec<_>>();
    buttons.sort_by_key(|r| r.x);
    buttons
}
fn pixels(file: &Path, rectangle: Rect) -> Result<Vec<u8>> {
    execute(Command::new("convert").arg(file).args([
        "-crop",
        &rectangle.crop(),
        "-depth",
        "8",
        "rgba:-",
    ]))
}

pub fn panel() -> Result<()> {
    ensure!(
        std::env::var("OPEN_METEO_TRAY_TEST_SESSION").as_deref() == Ok("1"),
        "Use scripts/check-tray.sh"
    );
    let theme = std::env::var("OPEN_METEO_PANEL_THEME").unwrap_or_else(|_| "Adwaita".into());
    let h = Harness::new(&format!("tray-panel-{}", theme.replace(':', "-")), "XFCE")?;
    let config = h
        .private
        .path()
        .join("config/xfce4/xfconf/xfce-perchannel-xml");
    fs::create_dir_all(&config)?;
    fs::write(config.join("xfce4-panel.xml"), CONFIG)?;
    fs::create_dir_all(h.private.path().join("config/gtk-3.0"))?;
    fs::write(
        h.private.path().join("config/gtk-3.0/settings.ini"),
        "[Settings]\ngtk-enable-animations=false\n",
    )?;
    let activation: BTreeMap<_, _> = [
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
        "XDG_CURRENT_DESKTOP",
        "GIO_USE_VFS",
    ]
    .into_iter()
    .map(|k| (k, &h.env[k]))
    .collect();
    call(
        &h.bus,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "UpdateActivationEnvironment",
        Some(&(activation,).to_variant()),
    )?;
    let a11y = Accessibility::new()?;
    // Enable the bridge before XFCE creates its external GtkSocket/AtkPlug.
    // Otherwise the plugin can embed before its accessibility tree is ready.
    call(
        &h.bus,
        "org.a11y.Bus",
        "/org/a11y/bus",
        "org.freedesktop.DBus.Properties",
        "Set",
        Some(&("org.a11y.Status", "ScreenReaderEnabled", true.to_variant()).to_variant()),
    )?;
    let _panel = Process(
        command("xfce4-panel", &h.env)
            .arg("--disable-wm-check")
            .env("GTK_THEME", &theme)
            .stdout(h.log.try_clone()?)
            .stderr(h.log.try_clone()?)
            .spawn()?,
    );
    a11y.expect("xfce4-panel")?;
    let plugins = text(command("xfconf-query", &h.env).args([
        "-c",
        "xfce4-panel",
        "-p",
        "/panels/panel-1/plugin-ids",
    ]))?;
    ensure!(
        plugins.trim().ends_with('1'),
        "Panel did not load isolated configuration: {plugins}"
    );
    let mut app = h.spawn(&["--demo"])?;
    a11y.expect("21°")?;
    let win = window()?;
    xdo(&["windowmove", &win, "0", "80"])?;
    xdo(&["mousemove", "900", "700"])?;
    let capture = |name: &str| -> Result<_> {
        pause(300);
        let file = h.output.join(name);
        execute(
            Command::new("import")
                .args(["-window", "root", "-crop", "960x32+0+0"])
                .arg(&file),
        )?;
        Ok(file)
    };
    if let Err(error) = until("one combined weather tray button", || {
        xdo(&["windowmove", &win, "0", "80"])?;
        Ok(buttons(&a11y).len() == 1)
    }) {
        capture("tray-failure.png")?;
        for node in a11y.nodes() {
            if node.application == "xfce4-panel" || node.application == "wrapper-2.0" {
                eprintln!("CHILDREN {}", node.child_diagnostics());
            }
            eprintln!(
                "PANEL NODE {:?} {:?} {:?}",
                node.application,
                node.name,
                node.role()
            );
            if node.is_role(Role::Button) {
                eprintln!(
                    "BUTTON {:?} {:?} {:?} {:?}",
                    node.application,
                    node.name,
                    node.rect(),
                    node.showing()
                );
            }
        }
        return Err(error);
    }
    let rectangle = buttons(&a11y)[0];
    let weather_width = (rectangle.width as f64 * 0.38) as i32;
    let left = Rect {
        width: weather_width,
        ..rectangle
    };
    let right = Rect {
        x: rectangle.x + weather_width,
        width: rectangle.width - weather_width,
        ..rectangle
    };
    let suffix = if theme.ends_with(":dark") {
        "dark"
    } else {
        "light"
    };
    let before = capture(&format!("tray-{suffix}-celsius.png"))?;
    a11y.click("°F")?;
    a11y.expect("70°")?;
    let after = capture(&format!("tray-{suffix}-fahrenheit.png"))?;
    ensure!(
        buttons(&a11y).len() == 1,
        "Unit changes must not add a second tray button"
    );
    ensure!(
        pixels(&before, left)? == pixels(&after, left)?,
        "Left element must remain the weather symbol"
    );
    ensure!(
        pixels(&before, right)? != pixels(&after, right)?,
        "Right element must be changing temperature"
    );
    xdo(&[
        "mousemove",
        &(rectangle.x + rectangle.width / 4).to_string(),
        &(rectangle.y + rectangle.height / 2).to_string(),
    ])?;
    let weather_hover = capture(&format!("tray-{suffix}-hover-weather.png"))?;
    xdo(&[
        "mousemove",
        &(rectangle.x + 3 * rectangle.width / 4).to_string(),
        &(rectangle.y + rectangle.height / 2).to_string(),
    ])?;
    let temperature_hover = capture(&format!("tray-{suffix}-hover-temperature.png"))?;
    ensure!(
        pixels(&weather_hover, rectangle)? == pixels(&temperature_hover, rectangle)?,
        "Both halves must share hover area"
    );
    for fraction in [0.2, 0.42, 0.75] {
        a11y.click("Close")?;
        until("hide forecast", || mapped(&win, "IsUnMapped"))?;
        xdo(&[
            "mousemove",
            &(rectangle.x + (rectangle.width as f64 * fraction) as i32).to_string(),
            &(rectangle.y + rectangle.height / 2).to_string(),
        ])?;
        xdo(&["click", "1"])?;
        until("open from any part of single tray item", || {
            mapped(&win, "IsViewable")
        })?;
    }
    xdo(&["windowfocus", &win])?;
    xdo(&["key", "ctrl+q"])?;
    app.success()?;
    until("quit removes tray item", || Ok(buttons(&a11y).is_empty()))?;
    println!(
        "TRAY_PANEL_SUITE_PASSED ({theme}): one weather + temperature item, units, shared hover/click area, and cleanup"
    );
    Ok(())
}
