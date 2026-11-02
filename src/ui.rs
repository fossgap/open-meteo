use crate::{
    api::WeatherClient,
    components::{self, clear, label},
    i18n::{self, tf, tr},
    model::{Forecast, Location, Units},
    storage::{Preferences, SavedForecast, Store},
    tray::{self, Command, TrayEvent},
};
use adw::prelude::*;
use gtk::{Orientation, gio, glib};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

enum Event {
    Forecast {
        generation: u64,
        location: Location,
        result: Box<Result<Forecast, String>>,
    },
    Search {
        generation: u64,
        result: Result<Vec<Location>, String>,
    },
}

struct State {
    prefs: Preferences,
    saved: Option<SavedForecast>,
    loading: bool,
    stale: bool,
    error: Option<String>,
    generation: u64,
    search_generation: u64,
    search_results: Vec<Location>,
    tray: Option<tray::PanelHandle>,
}

struct Ui {
    app: adw::Application,
    window: adw::ApplicationWindow,
    overlay: adw::ToastOverlay,
    content: gtk::Box,
    notice: gtk::Box,
    notice_text: gtk::Label,
    notice_retry: gtk::Button,
    refresh_button: gtk::Button,
    search_button: gtk::MenuButton,
    search_entry: gtk::SearchEntry,
    search_results: gtk::Box,
    tray_status: gtk::Label,
    tray_action: gio::SimpleAction,
    state: RefCell<State>,
    store: Store,
    client: Result<WeatherClient, String>,
    sender: mpsc::Sender<Event>,
    online: Arc<AtomicBool>,
    demo: bool,
    clock_online: Cell<bool>,
    desktop: RefCell<Option<crate::desktop::Desktop>>,
}

