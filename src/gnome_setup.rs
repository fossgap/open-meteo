//! Install and enable using GNOME's supported interface. A running Shell cannot
//! discover a newly copied local extension. Never use Eval or restart the desktop.
use crate::{desktop::EXTENSION_ID, i18n::tr};
use gtk::{gio, glib, prelude::*};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

const BUS: &str = "org.gnome.Shell";
const PATH: &str = "/org/gnome/Shell";
const INTERFACE: &str = "org.gnome.Shell.Extensions";
const FILES: [(&str, &str); 3] = [
    (
        "metadata.json",
        include_str!("../resources/gnome-extension/metadata.json"),
    ),
    (
        "extension.js",
        include_str!("../resources/gnome-extension/extension.js"),
    ),
    (
        "stylesheet.css",
        include_str!("../resources/gnome-extension/stylesheet.css"),
    ),
];

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Enabled,
    NextSession,
    Updated,
    NoShell,
}

impl Outcome {
    pub fn message(&self) -> String {
        tr(match self {
            Self::Enabled => "GNOME clock integration is enabled.",
            Self::NextSession => {
                "Open Meteo Clock is ready for your next GNOME session. GNOME has not discovered the new extension yet. On Wayland, sign out and back in once; on X11, restart GNOME Shell. It will enable automatically."
            }
            Self::Updated => {
                "The clock extension has been updated. GNOME will load it the next time you sign in. It will enable automatically."
            }
            Self::NoShell => {
                "Extension installed. Run clock setup from a GNOME session to enable it."
            }
        })
    }
}

fn data_path() -> Result<PathBuf, String> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .map(|p| p.join("gnome-shell/extensions").join(EXTENSION_ID))
        .ok_or_else(|| "No user data directory is available".into())
}

fn matches_bundle(path: &Path) -> bool {
    FILES.iter().all(|(name, text)| {
        std::fs::read(path.join(name)).is_ok_and(|data| data == text.as_bytes())
    })
}

fn install_files(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;
    for (name, text) in FILES {
        if std::fs::read(path.join(name)).is_ok_and(|data| data == text.as_bytes()) {
            continue;
        }
        let temporary = path.join(format!(".{name}.{}.tmp", std::process::id()));
        std::fs::write(&temporary, text)?;
        std::fs::rename(temporary, path.join(name))?;
    }
    Ok(())
}

async fn call(bus: &gio::DBusConnection, method: &str) -> Result<glib::Variant, String> {
    bus.call_future(
        Some(BUS),
        PATH,
        INTERFACE,
        method,
        Some(&(EXTENSION_ID,).to_variant()),
        None,
        gio::DBusCallFlags::NO_AUTO_START,
        3000,
    )
    .await
    .map_err(|e| e.to_string())
}

async fn property(bus: &gio::DBusConnection, name: &str) -> Result<glib::Variant, String> {
    bus.call_future(
        Some(BUS),
        PATH,
        "org.freedesktop.DBus.Properties",
        "Get",
        Some(&(INTERFACE, name).to_variant()),
        None,
        gio::DBusCallFlags::NO_AUTO_START,
        3000,
    )
    .await
    .map_err(|e| e.to_string())?
    .get::<(glib::Variant,)>()
    .map(|v| v.0)
    .ok_or_else(|| "Invalid GNOME response".into())
}

async fn info(bus: &gio::DBusConnection) -> Result<HashMap<String, glib::Variant>, String> {
    call(bus, "GetExtensionInfo")
        .await?
        .get::<(HashMap<String, glib::Variant>,)>()
        .map(|v| v.0)
        .ok_or_else(|| "Invalid GNOME extension information".into())
}

// GNOME serializes all numeric extension information as doubles.
fn state(info: &HashMap<String, glib::Variant>) -> Option<u32> {
    info.get("state")
        .and_then(|v| v.get::<f64>().map(|s| s as u32).or_else(|| v.get::<u32>()))
}

