use crate::{
    i18n::{self, tr},
    model::{Units, condition},
    storage::SavedForecast,
};
use ksni::{
    MenuItem, ToolTip,
    blocking::TrayMethods,
    menu::{StandardItem, SubMenu},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
};

#[derive(Clone, Copy)]
pub enum Command {
    Show,
    Refresh,
    Quit,
}

pub enum TrayEvent {
    Ready(Result<PanelHandle, String>),
    Availability(bool),
    Command(Command),
}

/// A condition icon and a temperature occupy their own full-size slots. Both
/// carry the same application ID, so grouping hosts keep the pair together.
/// GNOME's clock integration presents the same pair in its native clock layout.
pub struct PanelHandle {
    weather: ksni::blocking::Handle<WeatherTray>,
    temperature: ksni::blocking::Handle<WeatherTray>,
}

impl PanelHandle {
    pub fn update(&self, mut update: impl FnMut(&mut WeatherTray)) {
        self.weather.update(&mut update);
        self.temperature.update(update);
    }
}

impl Drop for PanelHandle {
    fn drop(&mut self) {
        self.weather.shutdown().wait();
        self.temperature.shutdown().wait();
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Part {
    Weather,
    Temperature,
}

#[derive(Clone)]
pub struct WeatherTray {
    sender: Sender<TrayEvent>,
    online: Arc<AtomicBool>,
    pub saved: Option<SavedForecast>,
    pub units: Units,
    pub stale: bool,
    pub demo: bool,
    pub clock_active: bool,
    part: Part,
}

impl WeatherTray {
    fn summary(&self) -> String {
        self.saved
            .as_ref()
            .map(|saved| {
                let c = &saved.forecast.current;
                format!(
                    "{} · {}{} · {}{}",
                    saved.location.name,
                    self.units.degrees(c.temperature_2m),
                    if self.units == Units::Celsius {
                        "C"
                    } else {
                        "F"
                    },
                    tr(condition(c.weather_code, c.is_day != Some(0)).0),
                    if self.demo {
                        format!(" ({})", tr("sample"))
                    } else if self.stale {
                        format!(" ({})", tr("saved"))
                    } else {
                        String::new()
                    }
                )
            })
            .unwrap_or_else(|| format!("Open Meteo · {}", tr("Choose a location")))
    }
}

impl ksni::Tray for WeatherTray {
    fn id(&self) -> String {
        "open-meteo".into()
    }
    fn title(&self) -> String {
        self.summary()
    }
    fn status(&self) -> ksni::Status {
        if self.clock_active {
            ksni::Status::Passive
        } else {
            ksni::Status::Active
        }
    }
    fn icon_name(&self) -> String {
        if self.part == Part::Temperature {
            String::new()
        } else {
            self.saved
                .as_ref()
                .map(|saved| {
                    let current = &saved.forecast.current;
                    condition(current.weather_code, current.is_day != Some(0))
                        .1
                        .to_string()
                })
                .unwrap_or_else(|| "weather-few-clouds-symbolic".into())
        }
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        if self.part == Part::Weather {
            return vec![];
        }
        let text = self.units.degrees(
            self.saved
                .as_ref()
                .and_then(|saved| saved.forecast.current.temperature_2m),
        );
        [24, 32, 48, 64]
            .into_iter()
            .filter_map(|size| temperature_icon(&text, size))
            .collect()
    }
    fn text_direction(&self) -> ksni::TextDirection {
        if i18n::rtl() {
            ksni::TextDirection::RightToLeft
        } else {
            ksni::TextDirection::LeftToRight
        }
    }
    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            icon_name: self
                .saved
                .as_ref()
                .map(|saved| {
                    let current = &saved.forecast.current;
                    condition(current.weather_code, current.is_day != Some(0))
                        .1
                        .to_string()
                })
                .unwrap_or_default(),
            title: self.summary(),
            description: tr("Click to open the forecast"),
            ..Default::default()
        }
    }
    fn activate(&mut self, _: i32, _: i32) {
        let _ = self.sender.send(TrayEvent::Command(Command::Show));
    }
    fn watcher_online(&self) {
        if self.part == Part::Weather {
            self.online.store(true, Ordering::Relaxed);
            let _ = self.sender.send(TrayEvent::Availability(true));
        }
    }
    fn watcher_offline(&self, _: ksni::OfflineReason) -> bool {
        if self.part == Part::Weather {
            self.online.store(false, Ordering::Relaxed);
            let _ = self.sender.send(TrayEvent::Availability(false));
        }
        true
    }
    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items: Vec<MenuItem<Self>> = vec![
            StandardItem {
                label: self.summary(),
                enabled: false,
                ..Default::default()
            }
            .into(),
        ];
        if let Some(saved) = &self.saved {
            let f = &saved.forecast;
            let now = if self.demo {
                f.current.time
            } else {
                chrono::Utc::now().timestamp()
            };
            let hours = f
                .hours(now)
                .iter()
                .take(6)
                .map(|&i| {
                    StandardItem {
                        label: format!(
                            "{}  ·  {}  ·  {}",
                            i18n::time(f, f.hourly.time[i], "%H:%M"),
                            self.units.degrees(f.hourly.temperature_2m[i]),
                            tr(
                                condition(f.hourly.weather_code[i], f.hourly.is_day[i] != Some(0))
                                    .0
                            )
                        ),
                        enabled: false,
                        ..Default::default()
                    }
                    .into()
                })
                .collect();
            items.push(
                SubMenu {
                    label: tr("Next hours"),
                    submenu: hours,
                    ..Default::default()
                }
                .into(),
            );
            let days = f
                .days(now)
                .iter()
                .map(|&i| {
                    StandardItem {
                        label: format!(
                            "{}  ·  {} / {}  ·  {}",
                            f.day_date(i).map(i18n::weekday).unwrap_or_default(),
                            self.units.degrees(f.daily.temperature_2m_max[i]),
                            self.units.degrees(f.daily.temperature_2m_min[i]),
                            tr(condition(f.daily.weather_code[i], true).0)
                        ),
                        enabled: false,
                        ..Default::default()
                    }
                    .into()
                })
                .collect();
            items.push(
                SubMenu {
                    label: tr("Next 7 days"),
                    submenu: days,
                    ..Default::default()
                }
                .into(),
            );
        }
        items.push(MenuItem::Separator);
        for (label, command) in [
            ("Open forecast", Command::Show),
            ("Refresh", Command::Refresh),
            ("Quit", Command::Quit),
        ] {
            items.push(
                StandardItem {
                    label: tr(label),
                    activate: Box::new(move |tray: &mut Self| {
                        let _ = tray.sender.send(TrayEvent::Command(command));
                    }),
                    ..Default::default()
                }
                .into(),
            );
        }
        items
    }
}