pub fn build(app: &adw::Application, demo: bool) {
    gtk::Widget::set_default_direction(if i18n::rtl() {
        gtk::TextDirection::Rtl
    } else {
        gtk::TextDirection::Ltr
    });
    install_style();
    let store = Store::new();
    let loaded = if demo {
        Ok(Preferences::default())
    } else {
        store.preferences()
    };
    let initial_error = loaded.as_ref().err().cloned();
    let prefs = loaded.unwrap_or_default();
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Open Meteo")
        .default_width(960)
        .default_height(790)
        .width_request(360)
        .height_request(420)
        .build();
    window.add_css_class("meteo");
    window.set_icon_name(Some(crate::APP_ID));
    let root = gtk::Box::new(Orientation::Vertical, 0);
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("Open Meteo", "")));
    let search_button = gtk::MenuButton::builder()
        .icon_name("system-search-symbolic")
        .tooltip_text(tr("Search locations (Ctrl+L)"))
        .build();
    header.pack_start(&search_button);
    let refresh_button = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text(tr("Refresh weather (Ctrl+R)"))
        .build();
    refresh_button.add_css_class("flat");
    header.pack_start(&refresh_button);

    let units = gtk::Box::new(Orientation::Horizontal, 0);
    units.add_css_class("linked");
    units.add_css_class("unit-toggle");
    let celsius = gtk::ToggleButton::with_label("°C");
    celsius.set_tooltip_text(Some(&tr("Celsius and kilometres per hour")));
    let fahrenheit = gtk::ToggleButton::with_label("°F");
    fahrenheit.set_tooltip_text(Some(&tr("Fahrenheit and miles per hour")));
    fahrenheit.set_group(Some(&celsius));
    celsius.set_active(prefs.units == Units::Celsius);
    fahrenheit.set_active(prefs.units == Units::Fahrenheit);
    units.append(&celsius);
    units.append(&fahrenheit);
    let menu = gio::Menu::new();
    menu.append(
        Some(&tr("Keep running in system tray")),
        Some("app.keep-in-tray"),
    );
    let languages = gio::Menu::new();
    languages.append(Some(&tr("System language")), Some("app.language::system"));
    for (code, name) in i18n::LANGUAGES {
        languages.append(Some(name), Some(&format!("app.language::{code}")));
    }
    menu.append_submenu(Some(&tr("Language (restart required)")), &languages);
    let general = gio::Menu::new();
    general.append(Some(&tr("GNOME clock setup")), Some("app.clock-setup"));
    general.append(Some(&tr("About Open Meteo")), Some("app.about"));
    general.append(Some(&tr("Quit")), Some("app.quit"));
    menu.append_section(None, &general);
    let menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text(tr("Main menu"))
        .menu_model(&menu)
        .build();
    menu_button.update_property(&[gtk::accessible::Property::Label(&tr("Main menu"))]);
    header.pack_end(&menu_button);
    header.pack_end(&units);
    root.append(&header);

    let notice = gtk::Box::new(Orientation::Horizontal, 12);
    notice.add_css_class("notice");
    let notice_text = label("", "");
    notice_text.set_wrap(true);
    notice_text.set_hexpand(true);
    notice.append(&notice_text);
    let retry = gtk::Button::with_label(&tr("Retry"));
    retry.set_valign(gtk::Align::Center);
    notice.append(&retry);
    notice.set_visible(false);
    root.append(&notice);
    let content = gtk::Box::new(Orientation::Vertical, 0);
    content.add_css_class("forecast-page");
    let clamp = adw::Clamp::builder()
        .maximum_size(1120)
        .tightening_threshold(880)
        .child(&content)
        .build();
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .child(&clamp)
        .build();
    root.append(&scroll);
    let overlay = adw::ToastOverlay::new();
    overlay.set_child(Some(&root));
    window.set_content(Some(&overlay));

    let search_content = gtk::Box::new(Orientation::Vertical, 8);
    search_content.add_css_class("search-content");
    search_content.set_width_request(300);
    search_content.append(&label(&tr("Find a location"), "heading"));
    let search_entry = gtk::SearchEntry::builder()
        .placeholder_text(tr("City or postal code"))
        .max_width_chars(32)
        .build();
    search_entry.set_search_delay(350);
    search_content.append(&search_entry);
    let search_results = gtk::Box::new(Orientation::Vertical, 4);
    let search_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .max_content_height(340)
        .propagate_natural_height(true)
        .child(&search_results)
        .build();
    search_content.append(&search_scroll);
    let popover = gtk::Popover::new();
    popover.set_child(Some(&search_content));
    search_button.set_popover(Some(&popover));

    let tray_action =
        gio::SimpleAction::new_stateful("keep-in-tray", None, &prefs.keep_in_tray.to_variant());
    app.add_action(&tray_action);
    let (sender, receiver) = mpsc::channel();
    let (tray_sender, tray_receiver) = mpsc::channel();
    let online = Arc::new(AtomicBool::new(true));
    let ui = Rc::new(Ui {
        app: app.clone(),
        window,
        overlay,
        content,
        notice,
        notice_text,
        notice_retry: retry.clone(),
        refresh_button,
        search_button,
        search_entry,
        search_results,
        tray_status: label(&tr("Checking system tray…"), "section-caption"),
        tray_action,
        state: RefCell::new(State {
            prefs,
            saved: None,
            loading: false,
            stale: true,
            error: None,
            generation: 0,
            search_generation: 0,
            search_results: vec![],
            tray: None,
        }),
        store,
        client: WeatherClient::new(),
        sender,
        online,
        demo,
        clock_online: Cell::new(false),
        desktop: RefCell::new(None),
    });

    if let Some(connection) = app.dbus_connection() {
        let command_ui = Rc::downgrade(&ui);
        let available_ui = Rc::downgrade(&ui);
        match crate::desktop::Desktop::new(
            connection,
            move |command| {
                if let Some(ui) = command_ui.upgrade() {
                    ui.receive_tray(TrayEvent::Command(command));
                }
            },
            move |online| {
                if let Some(ui) = available_ui.upgrade() {
                    ui.clock_online.set(online);
                    if !online && !ui.online.load(Ordering::Relaxed) && !ui.window.is_visible() {
                        ui.window.present();
                    }
                    ui.update_tray();
                    ui.update_tray_status();
                }
            },
        ) {
            Ok(desktop) => *ui.desktop.borrow_mut() = Some(desktop),
            Err(error) => eprintln!("GNOME clock connection: {error}"),
        }
    }
    let language_action = gio::SimpleAction::new_stateful(
        "language",
        Some(glib::VariantTy::STRING),
        &ui.state.borrow().prefs.language.to_variant(),
    );
    let this = ui.clone();
    language_action.connect_activate(move |action, parameter| {
        let Some(value) = parameter.and_then(|p| p.str()) else {
            return;
        };
        if value != "system" && !i18n::LANGUAGES.iter().any(|(code, _)| *code == value) {
            return;
        }
        this.state.borrow_mut().prefs.language = value.into();
        action.set_state(&value.to_variant());
        this.persist();
        this.toast("Language saved. Quit and reopen Open Meteo to apply it.");
    });
    app.add_action(&language_action);
    let setup = gio::SimpleAction::new("clock-setup", None);
    let this = ui.clone();
    setup.connect_activate(move |action, _| {
        action.set_enabled(false);
        this.toast("Setting up the GNOME clock…");
        let action = action.clone();
        let ui = this.clone();
        glib::MainContext::default().spawn_local(async move {
            let result = crate::gnome_setup::setup().await;
            action.set_enabled(true);
            let message = match result {
                Ok(crate::gnome_setup::Outcome::Enabled) => {
                    ui.toast("GNOME clock integration is enabled.");
                    return;
                }
                Ok(outcome) => outcome.message(),
                Err(error) => tf(
                    "Could not set up the GNOME clock: {error}",
                    &[("error", &error)],
                ),
            };
            let dialog = adw::MessageDialog::new(
                Some(&ui.window),
                Some(&tr("GNOME clock setup")),
                Some(&message),
            );
            dialog.add_response("close", &tr("Close"));
            dialog.present();
        });
    });
    app.add_action(&setup);
    let this = ui.clone();
    ui.refresh_button.connect_clicked(move |_| this.refresh());
    let this = ui.clone();
    retry.connect_clicked(move |_| this.refresh());
    for (button, unit) in [(celsius, Units::Celsius), (fahrenheit, Units::Fahrenheit)] {
        let this = ui.clone();
        button.connect_toggled(move |button| {
            if button.is_active() {
                this.state.borrow_mut().prefs.units = unit;
                this.persist();
                this.render();
                this.update_tray();
            }
        });
    }
    let this = ui.clone();
    ui.search_entry.connect_changed(move |_| {
        let mut state = this.state.borrow_mut();
        state.search_generation += 1;
        state.search_results.clear();
        drop(state);
        this.search_message("Searching…");
    });
    let this = ui.clone();
    ui.search_entry
        .connect_search_changed(move |_| this.search());
    let this = ui.clone();
    ui.search_entry.connect_activate(move |_| {
        let first = this.state.borrow().search_results.first().cloned();
        if let Some(location) = first {
            this.select(location);
        } else {
            this.search();
        }
    });
    let this = ui.clone();
    ui.search_entry
        .connect_stop_search(move |_| this.search_button.popdown());
    let entry = ui.search_entry.clone();
    popover.connect_show(move |_| {
        entry.grab_focus();
    });
    let this = ui.clone();
    ui.tray_action.connect_activate(move |action, _| {
        let value = !this.state.borrow().prefs.keep_in_tray;
        this.state.borrow_mut().prefs.keep_in_tray = value;
        action.set_state(&value.to_variant());
        this.persist();
        this.update_tray_status();
    });
    let this = ui.clone();
    ui.window.connect_close_request(move |window| {
        let state = this.state.borrow();
        if state.prefs.keep_in_tray
            && (this.clock_online.get()
                || (state.tray.is_some() && this.online.load(Ordering::Relaxed)))
        {
            window.set_visible(false);
            glib::Propagation::Stop
        } else {
            this.app.quit();
            glib::Propagation::Proceed
        }
    });
    for (name, shortcut) in [
        ("refresh", "<Primary>r"),
        ("search", "<Primary>l"),
        ("quit", "<Primary>q"),
        ("about", "F1"),
    ] {
        let action = gio::SimpleAction::new(name, None);
        let this = ui.clone();
        action.connect_activate(move |_, _| match name {
            "refresh" => this.refresh(),
            "search" => this.search_button.popup(),
            "quit" => this.app.quit(),
            "about" => this.about(),
            _ => (),
        });
        app.add_action(&action);
        app.set_accels_for_action(&format!("app.{name}"), &[shortcut]);
    }
    let this = Rc::downgrade(&ui);
    glib::timeout_add_local(Duration::from_millis(80), move || {
        let Some(this) = this.upgrade() else {
            return glib::ControlFlow::Break;
        };
        while let Ok(event) = receiver.try_recv() {
            this.receive(event);
        }
        while let Ok(event) = tray_receiver.try_recv() {
            this.receive_tray(event);
        }
        glib::ControlFlow::Continue
    });
    let this = Rc::downgrade(&ui);
    glib::timeout_add_seconds_local(15 * 60, move || {
        let Some(this) = this.upgrade() else {
            return glib::ControlFlow::Break;
        };
        this.refresh();
        glib::ControlFlow::Continue
    });
    let this = ui.clone();
    app.connect_shutdown(move |_| {
        this.desktop.borrow_mut().take();
        drop(this.state.borrow_mut().tray.take());
    });
    tray::start(tray_sender, ui.online.clone(), demo);
    ui.render();
    ui.show_recent();
    ui.window.present();
    if let Some(error) = initial_error {
        ui.toast(&error);
    }
    let selected = ui.state.borrow().prefs.location.clone();
    if demo {
        ui.select(Location::paris());
    } else if let Some(location) = selected {
        ui.select(location);
    }
}

