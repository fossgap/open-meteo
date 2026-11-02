use crate::{
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
                    condition(c.weather_code, c.is_day != Some(0)).0,
                    if self.demo {
                        " (sample)"
                    } else if self.stale {
                        " (saved)"
                    } else {
                        ""
                    }
                )
            })
            .unwrap_or_else(|| "Open Meteo · Choose a location".into())
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
        self.saved
            .as_ref()
            .map(|s| {
                condition(
                    s.forecast.current.weather_code,
                    s.forecast.current.is_day != Some(0),
                )
                .1
                .trim_end_matches("-symbolic")
                .to_string()
            })
            .unwrap_or_else(|| "weather-few-clouds".into())
    }
    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: self.summary(),
            description: "Click to open the forecast".into(),
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
                            f.local_time(f.hourly.time[i], "%H:%M"),
                            self.units.degrees(f.hourly.temperature_2m[i]),
                            condition(f.hourly.weather_code[i], f.hourly.is_day[i] != Some(0)).0
                        ),
                        enabled: false,
                        ..Default::default()
                    }
                    .into()
                })
                .collect();
            items.push(
                SubMenu {
                    label: "Next hours".into(),
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
                            f.day_date(i)
                                .map(|d| d.format("%a").to_string())
                                .unwrap_or_default(),
                            self.units.degrees(f.daily.temperature_2m_max[i]),
                            self.units.degrees(f.daily.temperature_2m_min[i]),
                            condition(f.daily.weather_code[i], true).0
                        ),
                        enabled: false,
                        ..Default::default()
                    }
                    .into()
                })
                .collect();
            items.push(
                SubMenu {
                    label: "Next 7 days".into(),
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
                    label: label.into(),
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
        };
        let handle = tray
            .assume_sni_available(true)
            .spawn()
            .map_err(|error| error.to_string());
        let _ = sender.send(TrayEvent::Ready(handle));
    });
}
