import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Pango from 'gi://Pango';
import St from 'gi://St';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

const BUS = 'io.github.open_meteo.OpenMeteo';
const PATH = '/io/github/open_meteo/OpenMeteo/Weather';
const INTERFACE = `${BUS}.Weather1`;

export default class OpenMeteoClock extends Extension {
    enable() {
        this._epoch = (this._epoch ?? 0) + 1;
        this._cancellable = new Gio.Cancellable();
        const dateMenu = Main.panel.statusArea.dateMenu;
        const clock = dateMenu?._clockDisplay;
        this._dateMenu = dateMenu;
        this._nativeWeather = dateMenu?._weatherItem;
        this._displays = this._nativeWeather?.get_parent();
        if (!clock?.get_parent() || !this._displays)
            throw new Error('Open Meteo: system clock/calendar is unavailable');

        // Add to the existing clock container; never replace its label/bindings.
        this._box = new St.BoxLayout({
            style_class: 'open-meteo-clock', visible: false,
            y_align: Clutter.ActorAlign.CENTER,
        });
        this._icon = new St.Icon({style_class: 'open-meteo-clock-icon'});
        this._temperature = new St.Label({y_align: Clutter.ActorAlign.CENTER});
        this._temperature.clutter_text.ellipsize = Pango.EllipsizeMode.NONE;
        this._box.add_child(this._icon);
        this._box.add_child(this._temperature);
        this._clock = clock;
        this._clockParent = clock.get_parent();
        this._clockStyled = clock.has_style_class_name('clock');
        this._clockGroup = new St.BoxLayout({style_class: 'clock open-meteo-clock-group'});
        const index = this._clockParent.get_children().indexOf(clock);
        this._clockParent.remove_child(clock);
        clock.remove_style_class_name('clock');
        this._clockParent.insert_child_at_index(this._clockGroup, index);
        this._clockGroup.add_child(this._box);
        this._clockGroup.add_child(clock);

        // Use the calendar's weather slot and theme, underneath world clocks.
        // Keep GNOME's own weather actor intact so it can be restored on exit.
        this._section = new St.Button({
            style_class: 'weather-button open-meteo-forecast',
            x_expand: true, can_focus: true, visible: false,
        });
        const content = new St.BoxLayout({style_class: 'weather-box', x_expand: true});
        content.layout_manager.orientation = Clutter.Orientation.VERTICAL;
        this._section.set_child(content);
        const header = new St.BoxLayout({style_class: 'weather-header-box'});
        this._title = new St.Label({style_class: 'weather-header', y_align: Clutter.ActorAlign.END});
        this._title.clutter_text.ellipsize = Pango.EllipsizeMode.NONE;
        this._location = new St.Label({
            style_class: 'weather-header location', x_expand: true,
            x_align: Clutter.ActorAlign.END, y_align: Clutter.ActorAlign.END,
        });
        this._location.clutter_text.ellipsize = Pango.EllipsizeMode.END;
        header.add_child(this._title);
        header.add_child(this._location);
        content.add_child(header);
        const layout = new Clutter.GridLayout();
        this._forecastGrid = new St.Widget({style_class: 'weather-grid', layout_manager: layout});
        layout.hookup_style(this._forecastGrid);
        content.add_child(this._forecastGrid);
        this._status = new St.Label({style_class: 'open-meteo-forecast-status'});
        this._status.clutter_text.ellipsize = Pango.EllipsizeMode.END;
        content.add_child(this._status);
        this._section.connect('clicked', () => {
            this._show();
            Main.overview.hide();
            dateMenu.menu.close();
        });
        this._displays.insert_child_at_index(this._section,
            this._displays.get_children().indexOf(this._nativeWeather));
        this._lastRefresh = 0;
        this._menuSignal = dateMenu.menu.connect('open-state-changed', (_menu, open) => {
            // Like GNOME Weather, refresh when viewed, with a 15-minute limit.
            const now = GLib.get_monotonic_time();
            if (open && this._proxy && (this._lastRefresh === 0 || now - this._lastRefresh >= 15 * 60 * 1000000)) {
                this._lastRefresh = now;
                this._call('Refresh');
            }
        });

        this._watch = Gio.bus_watch_name(Gio.BusType.SESSION, BUS,
            Gio.BusNameWatcherFlags.NONE,
            () => this._connect(), () => this._disconnect());
    }

