import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import Pango from 'gi://Pango';
import St from 'gi://St';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';

const BUS = 'io.github.open_meteo.OpenMeteo';
const PATH = '/io/github/open_meteo/OpenMeteo/Weather';
const INTERFACE = `${BUS}.Weather1`;

export default class OpenMeteoClock extends Extension {
    enable() {
        this._epoch = 0;
        this._cancellable = new Gio.Cancellable();
        const dateMenu = Main.panel.statusArea.dateMenu;
        const clock = dateMenu?._clockDisplay;
        if (!clock?.get_parent())
            throw new Error('Open Meteo: system clock is unavailable');

        // Add to the existing clock container; never replace its label/bindings.
        this._box = new St.BoxLayout({
            style_class: 'open-meteo-clock', visible: false,
            y_align: Clutter.ActorAlign.CENTER,
        });
        this._icon = new St.Icon({style_class: 'open-meteo-clock-icon'});
        this._temperature = new St.Label({y_align: Clutter.ActorAlign.CENTER});
        this._box.add_child(this._icon);
        this._box.add_child(this._temperature);
        const parent = clock.get_parent();
        parent.insert_child_at_index(this._box, parent.get_children().indexOf(clock));

        this._section = new PopupMenu.PopupMenuSection();
        this._summary = new PopupMenu.PopupMenuItem('Open Meteo', {reactive: false});
        this._status = new PopupMenu.PopupMenuItem('', {reactive: false});
        for (const item of [this._summary, this._status]) {
            item.label.add_style_class_name('open-meteo-summary');
            item.label.clutter_text.ellipsize = Pango.EllipsizeMode.END;
        }
        this._open = new PopupMenu.PopupMenuItem('Open Meteo');
        this._refresh = new PopupMenu.PopupMenuItem('');
        this._open.connect('activate', () => this._call('Show'));
        this._refresh.connect('activate', () => this._call('Refresh'));
        for (const item of [this._summary, this._status, this._open, this._refresh])
            this._section.addMenuItem(item);
        dateMenu.menu.addMenuItem(this._section);
        this._section.actor.hide();

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
                        if (signal === 'Changed')
                            this._update(parameters.deep_unpack()[0]);
                    });
                    this._call('Attach', value => this._update(value));
                } catch (error) {
                    if (!error.matches(Gio.IOErrorEnum, Gio.IOErrorEnum.CANCELLED))
                        console.error(`Open Meteo clock: ${error.message}`);
                }
            });
    }

    _call(method, done = null) {
        const epoch = this._epoch;
        this._proxy?.call(method, null, Gio.DBusCallFlags.NO_AUTO_START, 3000,
            this._cancellable, (proxy, result) => {
                try {
                    const reply = proxy.call_finish(result);
                    if (this._box && epoch === this._epoch)
                        done?.(reply.deep_unpack()[0]);
                } catch (error) {
                    if (!error.matches(Gio.IOErrorEnum, Gio.IOErrorEnum.CANCELLED))
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
            this._summary.label.text = data.available
                ? `${data.location} · ${data.temperature} · ${data.condition}` : data.choose;
            this._status.label.text = data.available ? data.status : '';
            this._status.visible = data.available;
            this._open.label.text = data.open;
            this._refresh.label.text = data.refresh;
            this._refresh.setSensitive(data.available);
            this._box.accessible_name = `${this._summary.label.text}. ${this._status.label.text}`;
            if (data.stale)
                this._box.add_style_class_name('open-meteo-clock-saved');
            else
                this._box.remove_style_class_name('open-meteo-clock-saved');
            this._box.show();
            this._section.actor.show();
        } catch (error) {
            console.error(`Open Meteo clock: invalid forecast: ${error.message}`);
        }
    }

    _disconnect() {
        this._epoch++;
        if (this._proxy && this._signal)
            this._proxy.disconnect(this._signal);
        this._signal = 0;
        this._proxy = null;
        this._box?.hide();
        this._section?.actor.hide();
    }

    disable() {
        if (this._watch)
            Gio.bus_unwatch_name(this._watch);
        this._watch = 0;
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
        this._box?.destroy();
        this._box = null;
        this._icon = null;
        this._temperature = null;
        this._summary = this._status = this._open = this._refresh = null;
    }
}