impl Ui {
    fn toast(&self, message: &str) {
        eprintln!("{message}");
        self.overlay.add_toast(adw::Toast::new(&tr(message)));
    }

    fn persist(&self) {
        if !self.demo {
            if let Err(error) = self.store.save_preferences(&self.state.borrow().prefs) {
                self.toast(&error);
            }
        }
    }

    fn select(self: &Rc<Self>, location: Location) {
        if !location.valid() {
            return;
        }
        self.search_button.popdown();
        let saved = if self.demo {
            let forecast = Forecast::sample();
            Some(SavedForecast {
                location: location.clone(),
                fetched_at: forecast.current.time,
                forecast,
            })
        } else {
            self.store.forecast(&location)
        };
        {
            let mut state = self.state.borrow_mut();
            state.prefs.select(location);
            state.saved = saved;
            state.error = None;
            state.loading = false;
            state.stale = !self.demo;
            state.generation += 1;
        }
        self.persist();
        self.render();
        self.update_tray();
        self.refresh();
    }

    fn refresh(self: &Rc<Self>) {
        if self.demo {
            self.render();
            return;
        }
        let mut state = self.state.borrow_mut();
        if state.loading {
            return;
        }
        let Some(location) = state.prefs.location.clone() else {
            return;
        };
        state.loading = true;
        state.error = None;
        state.generation += 1;
        let generation = state.generation;
        drop(state);
        self.render();
        let sender = self.sender.clone();
        let client = self.client.clone();
        std::thread::spawn(move || {
            let result = client.and_then(|client| client.forecast(&location));
            let _ = sender.send(Event::Forecast {
                generation,
                location,
                result: Box::new(result),
            });
        });
    }