    _connect() {
        this._disconnect();
        const epoch = this._epoch;
        Gio.DBusProxy.new_for_bus(Gio.BusType.SESSION,
            Gio.DBusProxyFlags.DO_NOT_AUTO_START | Gio.DBusProxyFlags.DO_NOT_LOAD_PROPERTIES,
            null, BUS, PATH, INTERFACE, this._cancellable, (_source, result) => {
                try {
                    const proxy = Gio.DBusProxy.new_for_bus_finish(result);
                    if (!this._box || epoch !== this._epoch)
                        return;
                    this._proxy = proxy;
                    this._signal = proxy.connect('g-signal', (_proxy, _sender, signal, parameters) => {
                        if (signal === 'Changed' && this._attached)
                            this._update(parameters.deep_unpack()[0]);
                    });
                    this._attach();
                } catch (error) {
                    if (!error.matches(Gio.IOErrorEnum, Gio.IOErrorEnum.CANCELLED))
                        console.error(`Open Meteo clock: ${error.message}`);
                }
            });
    }

    _attach() {
        this._call('Attach', value => {
            this._attached = true;
            this._update(value);
        }, error => {
            // GApplication owns its bus name before the GTK activate handler
            // exports Weather1. Wait for that object before showing weather.
            if (error.matches(Gio.DBusError, Gio.DBusError.UNKNOWN_METHOD) ||
                error.matches(Gio.DBusError, Gio.DBusError.UNKNOWN_OBJECT) ||
                error.matches(Gio.DBusError, Gio.DBusError.UNKNOWN_INTERFACE)) {
                this._attachRetry = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 250, () => {
                    this._attachRetry = 0;
                    this._attach();
                    return GLib.SOURCE_REMOVE;
                });
            } else {
                console.error(`Open Meteo clock: ${error.message}`);
            }
        });
    }

    _show() {
        // Carry the clock click's focus permission to the GTK window on Wayland.
        const context = global.create_app_launch_context(global.get_current_time(), -1);
        const info = Gio.DesktopAppInfo.new(`${BUS}.desktop`) ??
            Gio.AppInfo.create_from_commandline('open-meteo', 'Open Meteo',
                Gio.AppInfoCreateFlags.SUPPORTS_STARTUP_NOTIFICATION);
        const token = context.get_startup_notify_id(info, []);
        if (!token) {
            this._call('Show');
            return;
        }
        this._call('ShowWithToken', null, error => {
            context.launch_failed(token);
            if (error.matches(Gio.DBusError, Gio.DBusError.UNKNOWN_METHOD))
                this._call('Show'); // Compatibility with an older running app.
            else
                console.error(`Open Meteo clock: ${error.message}`);
        }, new GLib.Variant('(s)', [token]));
    }

    _call(method, done = null, failed = null, parameters = null) {
        const epoch = this._epoch;
        this._proxy?.call(method, parameters, Gio.DBusCallFlags.NO_AUTO_START, 3000,
            this._cancellable, (proxy, result) => {
                try {
                    const reply = proxy.call_finish(result);
                    if (this._box && epoch === this._epoch)
                        done?.(reply.deep_unpack()[0]);
                } catch (error) {
                    if (!this._box || epoch !== this._epoch ||
                        error.matches(Gio.IOErrorEnum, Gio.IOErrorEnum.CANCELLED))
                        return;
                    if (failed)
                        failed(error);
                    else
                        console.error(`Open Meteo clock: ${error.message}`);
                }
            });
    }

    _update(json) {
        try {
            const data = JSON.parse(json);
            if (data.version !== 1 || typeof data.available !== 'boolean')
                return;
            this._icon.icon_name = data.available ? data.icon : 'weather-few-clouds-symbolic';
            this._temperature.text = data.available ? `${data.stale ? '~' : ''}${data.temperature}` : '—';
            const summary = data.available
                ? `${data.location} · ${data.temperature} · ${data.condition}` : data.choose;
            this._title.text = [data.weather ?? 'Open Meteo',
                (data.stale || data.demo) ? data.source : null].filter(Boolean).join(' · ');
            this._location.text = data.available ? data.location : '';
            this._section.set_text_direction(data.rtl ? Clutter.TextDirection.RTL : Clutter.TextDirection.LTR);
            this._forecastGrid.destroy_all_children();
            const hours = data.available && Array.isArray(data.hourly) ? data.hourly.slice(0, 5) : [];
            const ordered = data.rtl ? [...hours].reverse() : hours;
            ordered.forEach((hour, column) => {
                const time = new St.Label({text: hour.time, style_class: 'weather-forecast-time',
                    x_align: Clutter.ActorAlign.CENTER, x_expand: true});
                const icon = new St.Icon({icon_name: hour.icon, style_class: 'weather-forecast-icon',
                    accessible_name: hour.condition, x_align: Clutter.ActorAlign.CENTER, x_expand: true});
                const temperature = new St.Label({text: hour.temperature, style_class: 'weather-forecast-temp',
                    x_align: Clutter.ActorAlign.CENTER, x_expand: true});
                time.clutter_text.ellipsize = Pango.EllipsizeMode.NONE;
                temperature.clutter_text.ellipsize = Pango.EllipsizeMode.NONE;
                this._forecastGrid.layout_manager.attach(time, column, 0, 1, 1);
                this._forecastGrid.layout_manager.attach(icon, column, 1, 1, 1);
                this._forecastGrid.layout_manager.attach(temperature, column, 2, 1, 1);
            });
            this._forecastGrid.visible = hours.length > 0;
            this._status.text = !data.available ? data.choose
                : hours.length === 0 ? (data.empty ?? summary) : '';
            this._status.visible = this._status.text.length > 0;
            this._box.accessible_name = `${summary}. ${data.status ?? ''}`;
            this._section.accessible_name = [data.open, summary, data.status, this._status.text,
                ...hours.map(h => `${h.time}, ${h.condition}, ${h.temperature}`)].filter(Boolean).join('. ');
            if (data.stale)
                this._box.add_style_class_name('open-meteo-clock-saved');
            else
                this._box.remove_style_class_name('open-meteo-clock-saved');
            this._box.show();
            if (this._nativeWeather.get_parent() === this._displays)
                this._displays.remove_child(this._nativeWeather);
            this._section.show();
        } catch (error) {
            console.error(`Open Meteo clock: invalid forecast: ${error.message}`);
        }
    }

    _disconnect() {
        this._epoch++;
        this._attached = false;
        if (this._attachRetry)
            GLib.source_remove(this._attachRetry);
        this._attachRetry = 0;
        if (this._proxy && this._signal)
            this._proxy.disconnect(this._signal);
        this._signal = 0;
        this._proxy = null;
        this._lastRefresh = 0;
        this._box?.hide();
        this._section?.hide();
        if (this._nativeWeather && !this._nativeWeather.get_parent()) {
            this._displays.insert_child_at_index(this._nativeWeather,
                this._displays.get_children().indexOf(this._section) + 1);
        }
    }

    disable() {
        if (this._watch)
            Gio.bus_unwatch_name(this._watch);
        this._watch = 0;
        if (this._menuSignal)
            this._dateMenu.menu.disconnect(this._menuSignal);
        this._menuSignal = 0;
        // Same bus connection as Attach. No cancellable: release the lease even
        // if an earlier Attach is still being handled by the app.
        if (this._proxy) {
            Gio.DBus.session.call(BUS, PATH, INTERFACE, 'Detach', null, null,
                Gio.DBusCallFlags.NO_AUTO_START, 3000, null, (connection, result) => {
                    try { connection.call_finish(result); } catch { /* App already closed. */ }
                });
        }
        this._cancellable?.cancel();
        this._cancellable = null;
        this._disconnect();
        this._section?.destroy();
        this._section = null;
        if (this._clockGroup) {
            const index = this._clockParent.get_children().indexOf(this._clockGroup);
            this._clockGroup.remove_child(this._clock);
            if (this._clockStyled)
                this._clock.add_style_class_name('clock');
            this._clockParent.insert_child_at_index(this._clock, index);
            this._clockGroup.destroy();
        }
        this._clockGroup = this._clock = this._clockParent = null;
        this._box = null;
        this._icon = null;
        this._temperature = null;
        this._title = this._location = this._status = this._forecastGrid = null;
        this._nativeWeather = this._displays = this._dateMenu = null;
    }
}
