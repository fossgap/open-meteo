//! Session-bus bridge used by the GNOME clock extension. No network in the shell.
use crate::{
    i18n::{self, tr},
    model::{Units, condition},
    storage::SavedForecast,
    tray::{Command, TrayEvent},
};
use gtk::{gio, glib, prelude::*};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

pub const PATH: &str = "/io/github/open_meteo/OpenMeteo/Weather";
pub const INTERFACE: &str = "io.github.open_meteo.OpenMeteo.Weather1";
pub const EXTENSION_ID: &str = "open-meteo-clock@fossgap.github.io";

const XML: &str = r#"<node><interface name="io.github.open_meteo.OpenMeteo.Weather1">
<method name="GetSnapshot"><arg type="s" direction="out"/></method>
<method name="Attach"><arg type="s" direction="out"/></method>
<method name="Detach"/><method name="Show"/><method name="Refresh"/>
<method name="ShowWithToken"><arg type="s" direction="in"/></method>
<signal name="Changed"><arg type="s"/></signal>
</interface></node>"#;

type Consumers = Rc<RefCell<HashMap<String, Box<dyn FnOnce()>>>>;

pub struct Desktop {
    connection: gio::DBusConnection,
    registration: Option<gio::RegistrationId>,
    snapshot: Rc<RefCell<String>>,
    consumers: Consumers,
}

impl Desktop {
    pub fn new(
        connection: gio::DBusConnection,
        event: impl Fn(TrayEvent) + 'static,
        available: impl Fn(bool) + 'static,
    ) -> Result<Self, glib::Error> {
        let snapshot = Rc::new(RefCell::new(snapshot(None, Units::Celsius, true, false)));
        let consumers: Consumers = Rc::new(RefCell::new(HashMap::new()));
        let data = snapshot.clone();
        let clients = consumers.clone();
        let available = Rc::new(available);
        let info = gio::DBusNodeInfo::for_xml(XML)?;
        let registration = connection
            .register_object(PATH, &info.interfaces()[0])
            .method_call(
                move |connection, sender, _, _, method, parameters, invocation| match method {
                    "GetSnapshot" | "Attach" => {
                        if method == "Attach" {
                            if let Some(sender) = sender {
                                if !clients.borrow().contains_key(sender) {
                                    let lost_clients = clients.clone();
                                    let lost_available = available.clone();
                                    let id = gio::bus_watch_name_on_connection(
                                        &connection,
                                        sender,
                                        gio::BusNameWatcherFlags::NONE,
                                        |_, _, _| {},
                                        move |_, name| {
                                            let id = lost_clients.borrow_mut().remove(name);
                                            if let Some(id) = id {
                                                id();
                                            }
                                            lost_available(!lost_clients.borrow().is_empty());
                                        },
                                    );
                                    clients.borrow_mut().insert(
                                        sender.into(),
                                        Box::new(move || gio::bus_unwatch_name(id)),
                                    );
                                    available(true);
                                }
                            }
                        }
                        invocation.return_value(Some(&(data.borrow().as_str(),).to_variant()));
                    }
                    "Detach" => {
                        if let Some(sender) = sender {
                            let id = clients.borrow_mut().remove(sender);
                            if let Some(id) = id {
                                id();
                            }
                            available(!clients.borrow().is_empty());
                        }
                        invocation.return_value(Some(&().to_variant()));
                    }
                    "Show" | "Refresh" => {
                        invocation.return_value(Some(&().to_variant()));
                        event(if method == "Show" {
                            TrayEvent::Activate(None)
                        } else {
                            // Calendar opens are automatic requests; respect the
                            // user's interval, including manual-only mode.
                            TrayEvent::Command(Command::RefreshIfDue)
                        });
                    }
                    "ShowWithToken" => {
                        let token = parameters.child_get::<String>(0);
                        invocation.return_value(Some(&().to_variant()));
                        event(TrayEvent::Activate((!token.is_empty()).then_some(token)));
                    }
                    _ => invocation.return_dbus_error(
                        "org.freedesktop.DBus.Error.UnknownMethod",
                        "Unknown method",
                    ),
                },
            )
            .build()?;
        Ok(Self {
            connection,
            registration: Some(registration),
            snapshot,
            consumers,
        })
    }

    pub fn update(&self, saved: Option<&SavedForecast>, units: Units, stale: bool, demo: bool) {
        let data = snapshot(saved, units, stale, demo);
        if *self.snapshot.borrow() == data {
            return;
        }
        *self.snapshot.borrow_mut() = data.clone();
        let _ = self.connection.emit_signal(
            None,
            PATH,
            INTERFACE,
            "Changed",
            Some(&(data,).to_variant()),
        );
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        for (_, id) in self.consumers.borrow_mut().drain() {
            id();
        }
        if let Some(id) = self.registration.take() {
            let _ = self.connection.unregister_object(id);
        }
    }
}

