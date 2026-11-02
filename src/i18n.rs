//! Embedded UTF-8 catalogs keep translations available in every package format.
use std::{collections::BTreeMap, sync::OnceLock};

pub const LANGUAGES: [(&str, &str); 7] = [
    ("en", "English"),
    ("fr", "Français"),
    ("de", "Deutsch"),
    ("es", "Español"),
    ("zh", "简体中文"),
    ("hi", "हिन्दी"),
    ("ar", "العربية"),
];
static LANGUAGE: OnceLock<usize> = OnceLock::new();
static CATALOGS: OnceLock<Vec<BTreeMap<String, String>>> = OnceLock::new();

fn catalogs() -> &'static [BTreeMap<String, String>] {
    CATALOGS.get_or_init(|| {
        [
            include_str!("../resources/locales/en.json"),
            include_str!("../resources/locales/fr.json"),
            include_str!("../resources/locales/de.json"),
            include_str!("../resources/locales/es.json"),
            include_str!("../resources/locales/zh.json"),
            include_str!("../resources/locales/hi.json"),
            include_str!("../resources/locales/ar.json"),
        ]
        .iter()
        .map(|json| serde_json::from_str(json).expect("validated translation catalog"))
        .collect()
    })
}

fn resolve(locale: &str) -> Option<usize> {
    let language = locale
        .split(['_', '-', '.', '@'])
        .next()?
        .to_ascii_lowercase();
    if language == "c" || language == "posix" {
        return Some(0);
    }
    LANGUAGES.iter().position(|(code, _)| *code == language)
}

pub fn init(preference: &str) {
    let index = if preference != "system" {
        resolve(preference)
    } else {
        None
    };
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|key| std::env::var(key).ok().filter(|v| !v.is_empty()))
        .unwrap_or_else(|| "en".into());
    let language = std::env::var("LANGUAGE").unwrap_or_default();
    // POSIX's C locale overrides LANGUAGE, as in gettext.
    let index = index.unwrap_or_else(|| {
        if locale == "C" || locale == "POSIX" {
            0
        } else {
            language
                .split(':')
                .chain(std::iter::once(locale.as_str()))
                .find_map(resolve)
                .unwrap_or(0)
        }
    });
    let _ = LANGUAGE.set(index);
}

pub fn code() -> &'static str {
    LANGUAGES[*LANGUAGE.get().unwrap_or(&0)].0
}
pub fn rtl() -> bool {
    code() == "ar"
}

pub fn tr(key: &str) -> String {
    catalogs()[*LANGUAGE.get().unwrap_or(&0)]
        .get(key)
        .map(String::as_str)
        .unwrap_or(key)
        .into()
}

/// Substitute named fields once, so user-supplied braces cannot become templates.
pub fn tf(key: &str, fields: &[(&str, &str)]) -> String {
    interpolate(&tr(key), fields)
}

fn interpolate(template: &str, fields: &[(&str, &str)]) -> String {
    let mut result = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        result.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = rest.find('}') else {
            break;
        };
        let name = &rest[1..end];
        result.push_str(
            fields
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| *value)
                .unwrap_or(&rest[..=end]),
        );
        rest = &rest[end + 1..];
    }
    result.push_str(rest);
    result
}

pub fn number(value: &str) -> String {
    number_for(code(), value)
}

fn number_for(language: &str, value: &str) -> String {
    value
        .chars()
        .map(|c| match (language, c) {
            ("fr" | "de" | "es", '.') => ',',
            ("ar", '0'..='9') => char::from_u32('٠' as u32 + c as u32 - '0' as u32).unwrap(),
            ("ar", '.') => '٫',
            ("ar", '%') => '٪',
            _ => c,
        })
        .collect()
}

pub fn weekday(date: chrono::NaiveDate) -> String {
    tr(&date.format("%a").to_string())
}

pub fn date(date: chrono::NaiveDate) -> String {
    use chrono::Datelike;
    let day = number(&date.day().to_string());
    let month = tr(&date.format("%B").to_string());
    tf("{day} {month}", &[("day", &day), ("month", &month)])
}

pub fn time(forecast: &crate::model::Forecast, timestamp: i64, format: &str) -> String {
    if format == "%e %b, %H:%M" {
        let local_date = forecast.local_time(timestamp, "%Y-%m-%d");
        let date = chrono::NaiveDate::parse_from_str(&local_date, "%Y-%m-%d")
            .ok()
            .map(date)
            .unwrap_or_else(|| "—".into());
        return format!(
            "{date}, {}",
            number(&forecast.local_time(timestamp, "%H:%M"))
        );
    }
    if format == "%a %H:%M %Z" {
        return format!(
            "{} {}",
            tr(&forecast.local_time(timestamp, "%a")),
            number(&forecast.local_time(timestamp, "%H:%M %Z"))
        );
    }
    number(&forecast.local_time(timestamp, format))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_catalog_has_the_same_keys_and_named_fields() {
        let english = &catalogs()[0];
        fn fields(s: &str) -> std::collections::BTreeSet<&str> {
            s.split('{')
                .skip(1)
                .filter_map(|v| v.split_once('}').map(|(key, _)| key))
                .collect()
        }
        for (index, catalog) in catalogs().iter().enumerate() {
            assert_eq!(
                catalog.keys().collect::<Vec<_>>(),
                english.keys().collect::<Vec<_>>(),
                "{} keys",
                LANGUAGES[index].0
            );
            for (key, value) in catalog {
                assert!(!value.trim().is_empty(), "{}: {key}", LANGUAGES[index].0);
                assert_eq!(fields(key), fields(value), "{}: {key}", LANGUAGES[index].0);
            }
        }
    }
    #[test]
    fn locales_and_numbers_support_scripts_and_regional_variants() {
        assert_eq!(resolve("de_DE.UTF-8"), Some(2));
        assert_eq!(resolve("zh-Hans-CN"), Some(4));
        assert_eq!(resolve("ar_EG"), Some(6));
        assert_eq!(resolve("pt_BR"), None);
        assert_eq!(number_for("ar", "-12.5%"), "-١٢٫٥٪");
        assert_eq!(number_for("de", "12.5"), "12,5");
    }
    #[test]
    fn interpolation_does_not_reinterpret_a_location_as_a_template() {
        assert_eq!(
            interpolate(
                "{location} · {temperature}",
                &[("location", "{temperature}"), ("temperature", "12°")]
            ),
            "{temperature} · 12°"
        );
    }
}