    fn search(self: &Rc<Self>) {
        let query = self.search_entry.text().to_string();
        let generation = {
            let mut state = self.state.borrow_mut();
            state.search_generation += 1;
            state.search_results.clear();
            state.search_generation
        };
        clear(&self.search_results);
        if query.trim().chars().count() < 2 {
            self.show_recent();
            return;
        }
        if query.chars().count() > 120 {
            self.search_message("Use a shorter city name or postal code.");
            return;
        }
        if self.demo {
            if "paris".contains(&query.trim().to_lowercase()) {
                self.show_results(vec![Location::paris()]);
            } else {
                self.search_message("Preview uses sample weather for Paris. Run without --demo to search other locations.");
            }
            return;
        }
        self.search_message("Searching…");
        let sender = self.sender.clone();
        let client = self.client.clone();
        std::thread::spawn(move || {
            let result = client.and_then(|client| client.search(&query));
            let _ = sender.send(Event::Search { generation, result });
        });
    }

    fn search_message(&self, message: &str) {
        clear(&self.search_results);
        let hint = label(&tr(message), "search-hint");
        hint.set_wrap(true);
        hint.set_max_width_chars(32);
        self.search_results.append(&hint);
    }

    fn show_recent(self: &Rc<Self>) {
        let locations = self.state.borrow().prefs.recent.clone();
        if locations.is_empty() {
            self.search_message(
                "Search for a city or postal code. Your location is never detected automatically.",
            );
        } else {
            self.show_results(locations);
        }
    }

