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
    Ready(Result<ksni::blocking::Handle<WeatherTray>, String>),
    Availability(bool),
    Command(Command),
}

pub struct WeatherTray {
    sender: Sender<TrayEvent>,
    online: Arc<AtomicBool>,
    pub saved: Option<SavedForecast>,
    pub units: Units,
    pub stale: bool,
    pub demo: bool,
    pub clock_active: bool,
    pub icons: Vec<ksni::Icon>,
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
        if !self.icons.is_empty() {
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
        self.icons.clone()
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
            icons: vec![],
        };
        let handle = tray
            .assume_sni_available(true)
            .spawn()
            .map_err(|error| error.to_string());
        let _ = sender.send(TrayEvent::Ready(handle));
    });
}

/// Use the same symbolic weather icon as the window. Icon themes and GTK
/// paintables must be accessed on the GTK thread, before updating the SNI worker.
pub fn panel_icons(saved: Option<&SavedForecast>, units: Units) -> Vec<ksni::Icon> {
    use gtk::prelude::*;
    let Some(saved) = saved else { return vec![] };
    let Some(display) = gtk::gdk::Display::default() else {
        return vec![];
    };
    let current = &saved.forecast.current;
    let name = condition(current.weather_code, current.is_day != Some(0)).1;
    let theme = gtk::IconTheme::for_display(&display);
    let text = units.degrees(current.temperature_2m);
    [24, 32, 48, 64]
        .into_iter()
        .filter_map(|size| {
            let side = (f64::from(size) * 0.58).round();
            let icon = theme.lookup_icon(
                name,
                &[],
                side as i32,
                1,
                gtk::TextDirection::Ltr,
                gtk::IconLookupFlags::FORCE_SYMBOLIC,
            );
            let snapshot = gtk::Snapshot::new();
            icon.snapshot_symbolic(&snapshot, side, side, &[gtk::gdk::RGBA::WHITE; 4]);
            let node = snapshot.to_node()?;
            let weather = gtk::cairo::ImageSurface::create(
                gtk::cairo::Format::ARgb32,
                side as i32,
                side as i32,
            )
            .ok()?;
            node.draw(&gtk::cairo::Context::new(&weather).ok()?);
            panel_icon(&weather, &text, size)
        })
        .collect()
}

/// Most SNI hosts constrain icons to a square, ignoring vendor text labels.
/// Preserve the condition symbol above a temperature badge in that square.
/// GNOME's clock extension has room for the full-size icon and text side by side.
fn panel_icon(weather: &gtk::cairo::ImageSurface, text: &str, size: i32) -> Option<ksni::Icon> {
    use gtk::cairo;
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, size, size).ok()?;
    {
        let cr = cairo::Context::new(&surface).ok()?;
        let x = f64::from(size - weather.width()) / 2.0;
        // Outline both symbol and text so either panel theme remains legible.
        cr.set_source_rgba(0.08, 0.12, 0.17, 1.0);
        let outline = f64::from(size) / 32.0;
        for (dx, dy) in [
            (-outline, 0.0),
            (outline, 0.0),
            (0.0, -outline),
            (0.0, outline),
        ] {
            cr.mask_surface(weather, x + dx, outline + dy).ok()?;
        }
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.mask_surface(weather, x, outline).ok()?;
        let layout = pangocairo::functions::create_layout(&cr);
        let mut font = gtk::pango::FontDescription::from_string("Sans Bold");
        font.set_absolute_size(f64::from(size) * 0.53 * f64::from(gtk::pango::SCALE));
        layout.set_font_description(Some(&font));
        layout.set_text(text);
        let (ink, _) = layout.pixel_extents();
        let scale = ((f64::from(size) - 3.0) / f64::from(ink.width()).max(1.0)).min(1.0);
        font.set_absolute_size(f64::from(size) * 0.53 * scale * f64::from(gtk::pango::SCALE));
        layout.set_font_description(Some(&font));
        let (ink, _) = layout.pixel_extents();
        cr.move_to(
            f64::from(size - ink.width()) / 2.0 - f64::from(ink.x()),
            f64::from(size - ink.height()) - outline - f64::from(ink.y()),
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
    #[test]
    fn panel_icon_preserves_both_weather_and_temperature() {
        let weather = gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, 18, 18).unwrap();
        let cr = gtk::cairo::Context::new(&weather).unwrap();
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.arc(9.0, 8.0, 5.0, 0.0, std::f64::consts::TAU);
        cr.fill().unwrap();
        for text in ["21°", "-20°", "104°", "—", "٢١°"] {
            let icon = panel_icon(&weather, text, 32).unwrap();
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
            panel_icon(&weather, "21°", 32).unwrap().data[18 * 32 * 4..],
            panel_icon(&weather, "70°", 32).unwrap().data[18 * 32 * 4..]
        );
        assert_eq!(
            panel_icon(&weather, "21°", 32).unwrap().data[..16 * 32 * 4],
            panel_icon(&weather, "70°", 32).unwrap().data[..16 * 32 * 4]
        );
        let clear = gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, 18, 18).unwrap();
        assert_ne!(
            panel_icon(&weather, "21°", 32).unwrap().data[..16 * 32 * 4],
            panel_icon(&clear, "21°", 32).unwrap().data[..16 * 32 * 4]
        );
    }
}