pub fn start(sender: Sender<TrayEvent>, online: Arc<AtomicBool>, demo: bool) {
    std::thread::spawn(move || {
        let tray = WeatherTray {
            sender: sender.clone(),
            online,
            saved: None,
            units: Units::Celsius,
            stale: true,
            demo,
            clock_active: false,
            part: Part::Temperature,
        };
        // Grouping panels insert the newest member first. Register temperature
        // first so the weather icon appears to its left in a horizontal tray.
        let temperature = match tray.clone().assume_sni_available(true).spawn() {
            Ok(handle) => handle,
            Err(error) => {
                let _ = sender.send(TrayEvent::Ready(Err(error.to_string())));
                return;
            }
        };
        // Registration is acknowledged before the host creates its widget.
        // Let the temperature finish that asynchronous step before adding the
        // weather symbol to the front of the shared application group. This
        // runs on the tray worker; the forecast window remains responsive.
        std::thread::sleep(std::time::Duration::from_millis(250));
        let weather = WeatherTray {
            part: Part::Weather,
            ..tray
        }
        .assume_sni_available(true)
        .spawn();
        let handle = match weather {
            Ok(weather) => Ok(PanelHandle {
                weather,
                temperature,
            }),
            Err(error) => {
                temperature.shutdown().wait();
                Err(error.to_string())
            }
        };
        let _ = sender.send(TrayEvent::Ready(handle));
    });
}