    fn show_results(self: &Rc<Self>, locations: Vec<Location>) {
        self.state.borrow_mut().search_results = locations.clone();
        clear(&self.search_results);
        if locations.is_empty() {
            self.search_message("No locations found. Try a nearby city or a different spelling.");
            return;
        }
        for location in locations {
            let button = gtk::Button::new();
            button.add_css_class("flat");
            button.add_css_class("location-result");
            let text = gtk::Box::new(Orientation::Vertical, 4);
            let name = label(&location.name, "name");
            name.set_ellipsize(gtk::pango::EllipsizeMode::End);
            name.set_max_width_chars(30);
            let region = label(&location.subtitle(), "region");
            region.set_ellipsize(gtk::pango::EllipsizeMode::End);
            region.set_max_width_chars(34);
            text.append(&name);
            text.append(&region);
            button.set_child(Some(&text));
            button.set_tooltip_text(Some(&format!("{}, {}", location.name, location.subtitle())));
            button.update_property(&[gtk::accessible::Property::Label(&format!(
                "{}, {}",
                location.name,
                location.subtitle()
            ))]);
            let this = self.clone();
            button.connect_clicked(move |_| this.select(location.clone()));
            self.search_results.append(&button);
        }
    }

    fn receive(self: &Rc<Self>, event: Event) {
        match event {
            Event::Search { generation, result } => {
                if generation != self.state.borrow().search_generation {
                    return;
                }
                match result {
                    Ok(locations) => self.show_results(locations),
                    Err(error) => self.search_message(&error),
                }
            }
            Event::Forecast {
                generation,
                location,
                result,
            } => {
                if generation != self.state.borrow().generation {
                    return;
                }
                self.state.borrow_mut().loading = false;
                match *result {
                    Ok(forecast) => {
                        let saved = SavedForecast {
                            location,
                            fetched_at: chrono::Utc::now().timestamp(),
                            forecast,
                        };
                        if let Err(error) = self.store.save_forecast(&saved) {
                            self.toast(&error);
                        }
                        let mut state = self.state.borrow_mut();
                        state.saved = Some(saved);
                        state.stale = false;
                        state.error = None;
                    }
                    Err(error) => {
                        let mut state = self.state.borrow_mut();
                        state.error = Some(error);
                        state.stale = true;
                    }
                }
                self.render();
                self.update_tray();
            }
        }
    }

    fn receive_tray(self: &Rc<Self>, event: TrayEvent) {
        match event {
            TrayEvent::Ready(result) => match result {
                Ok(handle) => {
                    self.state.borrow_mut().tray = Some(handle);
                    self.update_tray();
                }
                Err(error) => {
                    eprintln!("System tray: {error}");
                    self.online.store(false, Ordering::Relaxed);
                }
            },
            TrayEvent::Availability(online) => {
                if !online && !self.clock_online.get() && !self.window.is_visible() {
                    self.window.present();
                }
            }
            TrayEvent::Command(command) => match command {
                Command::Show => self.window.present(),
                Command::Refresh => self.refresh(),
                Command::Quit => self.app.quit(),
            },
        }
        self.update_tray_status();
    }

