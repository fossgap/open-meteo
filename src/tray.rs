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
    RefreshIfDue,
    Quit,
}

pub enum TrayEvent {
    Ready(Result<ksni::blocking::Handle<WeatherTray>, String>),
    Stopped,
    Availability(bool),
    Activate(Option<String>),
    Command(Command),
}

pub struct WeatherTray {
    sender: Sender<TrayEvent>,
    online: Arc<AtomicBool>,
    pub saved: Option<SavedForecast>,
    pub units: Units,
    pub stale: bool,
    pub demo: bool,
    pub icons: Vec<ksni::Icon>,
    native_label: bool,
}

impl WeatherTray {
    fn send_command(&self, command: Command) {
        let event = if matches!(command, Command::Show) {
            // Carry the host's Wayland focus permission onto the GTK thread.
            TrayEvent::Activate(ksni::current_xdg_activation_token())
        } else {
            TrayEvent::Command(command)
        };
        let _ = self.sender.send(event);
    }

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
    fn icon_name(&self) -> String {
        if !self.native_label && !self.icons.is_empty() {
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
        if self.native_label {
            vec![]
        } else {
            self.icons.clone()
        }
    }
    fn label(&self) -> String {
        if self.native_label {
            self.units.degrees(
                self.saved
                    .as_ref()
                    .and_then(|saved| saved.forecast.current.temperature_2m),
            )
        } else {
            String::new()
        }
    }
    fn label_guide(&self) -> String {
        if self.native_label {
            "−100°".into()
        } else {
            String::new()
        }
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
        self.send_command(Command::Show);
    }
    fn watcher_online(&self) {
        self.online.store(true, Ordering::Relaxed);
        let _ = self.sender.send(TrayEvent::Availability(true));
    }
    fn watcher_offline(&self, _: ksni::OfflineReason) -> bool {
        self.online.store(false, Ordering::Relaxed);
        let _ = self.sender.send(TrayEvent::Availability(false));
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
                        tray.send_command(command);
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
    // ksni reports initial registration failures, then later host changes; it
    // does not call watcher_online for the initial successful registration.
    online.store(true, Ordering::Relaxed);
    std::thread::spawn(move || {
        let tray = WeatherTray {
            sender: sender.clone(),
            online,
            saved: None,
            units: Units::Celsius,
            stale: true,
            demo,
            icons: vec![],
            native_label: desktop_has_native_labels(
                &std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
            ),
        };
        let handle = tray
            .assume_sni_available(true)
            .spawn()
            .map_err(|error| error.to_string());
        let _ = sender.send(TrayEvent::Ready(handle));
    });
}

pub fn stop(handle: ksni::blocking::Handle<WeatherTray>, sender: Sender<TrayEvent>) {
    let shutdown = handle.shutdown();
    std::thread::spawn(move || {
        shutdown.wait();
        let _ = sender.send(TrayEvent::Stopped);
    });
}

// GNOME's AppIndicator extension and Unity render XAyatanaLabel inside the
// item's button. Icon-only SNI hosts (including XFCE's standard systray and
// Plasma) need the combined horizontal pixmap to retain both readings.
fn desktop_has_native_labels(desktop: &str) -> bool {
    desktop
        .split(':')
        .any(|name| name.eq_ignore_ascii_case("gnome") || name.eq_ignore_ascii_case("unity"))
}

/// Snapshot the native condition symbol on the GTK thread, then combine it
/// with the temperature in a single horizontal image and StatusNotifier item.
pub fn panel_icons(saved: Option<&SavedForecast>, units: Units) -> Vec<ksni::Icon> {
    use gtk::prelude::*;
    let Some(display) = gtk::gdk::Display::default() else {
        return vec![];
    };
    let current = saved.map(|saved| &saved.forecast.current);
    let name = current
        .map(|current| condition(current.weather_code, current.is_day != Some(0)).1)
        .unwrap_or("weather-few-clouds-symbolic");
    let theme = gtk::IconTheme::for_display(&display);
    let text = units.degrees(current.and_then(|current| current.temperature_2m));
    [24, 32, 48, 64]
        .into_iter()
        .filter_map(|size| {
            let side = weather_size(size);
            let icon = theme.lookup_icon(
                name,
                &[],
                side,
                1,
                gtk::TextDirection::Ltr,
                gtk::IconLookupFlags::FORCE_SYMBOLIC,
            );
            let snapshot = gtk::Snapshot::new();
            icon.snapshot_symbolic(
                &snapshot,
                f64::from(side),
                f64::from(side),
                &[gtk::gdk::RGBA::WHITE; 4],
            );
            let weather =
                gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, side, side).ok()?;
            snapshot
                .to_node()?
                .draw(&gtk::cairo::Context::new(&weather).ok()?);
            panel_icon(&weather, &text, size)
        })
        .collect()
}

fn weather_size(size: i32) -> i32 {
    (f64::from(size) * 0.38).round() as i32
}

/// The host owns the tray slot's size. Keep both elements on the same baseline
/// within that slot; one bitmap means one hover area, menu and activation target.
fn panel_icon(weather: &gtk::cairo::ImageSurface, text: &str, size: i32) -> Option<ksni::Icon> {
    use gtk::cairo;
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, size, size).ok()?;
    {
        let cr = cairo::Context::new(&surface).ok()?;
        let outline = f64::from(size) / 48.0;
        let weather_x = outline;
        let weather_y = f64::from(size - weather.height()) / 2.0;
        cr.set_source_rgba(0.08, 0.12, 0.17, 1.0);
        for (dx, dy) in [
            (-outline, 0.0),
            (outline, 0.0),
            (0.0, -outline),
            (0.0, outline),
        ] {
            cr.mask_surface(weather, weather_x + dx, weather_y + dy)
                .ok()?;
        }
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.mask_surface(weather, weather_x, weather_y).ok()?;

        let text_x = f64::from(weather.width()) + outline * 3.0;
        let text_width = f64::from(size) - text_x - outline;
        let layout = pangocairo::functions::create_layout(&cr);
        let mut font = gtk::pango::FontDescription::from_string("Sans Condensed Bold");
        font.set_absolute_size(f64::from(size) * 0.70 * f64::from(gtk::pango::SCALE));
        layout.set_font_description(Some(&font));
        layout.set_text(text);
        let (ink, _) = layout.pixel_extents();
        let scale = (text_width / f64::from(ink.width()).max(1.0)).min(1.0);
        font.set_absolute_size(f64::from(size) * 0.70 * scale * f64::from(gtk::pango::SCALE));
        layout.set_font_description(Some(&font));
        let (ink, _) = layout.pixel_extents();
        cr.move_to(
            text_x + (text_width - f64::from(ink.width())) / 2.0 - f64::from(ink.x()),
            f64::from(size - ink.height()) / 2.0 - f64::from(ink.y()),
        );
        pangocairo::functions::layout_path(&cr, &layout);
        // A dark outline preserves legibility on both light and dark panels.
        cr.set_source_rgba(0.08, 0.12, 0.17, 1.0);
        cr.set_line_width(outline * 1.5);
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
    fn single_item_keeps_its_icon_and_temperature_with_shared_activation() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let mut tray = WeatherTray {
            sender,
            online: Arc::new(AtomicBool::new(false)),
            saved: None,
            units: Units::Celsius,
            stale: true,
            demo: false,
            icons: vec![],
            native_label: false,
        };
        assert!(tray.icon_name().starts_with("weather-"));
        tray.icons = vec![panel_icon(&weather(32), "21°", 32).unwrap()];
        assert!(tray.icon_name().is_empty(), "Show the combined pixmap");
        assert_eq!(tray.icon_pixmap().len(), 1);
        assert_eq!(tray.status(), ksni::Status::Active);
        tray.native_label = true;
        assert!(tray.icon_name().starts_with("weather-"));
        assert!(
            tray.icon_pixmap().is_empty(),
            "Native labels must not duplicate a bitmap temperature"
        );
        assert!(!tray.label().is_empty());
        assert!(desktop_has_native_labels("ubuntu:GNOME"));
        assert!(desktop_has_native_labels("Unity"));
        assert!(!desktop_has_native_labels("XFCE"));
        assert!(!desktop_has_native_labels("KDE"));
        tray.activate(0, 0);
        assert!(matches!(
            receiver.try_recv().unwrap(),
            TrayEvent::Activate(None)
        ));
        assert!(
            receiver.try_recv().is_err(),
            "One click must request one activation"
        );
    }

