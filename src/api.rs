use crate::model::{Forecast, Location};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::time::Duration;

const FORECAST_URL: &str = "https://api.open-meteo.com/v1/forecast";
const SEARCH_URL: &str = "https://geocoding-api.open-meteo.com/v1/search";

#[derive(Clone)]
pub struct WeatherClient(Client);

impl WeatherClient {
    pub fn new() -> Result<Self, String> {
        Client::builder()
            .user_agent(concat!("open-meteo-linux/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(7))
            .timeout(Duration::from_secs(20))
            .build()
            .map(Self)
            .map_err(|e| e.to_string())
    }

    pub fn search(&self, name: &str) -> Result<Vec<Location>, String> {
        if name.trim().chars().count() < 2 {
            return Ok(vec![]);
        }
        #[derive(Deserialize)]
        struct Results {
            #[serde(default)]
            results: Vec<Location>,
        }
        let results: Results = self
            .0
            .get(SEARCH_URL)
            .query(&[
                ("name", name.trim()),
                ("count", "8"),
                ("language", "en"),
                ("format", "json"),
            ])
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(network_error)?
            .json()
            .map_err(|_| {
                "Location search returned an unreadable response. Try again.".to_string()
            })?;
        Ok(results
            .results
            .into_iter()
            .filter(Location::valid)
            .collect())
    }

    pub fn forecast(&self, location: &Location) -> Result<Forecast, String> {
        if !location.valid() {
            return Err("Choose a valid location to get a forecast.".into());
        }
        let forecast: Forecast = self.0.get(FORECAST_URL)
            .query(&[
                ("latitude", location.latitude.to_string()),
                ("longitude", location.longitude.to_string()),
                ("current", "temperature_2m,relative_humidity_2m,apparent_temperature,is_day,weather_code,wind_speed_10m,wind_direction_10m".into()),
                ("hourly", "temperature_2m,precipitation_probability,weather_code,is_day".into()),
                ("daily", "weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,sunrise,sunset,wind_speed_10m_max,uv_index_max".into()),
                ("timezone", "auto".into()),
                ("timeformat", "unixtime".into()),
                ("forecast_days", "7".into()),
                ("temperature_unit", "celsius".into()),
                ("wind_speed_unit", "kmh".into()),
            ])
            .send().and_then(|r| r.error_for_status())
            .map_err(network_error)?
            .json().map_err(|_| "The weather service returned an unreadable forecast. Try refreshing.".to_string())?;
        forecast.validate()?;
        Ok(forecast)
    }
}

fn network_error(error: reqwest::Error) -> String {
    eprintln!("Weather request: {error}");
    if error.is_timeout() {
        "The weather service took too long to respond. Try again.".into()
    } else if error.status() == Some(reqwest::StatusCode::TOO_MANY_REQUESTS) {
        "The weather service is busy. Please try again in a few minutes.".into()
    } else if error.is_connect() {
        "Couldn’t connect. Check your internet connection and try again.".into()
    } else {
        "The weather service is temporarily unavailable. Try again shortly.".into()
    }
}
