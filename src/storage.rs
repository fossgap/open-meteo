use crate::model::{Forecast, Location, Units};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Preferences {
    pub location: Option<Location>,
    pub recent: Vec<Location>,
    pub units: Units,
    pub keep_in_tray: bool,
    pub language: String,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            location: None,
            recent: vec![],
            units: Units::Celsius,
            keep_in_tray: true,
            language: "system".into(),
        }
    }
}

impl Preferences {
    pub fn select(&mut self, location: Location) {
        self.recent.retain(|l| l.key() != location.key());
        self.recent.insert(0, location.clone());
        self.recent.truncate(5);
        self.location = Some(location);
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SavedForecast {
    pub location: Location,
    pub fetched_at: i64,
    pub forecast: Forecast,
}

#[derive(Clone)]
pub struct Store {
    config: Option<PathBuf>,
    cache: Option<PathBuf>,
}

impl Store {
    pub fn new() -> Self {
        Self {
            config: directory("XDG_CONFIG_HOME", ".config"),
            cache: directory("XDG_CACHE_HOME", ".cache"),
        }
    }

    pub fn preferences(&self) -> Result<Preferences, String> {
        let Some(dir) = &self.config else {
            return Ok(Preferences::default());
        };
        match fs::read(dir.join("settings.json")) {
            Ok(data) => {
                let mut prefs: Preferences = serde_json::from_slice(&data).map_err(|_| {
                    "Saved settings could not be read. Choose a location to start again."
                        .to_string()
                })?;
                prefs.location = prefs.location.filter(Location::valid);
                prefs.recent.retain(Location::valid);
                prefs.recent.truncate(5);
                Ok(prefs)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Preferences::default()),
            Err(_) => {
                Err("Saved settings are unavailable. Changes will be kept for this session.".into())
            }
        }
    }

    pub fn save_preferences(&self, prefs: &Preferences) -> Result<(), String> {
        let dir = self
            .config
            .as_ref()
            .ok_or("No configuration directory is available.")?;
        write_json(&dir.join("settings.json"), prefs).map_err(|e| {
            crate::i18n::tf(
                "Couldn’t save your preferences: {error}",
                &[("error", &e.to_string())],
            )
        })
    }

    pub fn forecast(&self, location: &Location) -> Option<SavedForecast> {
        let data = fs::read(
            self.cache
                .as_ref()?
                .join(format!("{}.json", location.key())),
        )
        .ok()?;
        let saved: SavedForecast = serde_json::from_slice(&data).ok()?;
        if saved.location.key() != location.key() || saved.forecast.validate().is_err() {
            return None;
        }
        Some(saved)
    }

    pub fn save_forecast(&self, saved: &SavedForecast) -> Result<(), String> {
        let dir = self
            .cache
            .as_ref()
            .ok_or("No cache directory is available.")?;
        write_json(&dir.join(format!("{}.json", saved.location.key())), saved).map_err(|e| {
            crate::i18n::tf(
                "Couldn’t save the offline forecast: {error}",
                &[("error", &e.to_string())],
            )
        })
    }
}

fn directory(variable: &str, fallback: &str) -> Option<PathBuf> {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(fallback)))
        .map(|dir| dir.join("open-meteo"))
}

fn write_json(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Invalid storage path"))?;
    fs::create_dir_all(parent)?;
    let data = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&temporary, data)?;
    fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_keep_distinct_recent_locations_in_order() {
        let mut prefs = Preferences::default();
        for i in 0..8 {
            prefs.select(Location {
                latitude: f64::from(i),
                ..Location::paris()
            });
        }
        prefs.select(Location {
            latitude: 5.0,
            ..Location::paris()
        });
        assert_eq!(prefs.recent.len(), 5);
        assert_eq!(prefs.recent[0].latitude, 5.0);
        assert_eq!(prefs.recent[1].latitude, 7.0);
    }

    #[test]
    fn cache_isolated_by_coordinates_and_corrupt_files_are_ignored() {
        let dir = std::env::temp_dir().join(format!("open-meteo-test-{}", std::process::id()));
        let store = Store {
            config: Some(dir.join("config")),
            cache: Some(dir.join("cache")),
        };
        let mut prefs = Preferences::default();
        prefs.select(Location::paris());
        prefs.units = Units::Fahrenheit;
        store.save_preferences(&prefs).unwrap();
        assert_eq!(store.preferences().unwrap().units, Units::Fahrenheit);
        let saved = SavedForecast {
            location: Location::paris(),
            fetched_at: 123,
            forecast: Forecast::sample(),
        };
        store.save_forecast(&saved).unwrap();
        assert!(store.forecast(&Location::paris()).is_some());
        assert!(
            store
                .forecast(&Location {
                    latitude: 0.0,
                    ..Location::paris()
                })
                .is_none()
        );
        fs::write(
            dir.join("cache")
                .join(format!("{}.json", Location::paris().key())),
            b"broken",
        )
        .unwrap();
        assert!(store.forecast(&Location::paris()).is_none());
        fs::write(dir.join("config/settings.json"), b"broken").unwrap();
        assert!(store.preferences().is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