fn queue_for_next_session() -> Result<(), String> {
    let schema = gio::SettingsSchemaSource::default()
        .and_then(|source| source.lookup("org.gnome.shell", true))
        .ok_or_else(|| tr("GNOME's extension settings are unavailable."))?;
    let settings = gio::Settings::new_full(&schema, None::<&gio::SettingsBackend>, None);
    let mut enabled = settings
        .strv("enabled-extensions")
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if !enabled.iter().any(|id| id == EXTENSION_ID) {
        enabled.push(EXTENSION_ID.into());
        settings
            .set_strv("enabled-extensions", enabled)
            .map_err(|e| e.to_string())?;
    }
    let disabled = settings.strv("disabled-extensions");
    if disabled.iter().any(|id| id == EXTENSION_ID) {
        let others = disabled
            .iter()
            .filter(|id| id.as_str() != EXTENSION_ID)
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        settings
            .set_strv("disabled-extensions", others)
            .map_err(|e| e.to_string())?;
    }
    gio::Settings::sync();
    Ok(())
}

pub async fn setup() -> Result<Outcome, String> {
    let destination = data_path()?;
    let bus = gio::bus_get_future(gio::BusType::Session).await.ok();
    let shell = if let Some(bus) = &bus {
        bus.call_future(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "NameHasOwner",
            Some(&(BUS,).to_variant()),
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            3000,
        )
        .await
        .ok()
        .and_then(|v| v.get::<(bool,)>())
        .is_some_and(|v| v.0)
    } else {
        false
    };
    let loaded = if shell {
        info(bus.as_ref().unwrap()).await?
    } else {
        HashMap::new()
    };
    // Check the files Shell originally loaded before overwriting them. GJS caches
    // imported modules: toggling an updated extension would still run old code.
    let metadata: serde_json::Value = serde_json::from_str(FILES[0].1).expect("extension metadata");
    let same_version =
        loaded.get("version").and_then(|v| v.get::<f64>()) == metadata["version"].as_f64();
    let same_code = same_version
        && loaded
            .get("path")
            .and_then(|v| v.get::<String>())
            .is_some_and(|path| matches_bundle(Path::new(&path)));
    install_files(&destination).map_err(|e| e.to_string())?;
    if !shell {
        return Ok(Outcome::NoShell);
    }
    let bus = bus.as_ref().unwrap();
    let version = property(bus, "ShellVersion")
        .await?
        .get::<String>()
        .unwrap_or_default();
    if !metadata["shell-version"]
        .as_array()
        .is_some_and(|versions| {
            versions
                .iter()
                .any(|v| v.as_str() == version.split('.').next())
        })
    {
        return Err(tr(
            "This GNOME Shell version is not supported by the bundled extension.",
        ));
    }
    if property(bus, "UserExtensionsEnabled").await?.get::<bool>() != Some(true) {
        return Err(tr(
            "GNOME extensions are switched off. Turn them on in Extensions, then run clock setup again.",
        ));
    }
    if loaded.is_empty() || !same_code {
        queue_for_next_session()?;
        return Ok(if loaded.is_empty() {
            Outcome::NextSession
        } else {
            Outcome::Updated
        });
    }
    if call(bus, "EnableExtension").await?.get::<(bool,)>() != Some((true,)) {
        return Err(tr(
            "GNOME did not enable the extension. Check Open Meteo Clock in Extensions for details.",
        ));
    }
    // EnableExtension acknowledges the request, not the asynchronous activation.
    for _ in 0..30 {
        let current = info(bus).await?;
        match state(&current) {
            Some(1) => return Ok(Outcome::Enabled),
            Some(3 | 4) => break,
            _ => glib::timeout_future(std::time::Duration::from_millis(100)).await,
        }
    }
    Err(tr(
        "GNOME did not enable the extension. Check Open Meteo Clock in Extensions for details.",
    ))
}
