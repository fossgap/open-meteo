use crate::i18n::{self, tf, tr};
use crate::model::{Forecast, Location, Units, compass, condition, percentage};
use adw::prelude::*;
use gtk::{Orientation, cairo};

pub fn label(text: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    if !class.is_empty() {
        label.add_css_class(class);
    }
    label
}

pub fn clear(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

pub fn current(location: &Location, f: &Forecast, units: Units, now: i64) -> gtk::Box {
    let panel = gtk::Box::new(Orientation::Vertical, 5);
    panel.add_css_class("current-weather");
    let name = label(&location.name, "location-name");
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    name.set_tooltip_text(Some(&location.name));
    panel.append(&name);
    let subtitle = label(&location.subtitle(), "location-subtitle");
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
    subtitle.set_tooltip_text(Some(&location.subtitle()));
    panel.append(&subtitle);

    let main = gtk::Box::new(Orientation::Horizontal, 12);
    main.set_margin_top(8);
    let reading = gtk::Box::new(Orientation::Vertical, 0);
    reading.set_hexpand(true);
    let temperature = label(&units.degrees(f.current.temperature_2m), "temperature");
    temperature.set_tooltip_text(Some(&tf(
        "Temperature in {unit}",
        &[(
            "unit",
            &tr(if units == Units::Celsius {
                "Celsius"
            } else {
                "Fahrenheit"
            }),
        )],
    )));
    reading.append(&temperature);
    let (description, icon) = condition(f.current.weather_code, f.current.is_day != Some(0));
    let description = label(&tr(description), "conditions");
    description.set_wrap(true);
    reading.append(&description);
    let feels = label(
        &tf(
            "Feels like {temperature}",
            &[(
                "temperature",
                &units.degrees(f.current.apparent_temperature),
            )],
        ),
        "current-details",
    );
    feels.set_wrap(true);
    feels.set_margin_top(6);
    reading.append(&feels);
    main.append(&reading);
    let visual = gtk::Box::new(Orientation::Vertical, 16);
    visual.set_valign(gtk::Align::Center);
    let image = gtk::Image::from_icon_name(icon);
    image.set_pixel_size(88);
    image.add_css_class("weather-symbol");
    visual.append(&image);
    if let Some(&today) = f.days(now).first() {
        let range = label(
            &tf(
                "H {high}   L {low}",
                &[
                    ("high", &units.degrees(f.daily.temperature_2m_max[today])),
                    ("low", &units.degrees(f.daily.temperature_2m_min[today])),
                ],
            ),
            "current-details",
        );
        range.set_wrap(true);
        range.set_max_width_chars(16);
        range.set_halign(gtk::Align::Center);
        visual.append(&range);
    }
    main.append(&visual);
    panel.append(&main);

    let stats = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .homogeneous(true)
        .min_children_per_line(2)
        .max_children_per_line(4)
        .column_spacing(12)
        .row_spacing(12)
        .build();
    stats.add_css_class("weather-stats");
    let rain = f
        .hours(now)
        .first()
        .and_then(|&i| f.hourly.precipitation_probability[i]);
    for (title, value) in [
        (
            "Wind",
            format!(
                "{} {}",
                units.wind(f.current.wind_speed_10m),
                tr(compass(f.current.wind_direction_10m))
            ),
        ),
        ("Humidity", percentage(f.current.relative_humidity_2m)),
        ("Precipitation chance", percentage(rain)),
        ("Conditions at", i18n::time(f, f.current.time, "%H:%M")),
    ] {
        let item = gtk::Box::new(Orientation::Vertical, 5);
        let caption = label(&tr(title), "stat-label");
        caption.set_wrap(true);
        item.append(&caption);
        item.append(&label(value.trim(), "stat-value"));
        let child = gtk::FlowBoxChild::new();
        child.set_focusable(false);
        child.set_child(Some(&item));
        stats.insert(&child, -1);
    }
    panel.append(&stats);
    panel
}

fn heading(title: &str, subtitle: &str) -> gtk::Box {
    let section = gtk::Box::new(Orientation::Vertical, 5);
    let title = label(&tr(title), "section-title");
    title.set_wrap(true);
    section.append(&title);
    let caption = label(&tr(subtitle), "section-caption");
    caption.set_wrap(true);
    section.append(&caption);
    section
}

pub fn hourly(f: &Forecast, units: Units, now: i64) -> gtk::Box {
    let section = heading("Next 24 hours", "Temperature and chance of precipitation");
    let hours = f.hours(now);
    if hours.is_empty() {
        let empty = label(
            &tr("This saved hourly forecast has ended. Refresh to get the latest weather."),
            "section-caption",
        );
        empty.set_wrap(true);
        empty.set_margin_top(24);
        section.append(&empty);
        return section;
    }
    let row = gtk::Box::new(Orientation::Horizontal, 3);
    row.set_direction(gtk::TextDirection::Ltr);
    for &i in &hours {
        let cell = gtk::Box::new(Orientation::Vertical, 10);
        cell.add_css_class("hour");
        cell.set_width_request(48);
        let time = if f.hourly.time[i] <= now && now < f.hourly.time[i].saturating_add(3600) {
            tr("Now")
        } else {
            i18n::time(f, f.hourly.time[i], "%H:%M")
        };
        let hour = label(&time, "hour-time");
        hour.set_halign(gtk::Align::Center);
        cell.append(&hour);
        let (description, icon) =
            condition(f.hourly.weather_code[i], f.hourly.is_day[i] != Some(0));
        let image = gtk::Image::from_icon_name(icon);
        image.set_pixel_size(24);
        image.add_css_class("weather-symbol");
        cell.append(&image);
        let temperature = label(
            &units.degrees(f.hourly.temperature_2m[i]),
            "hour-temperature",
        );
        temperature.set_halign(gtk::Align::Center);
        cell.append(&temperature);
        let rain = label(&percentage(f.hourly.precipitation_probability[i]), "rain");
        rain.set_halign(gtk::Align::Center);
        cell.append(&rain);
        cell.set_tooltip_text(Some(&tf(
            "{time} · {condition}\n{temperature} · {rain} chance of precipitation",
            &[
                ("time", &i18n::time(f, f.hourly.time[i], "%a %H:%M %Z")),
                ("condition", &tr(description)),
                ("temperature", &units.degrees(f.hourly.temperature_2m[i])),
                ("rain", &percentage(f.hourly.precipitation_probability[i])),
            ],
        )));
        row.append(&cell);
    }
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_width(false)
        .min_content_width(260)
        .child(&row)
        .build();
    scroll.add_css_class("hourly-scroll");
    scroll.set_direction(gtk::TextDirection::Ltr);
    // GTK creates a viewport around the box. Text direction is per-widget,
    // so set it there too to preserve the current hour when resizing in RTL.
    if let Some(viewport) = scroll.child() {
        viewport.set_direction(gtk::TextDirection::Ltr);
    }
    scroll.set_tooltip_text(Some(&tr("Scroll horizontally for all 24 hours")));
    section.append(&scroll);

    let chart = temperature_chart(f, &hours, units);
    section.append(&chart);
    if let Some(&day) = f.days(now).first() {
        let daylight = gtk::Box::new(Orientation::Horizontal, 12);
        daylight.add_css_class("daylight");
        for (title, value) in [
            (
                "Sunrise",
                f.daily.sunrise[day]
                    .map(|t| i18n::time(f, t, "%H:%M"))
                    .unwrap_or_else(|| "—".into()),
            ),
            (
                "Sunset",
                f.daily.sunset[day]
                    .map(|t| i18n::time(f, t, "%H:%M"))
                    .unwrap_or_else(|| "—".into()),
            ),
            (
                "UV index",
                f.daily.uv_index_max[day]
                    .map(|v| i18n::number(&format!("{v:.1}")))
                    .unwrap_or_else(|| "—".into()),
            ),
        ] {
            let item = gtk::Box::new(Orientation::Vertical, 6);
            item.set_hexpand(true);
            let title = label(&tr(title), "stat-label");
            title.set_wrap(true);
            title.set_max_width_chars(12);
            item.append(&title);
            item.append(&label(&value, "stat-value"));
            daylight.append(&item);
        }
        section.append(&daylight);
    }
    section
}

pub fn daily(f: &Forecast, units: Units, now: i64) -> gtk::Box {
    let section = heading("Next 7 days", "Select a day for more details");
    let list = gtk::Box::new(Orientation::Vertical, 0);
    list.add_css_class("daily-list");
    let days = f.days(now);
    let low = days
        .iter()
        .filter_map(|&i| f.daily.temperature_2m_min[i])
        .filter(|v| v.is_finite())
        .fold(f64::INFINITY, f64::min);
    let high = days
        .iter()
        .filter_map(|&i| f.daily.temperature_2m_max[i])
        .filter(|v| v.is_finite())
        .fold(f64::NEG_INFINITY, f64::max);
    let today = f.local_time(now, "%Y-%m-%d");
    for &i in &days {
        let row = gtk::Box::new(Orientation::Horizontal, 8);
        row.add_css_class("daily-row");
        row.set_hexpand(true);
        let name = f
            .day_date(i)
            .map(|d| {
                if d.to_string() == today {
                    tr("Today")
                } else {
                    i18n::weekday(d)
                }
            })
            .unwrap_or_else(|| "—".into());
        let day = label(&name, "day-name");
        day.set_ellipsize(gtk::pango::EllipsizeMode::End);
        day.set_max_width_chars(6);
        day.set_tooltip_text(Some(&name));
        day.set_width_chars(5);
        row.append(&day);
        let (description, icon) = condition(f.daily.weather_code[i], true);
        let image = gtk::Image::from_icon_name(icon);
        image.set_pixel_size(22);
        image.set_tooltip_text(Some(&tr(description)));
        row.append(&image);
        let rain = label(
            &percentage(f.daily.precipitation_probability_max[i]),
            "rain",
        );
        rain.set_xalign(1.0);
        rain.set_width_chars(4);
        row.append(&rain);
        let min = label(
            &units.degrees(f.daily.temperature_2m_min[i]),
            "low-temperature",
        );
        min.set_width_chars(3);
        min.set_xalign(1.0);
        row.append(&min);
        row.append(&range_bar(
            f.daily.temperature_2m_min[i],
            f.daily.temperature_2m_max[i],
            low,
            high,
        ));
        let max = label(
            &units.degrees(f.daily.temperature_2m_max[i]),
            "high-temperature",
        );
        max.set_width_chars(3);
        max.set_xalign(1.0);
        row.append(&max);
        let details = gtk::Box::new(Orientation::Vertical, 5);
        details.add_css_class("day-details");
        details.append(&label(&tr(description), ""));
        let wind = label(
            &tf(
                "Maximum wind: {wind}",
                &[("wind", &units.wind(f.daily.wind_speed_10m_max[i]))],
            ),
            "",
        );
        wind.set_wrap(true);
        details.append(&wind);
        let sunrise = f.daily.sunrise[i]
            .map(|t| i18n::time(f, t, "%H:%M"))
            .unwrap_or_else(|| "—".into());
        let sunset = f.daily.sunset[i]
            .map(|t| i18n::time(f, t, "%H:%M"))
            .unwrap_or_else(|| "—".into());
        let sun = label(
            &tf(
                "Sunrise {sunrise} · Sunset {sunset}",
                &[("sunrise", &sunrise), ("sunset", &sunset)],
            ),
            "",
        );
        sun.set_wrap(true);
        details.append(&sun);
        let expander = gtk::Expander::builder()
            .label_widget(&row)
            .child(&details)
            .hexpand(true)
            .build();
        expander.update_property(&[gtk::accessible::Property::Label(&tf(
            "{day} forecast",
            &[("day", &name)],
        ))]);
        expander.set_tooltip_text(Some(&tf(
            "{day}: {condition}. Low {low}, high {high}. {rain} chance of precipitation.",
            &[
                ("day", &f.day_date(i).map(i18n::date).unwrap_or_default()),
                ("condition", &tr(description)),
                ("low", &units.degrees(f.daily.temperature_2m_min[i])),
                ("high", &units.degrees(f.daily.temperature_2m_max[i])),
                (
                    "rain",
                    &percentage(f.daily.precipitation_probability_max[i]),
                ),
            ],
        )));
        list.append(&expander);
    }
    if days.is_empty() {
        let empty = label(
            &tr("This saved daily forecast has ended. Refresh to get the latest weather."),
            "section-caption",
        );
        empty.set_wrap(true);
        list.append(&empty);
    }
    section.append(&list);
    section
}

fn range_bar(min: Option<f64>, max: Option<f64>, low: f64, high: f64) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder()
        .content_width(42)
        .content_height(8)
        .hexpand(true)
        .valign(gtk::Align::Center)
        .build();
    area.set_draw_func(move |_, cr, width, height| {
        let width = f64::from(width);
        if i18n::rtl() {
            cr.translate(width, 0.0);
            cr.scale(-1.0, 1.0);
        }
        let y = f64::from(height) / 2.0;
        cr.set_line_cap(cairo::LineCap::Round);
        cr.set_line_width(5.0);
        cr.set_source_rgba(0.45, 0.55, 0.65, 0.15);
        cr.move_to(3.0, y);
        cr.line_to(width - 3.0, y);
        let _ = cr.stroke();
        if let (Some(a), Some(b)) = (min, max) {
            if [a, b, low, high].iter().any(|v| !v.is_finite()) {
                return;
            }
            let span = (high - low).max(1.0);
            let x1 = 3.0 + ((a - low) / span).clamp(0.0, 1.0) * (width - 6.0);
            let x2 = 3.0 + ((b - low) / span).clamp(0.0, 1.0) * (width - 6.0);
            let gradient = cairo::LinearGradient::new(0.0, 0.0, width, 0.0);
            gradient.add_color_stop_rgb(0.0, 0.24, 0.57, 0.73);
            gradient.add_color_stop_rgb(1.0, 0.88, 0.63, 0.28);
            let _ = cr.set_source(&gradient);
            cr.move_to(x1, y);
            cr.line_to(x2.max(x1 + 1.0), y);
            let _ = cr.stroke();
        }
    });
    area
}