    fn weather(size: i32) -> gtk::cairo::ImageSurface {
        let side = weather_size(size);
        let surface =
            gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, side, side).unwrap();
        let cr = gtk::cairo::Context::new(&surface).unwrap();
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.arc(
            f64::from(side) / 2.0,
            f64::from(side) / 2.0,
            f64::from(side) / 3.0,
            0.0,
            std::f64::consts::TAU,
        );
        cr.fill().unwrap();
        surface
    }

    #[test]
    fn symbol_and_temperature_share_one_horizontal_bitmap() {
        for size in [24, 32, 48, 64] {
            for text in ["21°", "-20°", "104°", "—", "٢١°"] {
                let icon = panel_icon(&weather(size), text, size).unwrap();
                assert_eq!(icon.data.len(), (size * size * 4) as usize);
                assert!(icon.data.chunks_exact(4).any(|p| p[0] > 0));
                assert!(icon.data.chunks_exact(4).any(|p| p[0] == 0));
                assert!(
                    icon.data
                        .chunks_exact(4)
                        .all(|p| p[1..].iter().all(|v| *v <= p[0]))
                );
                let split = weather_size(size) + size / 48 + 1;
                let bounds = |left: bool| {
                    let rows: Vec<_> = icon
                        .data
                        .chunks_exact(4)
                        .enumerate()
                        .filter(|(i, p)| p[0] > 0 && ((*i as i32 % size < split) == left))
                        .map(|(i, _)| i as i32 / size)
                        .collect();
                    (*rows.iter().min().unwrap(), *rows.iter().max().unwrap())
                };
                let (weather_top, weather_bottom) = bounds(true);
                let (text_top, text_bottom) = bounds(false);
                assert!(
                    (weather_top + weather_bottom - text_top - text_bottom).abs() <= 3,
                    "Weather and temperature must be centered on the same line"
                );
            }
            let celsius = panel_icon(&weather(size), "21°", size).unwrap();
            let fahrenheit = panel_icon(&weather(size), "70°", size).unwrap();
            assert_ne!(celsius.data, fahrenheit.data);
            for (c, f) in celsius
                .data
                .chunks_exact((size * 4) as usize)
                .zip(fahrenheit.data.chunks_exact((size * 4) as usize))
            {
                let symbol_end = (weather_size(size) * 4) as usize;
                assert_eq!(
                    &c[..symbol_end],
                    &f[..symbol_end],
                    "Units must not replace the weather symbol"
                );
            }
        }
    }
}