/// Render just the temperature in its own slot, beside the native weather icon.
fn temperature_icon(text: &str, size: i32) -> Option<ksni::Icon> {
    use gtk::cairo;
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, size, size).ok()?;
    {
        let cr = cairo::Context::new(&surface).ok()?;
        let layout = pangocairo::functions::create_layout(&cr);
        let mut font = gtk::pango::FontDescription::from_string("Sans Bold");
        font.set_absolute_size(f64::from(size) * 0.70 * f64::from(gtk::pango::SCALE));
        layout.set_font_description(Some(&font));
        layout.set_text(text);
        let (ink, _) = layout.pixel_extents();
        let scale = ((f64::from(size) - 3.0) / f64::from(ink.width()).max(1.0)).min(1.0);
        font.set_absolute_size(f64::from(size) * 0.70 * scale * f64::from(gtk::pango::SCALE));
        layout.set_font_description(Some(&font));
        let (ink, _) = layout.pixel_extents();
        cr.move_to(
            f64::from(size - ink.width()) / 2.0 - f64::from(ink.x()),
            f64::from(size - ink.height()) / 2.0 - f64::from(ink.y()),
        );
        pangocairo::functions::layout_path(&cr, &layout);
        // A dark outline preserves legibility on both light and dark panels.
        cr.set_source_rgba(0.08, 0.12, 0.17, 1.0);
        cr.set_line_width(f64::from(size) / 14.0);
        cr.set_line_join(cairo::LineJoin::Round);
        cr.stroke_preserve().ok()?;
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.fill().ok()?;
    }
    // Cairo uses native-endian premultiplied ARGB; SNI requires network ARGB.
    let stride = surface.stride() as usize;
    let pixels = surface.data().ok()?;
    let mut argb = Vec::with_capacity((size * size * 4) as usize);
    for row in pixels.chunks(stride).take(size as usize) {
        for pixel in row[..size as usize * 4].chunks_exact(4) {
            argb.extend_from_slice(&u32::from_ne_bytes(pixel.try_into().ok()?).to_be_bytes());
        }
    }
    Some(ksni::Icon {
        width: size,
        height: size,
        data: argb,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ksni::Tray;

    #[test]
    fn weather_and_temperature_use_separate_full_size_slots() {
        let (sender, _) = std::sync::mpsc::channel();
        let weather = WeatherTray {
            sender,
            online: Arc::new(AtomicBool::new(false)),
            saved: None,
            units: Units::Celsius,
            stale: true,
            demo: false,
            clock_active: false,
            part: Part::Weather,
        };
        let mut temperature = WeatherTray {
            part: Part::Temperature,
            ..weather.clone()
        };
        assert_eq!(
            weather.id(),
            temperature.id(),
            "Keep both slots in the same application group"
        );
        assert!(weather.icon_name().starts_with("weather-"));
        assert!(
            weather.icon_pixmap().is_empty(),
            "Weather must use the full-size themed icon"
        );
        assert!(temperature.icon_name().is_empty());
        assert_eq!(temperature.icon_pixmap().len(), 4);
        temperature.clock_active = true;
        let mut weather = weather;
        weather.clock_active = true;
        assert_eq!(weather.status(), ksni::Status::Passive);
        assert_eq!(temperature.status(), ksni::Status::Passive);
    }

    #[test]
    fn temperature_is_legible_and_changes_with_units() {
        for text in ["21°", "-20°", "104°", "—", "٢١°"] {
            let icon = temperature_icon(text, 32).unwrap();
            assert_eq!(icon.data.len(), 32 * 32 * 4);
            assert!(icon.data.chunks_exact(4).any(|p| p[0] > 0));
            assert!(icon.data.chunks_exact(4).any(|p| p[0] == 0));
            assert!(
                icon.data
                    .chunks_exact(4)
                    .all(|p| p[1..].iter().all(|v| *v <= p[0]))
            );
        }
        assert_ne!(
            temperature_icon("21°", 32).unwrap().data,
            temperature_icon("70°", 32).unwrap().data
        );
    }
}