fn temperature_chart(f: &Forecast, hours: &[usize], units: Units) -> gtk::DrawingArea {
    let values: Vec<Option<f64>> = hours
        .iter()
        .map(|&i| {
            f.hourly.temperature_2m[i]
                .filter(|v| v.is_finite())
                .map(|v| units.convert(v))
        })
        .collect();
    let times: Vec<String> = hours
        .iter()
        .map(|&i| i18n::time(f, f.hourly.time[i], "%H:%M"))
        .collect();
    let area = gtk::DrawingArea::builder()
        .content_height(116)
        .hexpand(true)
        .build();
    area.add_css_class("chart");
    area.set_tooltip_text(Some(&tf("24-hour temperature trend in {unit}. All readings are available in the hourly strip above.", &[("unit", units.symbol())])));
    area.set_draw_func(move |_, cr, width, height| {
        let Some((min, max)) = temperature_bounds(&values) else {
            return;
        };
        let dark = adw::StyleManager::default().is_dark();
        let w = f64::from(width) - 48.0;
        let h = f64::from(height) - 40.0;
        let span = max - min;
        let count = values.len().saturating_sub(1).max(1) as f64;
        let point = |i: usize, v: f64| (34.0 + i as f64 / count * w, 12.0 + (max - v) / span * h);
        let text_layout = pangocairo::functions::create_layout(cr);
        let mut font = gtk::pango::FontDescription::from_string("Sans");
        font.set_absolute_size(11.0 * f64::from(gtk::pango::SCALE));
        text_layout.set_font_description(Some(&font));
        let draw_text = |text: &str, x: f64, baseline: f64| {
            text_layout.set_text(text);
            cr.move_to(
                x,
                baseline - f64::from(text_layout.baseline()) / f64::from(gtk::pango::SCALE),
            );
            pangocairo::functions::show_layout(cr, &text_layout);
        };
        if dark {
            cr.set_source_rgb(0.67, 0.74, 0.80);
        } else {
            cr.set_source_rgb(0.36, 0.43, 0.50);
        }
        for (value, y) in [(max, 16.0), (min, h + 16.0)] {
            draw_text(&i18n::number(&format!("{value:.0}°")), 0.0, y);
        }
        for i in [0, values.len() / 3, 2 * values.len() / 3, values.len() - 1] {
            let x = 34.0 + i as f64 / count * w;
            text_layout.set_text(&times[i]);
            let text_width = f64::from(text_layout.pixel_size().0);
            let text_x =
                (x - text_width / 2.0).clamp(0.0, (f64::from(width) - text_width).max(0.0));
            draw_text(&times[i], text_x, f64::from(height) - 3.0);
        }
        cr.set_source_rgba(0.45, 0.55, 0.65, 0.18);
        cr.set_line_width(1.0);
        for y in [12.0, h + 12.0] {
            cr.move_to(34.0, y);
            cr.line_to(w + 34.0, y);
            let _ = cr.stroke();
        }
        if dark {
            cr.set_source_rgb(0.44, 0.73, 0.93);
        } else {
            cr.set_source_rgb(0.17, 0.42, 0.62);
        }
        cr.set_line_width(2.5);
        cr.set_line_join(cairo::LineJoin::Round);
        cr.set_line_cap(cairo::LineCap::Round);
        let mut connected = false;
        for (i, value) in values.iter().enumerate() {
            if let Some(v) = value {
                let (x, y) = point(i, *v);
                if connected {
                    cr.line_to(x, y);
                } else {
                    cr.move_to(x, y);
                }
                connected = true;
            } else {
                let _ = cr.stroke();
                connected = false;
            }
        }
        let _ = cr.stroke();
    });
    area
}

/// Axis labels and plotted points must share these bounds, even for flat days.
fn temperature_bounds(values: &[Option<f64>]) -> Option<(f64, f64)> {
    let min = values
        .iter()
        .flatten()
        .copied()
        .filter(|v| v.is_finite())
        .reduce(f64::min)?
        .floor();
    let max = values
        .iter()
        .flatten()
        .copied()
        .filter(|v| v.is_finite())
        .reduce(f64::max)?
        .ceil()
        .max(min + 2.0);
    Some((min, max))
}

#[cfg(test)]
mod tests {
    use super::temperature_bounds;

    #[test]
    fn flat_and_narrow_temperature_ranges_keep_an_honest_axis() {
        let (min, max) = temperature_bounds(&[Some(20.0), Some(21.0)]).unwrap();
        assert_eq!((min, max), (20.0, 22.0));
        assert_eq!((21.0 - min) / (max - min), 0.5);
        assert!(temperature_bounds(&[Some(20.0), Some(20.0)]).is_some());
        assert_eq!(temperature_bounds(&[None, Some(f64::NAN)]), None);
        let (min, max) = temperature_bounds(&[Some(-4.3), Some(5.6)]).unwrap();
        assert!(min <= -4.3 && max >= 5.6);
    }
}