    fn update_tray(&self) {
        let state = self.state.borrow();
        if let Some(desktop) = self.desktop.borrow().as_ref() {
            desktop.update(
                state.saved.as_ref(),
                state.prefs.units,
                state.stale,
                self.demo,
            );
        }
        if let Some(handle) = &state.tray {
            handle.update(|tray| {
                tray.saved = state.saved.clone();
                tray.units = state.prefs.units;
                tray.stale = state.stale;
                tray.clock_active = self.clock_online.get();
            });
        }
    }

    fn update_tray_status(&self) {
        let state = self.state.borrow();
        let available = self.clock_online.get()
            || (state.tray.is_some() && self.online.load(Ordering::Relaxed));
        self.tray_action.set_enabled(available);
        self.tray_status.set_text(&tr(if !available {
            "System tray unavailable on this desktop"
        } else if self.clock_online.get() && state.prefs.keep_in_tray {
            "Weather is displayed beside the GNOME clock"
        } else if state.prefs.keep_in_tray {
            "Closing the window keeps weather in your system tray"
        } else {
            "Closing the window quits Open Meteo"
        }));
    }

    fn render(self: &Rc<Self>) {
        let state = self.state.borrow();
        self.refresh_button
            .set_sensitive(!state.loading && state.prefs.location.is_some());
        if state.loading {
            let spinner = gtk::Spinner::new();
            spinner.start();
            self.refresh_button.set_child(Some(&spinner));
        } else {
            self.refresh_button.set_icon_name("view-refresh-symbolic");
        }
        self.notice.set_visible(
            state.error.is_some() || self.demo || (state.stale && state.saved.is_some()),
        );
        self.notice_retry.set_visible(state.error.is_some());
        self.notice_retry.set_sensitive(!state.loading);
        if let Some(error) = &state.error {
            self.notice_text.set_text(&format!(
                "{}{}",
                tr(error),
                if state.saved.is_some() {
                    format!(" {}", tr("Showing saved weather."))
                } else {
                    String::new()
                }
            ));
        } else if self.demo {
            self.notice_text.set_text(&tr(
                "Preview · Sample weather for Paris, not a live forecast.",
            ));
        } else if state.stale {
            if let Some(saved) = &state.saved {
                self.notice_text.set_text(&tf(
                    "Saved forecast from {time}. {status}",
                    &[
                        (
                            "time",
                            &i18n::time(&saved.forecast, saved.fetched_at, "%e %b, %H:%M"),
                        ),
                        (
                            "status",
                            &tr(if state.loading {
                                "Checking for updates…"
                            } else {
                                "Refresh for the latest weather."
                            }),
                        ),
                    ],
                ));
            }
        }
        clear(&self.content);
        if let Some(saved) = &state.saved {
            let f = &saved.forecast;
            let now = if self.demo {
                f.current.time
            } else {
                chrono::Utc::now().timestamp()
            };
            self.content.append(&components::current(
                &saved.location,
                f,
                state.prefs.units,
                now,
            ));
            let outlooks = gtk::FlowBox::builder()
                .selection_mode(gtk::SelectionMode::None)
                .homogeneous(true)
                .min_children_per_line(1)
                .max_children_per_line(2)
                .column_spacing(32)
                .row_spacing(28)
                .build();
            outlooks.add_css_class("outlooks");
            for section in [
                components::hourly(f, state.prefs.units, now),
                components::daily(f, state.prefs.units, now),
            ] {
                section.set_width_request(290);
                section.set_valign(gtk::Align::Start);
                let child = gtk::FlowBoxChild::new();
                child.set_focusable(false);
                child.set_child(Some(&section));
                outlooks.insert(&child, -1);
            }
            self.content.append(&outlooks);
            let footer = gtk::Box::new(Orientation::Vertical, 6);
            footer.add_css_class("footer");
            let stamp = if self.demo {
                tr("Preview · Sample weather, not a live forecast")
            } else {
                tf(
                    "{status} · {updated} · Conditions at {time}",
                    &[
                        (
                            "status",
                            &tr(if state.stale {
                                "Saved forecast"
                            } else {
                                "Updated"
                            }),
                        ),
                        ("updated", &i18n::time(f, saved.fetched_at, "%e %b, %H:%M")),
                        ("time", &i18n::time(f, f.current.time, "%e %b, %H:%M")),
                    ],
                )
            };
            let updated = label(&stamp, "section-caption");
            updated.set_wrap(true);
            footer.append(&updated);
            let timezone = label(
                &tf(
                    "Forecast times in {zone}",
                    &[("zone", &f.timezone.replace('_', " "))],
                ),
                "section-caption",
            );
            timezone.set_wrap(true);
            footer.append(&timezone);
            if self.tray_status.parent().is_some() {
                self.tray_status.unparent();
            }
            self.tray_status.set_wrap(true);
            footer.append(&self.tray_status);
            let sources = gtk::Box::new(Orientation::Horizontal, 4);
            sources.append(&gtk::LinkButton::with_label(
                "https://open-meteo.com/",
                &tr("Weather by Open-Meteo"),
            ));
            sources.append(&gtk::LinkButton::with_label(
                "https://www.geonames.org/",
                "GeoNames",
            ));
            footer.append(&sources);
            self.content.append(&footer);
        } else if state.loading {
            let loading = adw::StatusPage::builder()
                .title(tr("Getting your forecast"))
                .description(
                    state
                        .prefs
                        .location
                        .as_ref()
                        .map(|l| l.name.as_str())
                        .unwrap_or(""),
                )
                .icon_name("weather-few-clouds-symbolic")
                .build();
            self.content.append(&loading);
        } else {
            let page = adw::StatusPage::builder()
                .title(tr(if state.error.is_some() {
                    "Weather is taking a moment"
                } else {
                    "Your weather, at a glance"
                }))
                .description(tr(if state.error.is_some() {
                    "Try refreshing, or choose another location."
                } else {
                    "Find your city for current conditions, the next 24 hours, and the week ahead."
                }))
                .icon_name("weather-few-clouds-symbolic")
                .build();
            let button = gtk::Button::with_label(&tr("Find a location"));
            button.add_css_class("suggested-action");
            button.add_css_class("pill");
            button.set_halign(gtk::Align::Center);
            let this = self.clone();
            button.connect_clicked(move |_| this.search_button.popup());
            page.set_child(Some(&button));
            self.content.append(&page);
        }
    }