fn snapshot(saved: Option<&SavedForecast>, units: Units, stale: bool, demo: bool) -> String {
    let now = if demo {
        saved.map(|s| s.forecast.current.time).unwrap_or_default()
    } else {
        chrono::Utc::now().timestamp()
    };
    snapshot_at(saved, units, stale, demo, now)
}

fn snapshot_at(
    saved: Option<&SavedForecast>,
    units: Units,
    stale: bool,
    demo: bool,
    now: i64,
) -> String {
    let mut data = serde_json::json!({
        "version": 1, "available": saved.is_some(), "language": i18n::code(), "rtl": i18n::rtl(),
        "open": tr("Open forecast"), "refresh": tr("Refresh"), "choose": tr("Choose a location"),
        "weather": tr("Weather"), "hourly": [],
        "empty": tr("This saved hourly forecast has ended. Refresh to get the latest weather."),
    });
    if let Some(saved) = saved {
        let f = &saved.forecast;
        let (description, icon) = condition(f.current.weather_code, f.current.is_day != Some(0));
        let temperature = format!(
            "{}{}",
            units.degrees(f.current.temperature_2m),
            if f.current.temperature_2m.is_some() {
                units.symbol().trim_start_matches('°')
            } else {
                ""
            }
        );
        let status = tr(if demo {
            "sample"
        } else if stale {
            "saved"
        } else {
            "Updated"
        });
        data["source"] = if stale || demo {
            status.clone()
        } else {
            String::new()
        }
        .into();
        data["location"] = saved.location.name.clone().into();
        data["temperature"] = temperature.into();
        data["condition"] = tr(description).into();
        data["icon"] = icon.into();
        data["stale"] = stale.into();
        data["demo"] = demo.into();
        data["hourly"] = f
            .hours(now)
            .into_iter()
            .filter(|&i| f.hourly.time[i] > now)
            .take(5)
            .map(|i| {
                let (description, icon) =
                    condition(f.hourly.weather_code[i], f.hourly.is_day[i] != Some(0));
                serde_json::json!({
                    "time": i18n::time(f, f.hourly.time[i], "%H:%M"),
                    "timestamp": f.hourly.time[i],
                    "temperature": units.degrees(f.hourly.temperature_2m[i]),
                    "condition": tr(description), "icon": icon,
                })
            })
            .collect::<Vec<_>>()
            .into();
        data["status"] = format!(
            "{status} · {}",
            i18n::time(f, saved.fetched_at, "%e %b, %H:%M")
        )
        .into();
    }
    data.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_retains_staleness_units_and_safe_text() {
        let saved = SavedForecast {
            location: crate::model::Location::paris(),
            fetched_at: 0,
            forecast: crate::model::Forecast::sample(),
        };
        let data: serde_json::Value =
            serde_json::from_str(&snapshot(Some(&saved), Units::Fahrenheit, true, false)).unwrap();
        assert_eq!(data["temperature"], "70°F");
        assert_eq!(data["stale"], true);
        assert_eq!(data["location"], "Paris");
        let empty: serde_json::Value =
            serde_json::from_str(&snapshot(None, Units::Celsius, false, false)).unwrap();
        assert_eq!(empty["available"], false);
        assert_eq!(empty["hourly"], serde_json::json!([]));
    }

    #[test]
    fn clock_forecast_shows_five_future_hours_in_location_time_and_selected_units() {
        let mut saved = SavedForecast {
            location: crate::model::Location::paris(),
            fetched_at: 0,
            forecast: crate::model::Forecast::sample(),
        };
        let now = saved.forecast.current.time + 21 * 60;
        let indices: Vec<_> = saved
            .forecast
            .hours(now)
            .into_iter()
            .filter(|&i| saved.forecast.hourly.time[i] > now)
            .take(5)
            .collect();
        saved.forecast.hourly.temperature_2m[indices[1]] = None;
        for units in [Units::Celsius, Units::Fahrenheit] {
            let data: serde_json::Value =
                serde_json::from_str(&snapshot_at(Some(&saved), units, false, false, now)).unwrap();
            let hours = data["hourly"].as_array().unwrap();
            assert_eq!(hours.len(), 5);
            for (hour, &i) in hours.iter().zip(&indices) {
                assert_eq!(hour["timestamp"], saved.forecast.hourly.time[i]);
                assert_eq!(
                    hour["time"],
                    i18n::time(&saved.forecast, saved.forecast.hourly.time[i], "%H:%M")
                );
                assert_eq!(
                    hour["temperature"],
                    units.degrees(saved.forecast.hourly.temperature_2m[i])
                );
                assert_eq!(
                    hour["icon"],
                    condition(
                        saved.forecast.hourly.weather_code[i],
                        saved.forecast.hourly.is_day[i] != Some(0)
                    )
                    .1
                );
            }
            assert_eq!(hours[1]["temperature"], "—");
        }
        let expired: serde_json::Value = serde_json::from_str(&snapshot_at(
            Some(&saved),
            Units::Celsius,
            true,
            false,
            saved.forecast.hourly.time.last().unwrap() + 3600,
        ))
        .unwrap();
        assert_eq!(expired["hourly"], serde_json::json!([]));
    }
}
