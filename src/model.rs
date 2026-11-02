use chrono::{DateTime, FixedOffset, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Location {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    #[serde(default)]
    pub country: String,
    #[serde(default)]
    pub admin1: String,
}

impl Location {
    pub fn subtitle(&self) -> String {
        [&self.admin1, &self.country]
            .into_iter()
            .filter(|part| !part.is_empty() && **part != self.name)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn valid(&self) -> bool {
        !self.name.trim().is_empty()
            && self.name.len() <= 512
            && self.latitude.is_finite()
            && self.longitude.is_finite()
            && (-90.0..=90.0).contains(&self.latitude)
            && (-180.0..=180.0).contains(&self.longitude)
    }

    pub fn key(&self) -> String {
        format!("{:.5}_{:.5}", self.latitude, self.longitude)
    }

    pub fn paris() -> Self {
        Self {
            name: "Paris".into(),
            latitude: 48.85341,
            longitude: 2.3488,
            country: "France".into(),
            admin1: "Île-de-France".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Units {
    #[default]
    Celsius,
    Fahrenheit,
}

impl Units {
    pub fn degrees(self, value: Option<f64>) -> String {
        value
            .filter(|v| v.is_finite())
            .map(|v| crate::i18n::number(&format!("{:.0}°", self.convert(v))))
            .unwrap_or_else(|| "—".into())
    }

    pub fn convert(self, celsius: f64) -> f64 {
        match self {
            Self::Celsius => celsius,
            Self::Fahrenheit => celsius * 1.8 + 32.0,
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            Self::Celsius => "°C",
            Self::Fahrenheit => "°F",
        }
    }

    pub fn wind(self, kmh: Option<f64>) -> String {
        kmh.filter(|v| v.is_finite())
            .map(|v| match self {
                Self::Celsius => format!(
                    "{} {}",
                    crate::i18n::number(&format!("{v:.0}")),
                    crate::i18n::tr("km/h")
                ),
                Self::Fahrenheit => format!(
                    "{} {}",
                    crate::i18n::number(&format!("{:.0}", v / 1.609344)),
                    crate::i18n::tr("mph")
                ),
            })
            .unwrap_or_else(|| "—".into())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Forecast {
    pub timezone: String,
    pub utc_offset_seconds: i32,
    pub current: Current,
    pub hourly: Hourly,
    pub daily: Daily,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Current {
    pub time: i64,
    pub temperature_2m: Option<f64>,
    pub apparent_temperature: Option<f64>,
    pub relative_humidity_2m: Option<f64>,
    pub weather_code: Option<u8>,
    pub is_day: Option<u8>,
    pub wind_speed_10m: Option<f64>,
    pub wind_direction_10m: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Hourly {
    pub time: Vec<i64>,
    pub temperature_2m: Vec<Option<f64>>,
    pub precipitation_probability: Vec<Option<f64>>,
    pub weather_code: Vec<Option<u8>>,
    pub is_day: Vec<Option<u8>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Daily {
    pub time: Vec<i64>,
    pub weather_code: Vec<Option<u8>>,
    pub temperature_2m_max: Vec<Option<f64>>,
    pub temperature_2m_min: Vec<Option<f64>>,
    pub precipitation_probability_max: Vec<Option<f64>>,
    pub sunrise: Vec<Option<i64>>,
    pub sunset: Vec<Option<i64>>,
    pub wind_speed_10m_max: Vec<Option<f64>>,
    pub uv_index_max: Vec<Option<f64>>,
}

impl Forecast {
    pub fn validate(&self) -> Result<(), String> {
        let h = self.hourly.time.len();
        let d = self.daily.time.len();
        if h == 0 || h > 400 || d == 0 || d > 16 {
            return Err(
                "The weather service returned an incomplete forecast. Try refreshing.".into(),
            );
        }
        if [
            self.hourly.temperature_2m.len(),
            self.hourly.precipitation_probability.len(),
            self.hourly.weather_code.len(),
            self.hourly.is_day.len(),
        ]
        .into_iter()
        .any(|len| len != h)
            || [
                self.daily.weather_code.len(),
                self.daily.temperature_2m_max.len(),
                self.daily.temperature_2m_min.len(),
                self.daily.precipitation_probability_max.len(),
                self.daily.sunrise.len(),
                self.daily.sunset.len(),
                self.daily.wind_speed_10m_max.len(),
                self.daily.uv_index_max.len(),
            ]
            .into_iter()
            .any(|len| len != d)
        {
            return Err("The forecast is missing some values. Try refreshing.".into());
        }
        if FixedOffset::east_opt(self.utc_offset_seconds).is_none()
            || std::iter::once(&self.current.time)
                .chain(self.hourly.time.iter())
                .chain(self.daily.time.iter())
                .any(|t| DateTime::from_timestamp(*t, 0).is_none())
            || self.hourly.time.windows(2).any(|t| t[0] >= t[1])
            || self.daily.time.windows(2).any(|t| t[0] >= t[1])
        {
            return Err("The forecast contains invalid dates. Try refreshing.".into());
        }
        Ok(())
    }

    /// Hourly timestamps are UTC instants; convert with the location's IANA zone
    /// so a daylight-saving transition does not shift the remaining forecast.
    pub fn local_time(&self, timestamp: i64, format: &str) -> String {
        let Some(time) = DateTime::<Utc>::from_timestamp(timestamp, 0) else {
            return "—".into();
        };
        if let Ok(zone) = self.timezone.parse::<chrono_tz::Tz>() {
            time.with_timezone(&zone).format(format).to_string()
        } else if let Some(offset) = FixedOffset::east_opt(self.utc_offset_seconds) {
            time.with_timezone(&offset).format(format).to_string()
        } else {
            time.format(format).to_string()
        }
    }

    /// Open-Meteo's daily boundaries use the response's fixed UTC offset,
    /// unlike hourly instants. Keep their original calendar date across DST.
    pub fn day_date(&self, index: usize) -> Option<NaiveDate> {
        let time = *self.daily.time.get(index)?;
        DateTime::from_timestamp(time.checked_add(i64::from(self.utc_offset_seconds))?, 0)
            .map(|time| time.date_naive())
    }

    pub fn hours(&self, now: i64) -> Vec<usize> {
        let times = &self.hourly.time;
        if times
            .last()
            .is_none_or(|last| now >= last.saturating_add(3600))
        {
            return vec![];
        }
        // An API hour can start at :30 or :45 UTC in fractional-offset zones.
        // Select by the actual instants, not a whole-hour UTC rounding.
        let start = times.partition_point(|time| *time <= now).saturating_sub(1);
        (start..times.len()).take(24).collect()
    }

    pub fn days(&self, now: i64) -> Vec<usize> {
        let today = NaiveDate::parse_from_str(&self.local_time(now, "%Y-%m-%d"), "%Y-%m-%d").ok();
        (0..self.daily.time.len())
            .filter(|i| self.day_date(*i) >= today)
            .take(7)
            .collect()
    }

    pub fn sample() -> Self {
        serde_json::from_str(include_str!("../tests/fixtures/forecast.json"))
            .expect("the bundled sample forecast must be valid JSON")
    }
}

pub fn percentage(value: Option<f64>) -> String {
    value
        .filter(|v| v.is_finite())
        .map(|v| crate::i18n::number(&format!("{v:.0}%")))
        .unwrap_or_else(|| "—".into())
}

pub fn compass(degrees: Option<f64>) -> &'static str {
    let Some(degrees) = degrees.filter(|v| v.is_finite()) else {
        return "";
    };
    const DIRECTIONS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    DIRECTIONS[((degrees.rem_euclid(360.0) / 45.0).round() as usize) % 8]
}

pub fn condition(code: Option<u8>, day: bool) -> (&'static str, &'static str) {
    match code {
        Some(0) => {
            if day {
                ("Clear skies", "weather-clear-symbolic")
            } else {
                ("Clear night", "weather-clear-night-symbolic")
            }
        }
        Some(1 | 2) => {
            if day {
                ("Partly cloudy", "weather-few-clouds-symbolic")
            } else {
                ("Partly cloudy", "weather-few-clouds-night-symbolic")
            }
        }
        Some(3) => ("Overcast", "weather-overcast-symbolic"),
        Some(45 | 48) => ("Fog", "weather-fog-symbolic"),
        Some(51 | 53 | 55) => ("Drizzle", "weather-showers-scattered-symbolic"),
        Some(56 | 57 | 66 | 67) => ("Freezing rain", "weather-showers-symbolic"),
        Some(61 | 63 | 65) => ("Rain", "weather-showers-symbolic"),
        Some(71 | 73 | 75 | 77 | 85 | 86) => ("Snow", "weather-snow-symbolic"),
        Some(80..=82) => ("Rain showers", "weather-showers-scattered-symbolic"),
        Some(95 | 96 | 99) => ("Thunderstorms", "weather-storm-symbolic"),
        _ => ("Conditions unavailable", "weather-overcast-symbolic"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_convert_without_turning_missing_values_into_zero() {
        assert_eq!(Units::Fahrenheit.degrees(Some(0.0)), "32°");
        assert_eq!(Units::Celsius.degrees(None), "—");
        assert_eq!(Units::Celsius.degrees(Some(f64::NAN)), "—");
        assert_eq!(Units::Fahrenheit.wind(Some(16.09344)), "10 mph");
    }

    #[test]
    fn forecast_starts_at_the_current_hour_and_has_24_values() {
        let f = Forecast::sample();
        f.validate().unwrap();
        let hours = f.hours(f.current.time + 900);
        assert_eq!(hours.len(), 24);
        assert_eq!(f.hourly.time[hours[0]], f.current.time);
        assert!(f.hours(f.hourly.time.last().unwrap() + 3600).is_empty());
    }

    #[test]
    fn nullable_values_are_allowed_but_mismatched_arrays_are_rejected() {
        let mut f = Forecast::sample();
        f.hourly.temperature_2m[0] = None;
        f.current.temperature_2m = None;
        assert!(f.validate().is_ok());
        f.hourly.weather_code.pop();
        assert!(f.validate().is_err());
    }

    #[test]
    fn current_hour_is_retained_in_fractional_offset_time_zones() {
        let mut f = Forecast::sample();
        for time in &mut f.hourly.time {
            *time += 1800;
        }
        let start = f.hourly.time[12];
        assert_eq!(f.hours(start + 900)[0], 12);
        assert_eq!(f.hours(start + 2700)[0], 12);
        assert_eq!(f.hours(start + 3600)[0], 13);
    }

    #[test]
    fn local_times_follow_the_location_across_daylight_saving() {
        let mut f = Forecast::sample();
        f.timezone = "Europe/Paris".into();
        let before = DateTime::parse_from_rfc3339("2026-03-29T00:00:00Z")
            .unwrap()
            .timestamp();
        assert_eq!(f.local_time(before, "%H:%M"), "01:00");
        assert_eq!(f.local_time(before + 3600, "%H:%M"), "03:00");
        f.timezone = "Asia/Kolkata".into();
        assert_eq!(f.local_time(before, "%H:%M"), "05:30");
    }

    #[test]
    fn daily_dates_keep_the_provider_offset_at_dst_boundaries() {
        let mut f = Forecast::sample();
        f.utc_offset_seconds = 7200;
        f.daily.time[0] = DateTime::parse_from_rfc3339("2026-10-25T22:00:00Z")
            .unwrap()
            .timestamp();
        assert_eq!(f.day_date(0).unwrap().to_string(), "2026-10-26");
    }

    #[test]
    fn unknown_codes_and_polar_nights_are_not_reported_as_sunshine() {
        assert_eq!(condition(None, true).0, "Conditions unavailable");
        assert_eq!(condition(Some(0), false).0, "Clear night");
        assert_eq!(condition(Some(99), true).0, "Thunderstorms");
        assert!(
            !Location {
                latitude: f64::NAN,
                ..Location::paris()
            }
            .valid()
        );
    }
}