    fn about(&self) {
        let about = adw::AboutWindow::builder().transient_for(&self.window).modal(true)
            .application_name("Open Meteo").application_icon("weather-few-clouds")
            .version(env!("CARGO_PKG_VERSION"))
            .comments(format!("{}\n\n{}", tr("A native Linux weather app."), tr("Forecasts by Open-Meteo, location data by GeoNames. An independent client, not affiliated with the Open-Meteo service.")))
            .website("https://open-meteo.com/").build();
        about.add_link(
            &tr("Weather by Open-Meteo"),
            "https://open-meteo.com/en/terms",
        );
        about.present();
    }
}

fn install_style() {
    let provider = gtk::CssProvider::new();
    let manager = adw::StyleManager::default();
    manager.set_color_scheme(adw::ColorScheme::Default);
    let load = |provider: &gtk::CssProvider, dark: bool| {
        let colors = if dark {
            "@define-color meteo_ground #1d252e; @define-color meteo_sky #20384b; @define-color meteo_ink #e8f1f8; @define-color meteo_muted #a9bac9; @define-color meteo_accent #70baed; @define-color meteo_rain #7bc2ef; @define-color meteo_rule alpha(#a9bac9, 0.15); @define-color meteo_notice #4b3b27;"
        } else {
            "@define-color meteo_ground #f7f9fb; @define-color meteo_sky #e5eff7; @define-color meteo_ink #1c3549; @define-color meteo_muted #536878; @define-color meteo_accent #2b6b9e; @define-color meteo_rain #22658f; @define-color meteo_rule alpha(#536878, 0.16); @define-color meteo_notice #fbefda;"
        };
        provider.load_from_data(&format!("{colors}\n{}", include_str!("style.css")));
    };
    load(&provider, manager.is_dark());
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
    manager.connect_dark_notify(move |manager| load(&provider, manager.is_dark()));
}
