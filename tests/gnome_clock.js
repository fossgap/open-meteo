// Run only in the isolated Shell started by scripts/check-gnome.sh.
import Gio from 'gi://Gio';
import Clutter from 'gi://Clutter';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as Scripting from 'resource:///org/gnome/shell/ui/scripting.js';

export const METRICS = {};
const UUID = 'open-meteo-clock@fossgap.github.io';
const BUS = 'io.github.open_meteo.OpenMeteo';
const PATH = '/io/github/open_meteo/OpenMeteo/Weather';
const XML = `<node><interface name="${BUS}.Weather1">
<method name="Attach"><arg type="s" direction="out"/></method>
<method name="Detach"/><method name="Show"/><method name="Refresh"/>
<method name="ShowWithToken"><arg type="s" direction="in"/></method>
<signal name="Changed"><arg type="s"/></signal></interface></node>`;

function assert(value, message) {
    if (!value)
        throw new Error(message);
}

async function until(predicate, message) {
    for (let i = 0; i < 100; i++) {
        if (predicate())
            return;
        await Scripting.sleep(100);
    }
    throw new Error(`Timed out: ${message}`);
}

function shellCall(method) {
    return new Promise((resolve, reject) => {
        Gio.DBus.session.call('org.gnome.Shell', '/org/gnome/Shell', 'org.gnome.Shell.Extensions',
            method, new GLib.Variant('(s)', [UUID]), null, Gio.DBusCallFlags.NO_AUTO_START, 3000,
            null, (connection, result) => {
                try { resolve(connection.call_finish(result).deep_unpack()[0]); }
                catch (error) { reject(error); }
            });
    });
}

export async function run() {
    await until(() => Main.extensionManager.lookup(UUID)?.stateObj?._box, 'extension enabled');
    const extension = Main.extensionManager.lookup(UUID).stateObj;
    const dateMenu = Main.panel.statusArea.dateMenu;
    const clock = dateMenu._clockDisplay;
    const nativeWeather = dateMenu._weatherItem;
    const displays = nativeWeather.get_parent();
    const parent = extension._clockParent;
    const originalCount = parent.get_n_children();
    let attached = 0;
    let shown = 0;
    let activationToken = '';
    let refreshed = 0;
    const data = {version: 1, available: true, location: 'Cannes', temperature: '11°C',
        condition: 'Overcast', icon: 'weather-overcast-symbolic', stale: false,
        status: 'Updated · test fixture', open: 'Open forecast', refresh: 'Refresh', choose: 'Choose a location',
        weather: 'Météo', empty: 'Refresh for the latest weather.', rtl: false,
        hourly: ['22:00', '23:00', '00:00', '01:00', '02:00'].map((time, i) => ({
            time, temperature: '10°', condition: i === 4 ? 'Few clouds' : 'Clear night',
            icon: i === 4 ? 'weather-few-clouds-night-symbolic' : 'weather-clear-night-symbolic',
        }))};
    const service = Gio.DBusExportedObject.wrapJSObject(XML, {
        Attach() { attached++; return JSON.stringify(data); },
        Detach() { attached--; }, Show() { shown++; }, Refresh() { refreshed++; },
        ShowWithToken(token) { activationToken = token; shown++; },
    });
    let owner = Gio.bus_own_name_on_connection(Gio.DBus.session, BUS, Gio.BusNameOwnerFlags.NONE, null, null);
    // GApplication acquires the name before GTK exports the weather object.
    await until(() => extension._attachRetry, 'wait for the weather service during startup');
    assert(!extension._box.visible, 'Do not show clock weather before attaching');
    service.export(Gio.DBus.session, PATH);
    await until(() => extension._temperature.text === '11°C', 'clock temperature');
    assert(extension._icon.icon_name === 'weather-overcast-symbolic' && extension._icon.visible,
        'Weather symbol must remain visible beside the temperature');
    assert(attached === 1, 'Extension must attach once');
    assert(extension._clockGroup.get_children().indexOf(extension._box) + 1 === extension._clockGroup.get_children().indexOf(clock),
        'Weather must be immediately beside the original clock');
    assert(extension._clockGroup.has_style_class_name('clock') && !clock.has_style_class_name('clock'),
        'One shared clock pill must include the weather');
    assert(clock.text.length > 0, 'Native clock text must remain visible');
    Main.overview.hide();
    await Scripting.sleep(300);
    assert(dateMenu.contains(extension._box), 'Weather and clock must share the original clock button');
    const [ix, iy] = extension._icon.get_transformed_position();
    const [iw, ih] = extension._icon.get_transformed_size();
    const [tx, ty] = extension._temperature.get_transformed_position();
    const [tw, th] = extension._temperature.get_transformed_size();
    const [cx, cy] = clock.get_transformed_position();
    assert(ix + iw <= tx && tx + tw <= cx && Math.abs(iy + ih / 2 - ty - th / 2) <= 2,
        'Weather, temperature and clock must be on one horizontal line');
    for (const x of [ix + iw / 2, tx + tw / 2, cx + 2]) {
        let hit = global.stage.get_actor_at_pos(Clutter.PickMode.REACTIVE, x, cy + th / 2);
        while (hit && hit !== dateMenu) hit = hit.get_parent();
        assert(hit === dateMenu, 'Weather and native clock must share the calendar click target');
    }
    dateMenu.menu.open();
    await until(() => refreshed === 1, 'refresh when opening calendar');
    await Scripting.sleep(300);
    assert(extension._section.visible && extension._section.get_parent() === displays,
        'Forecast card belongs inside the calendar column');
    assert(!nativeWeather.get_parent(), 'Only one weather card while Open Meteo is active');
    assert(extension._forecastGrid.get_n_children() === 15, 'Five time/icon/temperature columns');
    assert(extension._title.text === 'Météo' && extension._location.text === 'Cannes', 'Localized card heading');
    const [fx, fy] = extension._section.get_transformed_position();
    const [fw] = extension._section.get_transformed_size();
    const [calx, caly] = dateMenu._calendar.get_transformed_position();
    const [calw, calh] = dateMenu._calendar.get_transformed_size();
    assert(Math.abs(fx - calx) < 25 && fw <= calw + 25 && fy >= caly + calh,
        'Forecast must fit below the calendar, not below the entire menu');
    const output = GLib.getenv('OPEN_METEO_GNOME_SCREENSHOT');
    const capture = async suffix => {
        if (!output) return;
        const [, cardY] = extension._section.get_transformed_position();
        const [, cardHeight] = extension._section.get_transformed_size();
        const [, viewportY] = dateMenu._displaysSection.get_transformed_position();
        const [, viewportHeight] = dateMenu._displaysSection.get_transformed_size();
        assert(cardY + cardHeight <= viewportY + viewportHeight + 1,
            'Entire forecast, including RTL temperatures, must fit above the calendar viewport edge');
        const screenshot = new Shell.Screenshot();
        const file = suffix ? output.replace('.png', `-${suffix}.png`) : output;
        const stream = Gio.File.new_for_path(file).replace(null, false, Gio.FileCreateFlags.NONE, null);
        await screenshot.screenshot_area(0, 0, global.stage.width, global.stage.height, stream);
        stream.close(null);
    };
    const appearance = new Gio.Settings({schema_id: 'org.gnome.desktop.interface'});
    const originalScheme = appearance.get_string('color-scheme');
    for (const scheme of ['prefer-dark', 'prefer-light']) {
        appearance.set_string('color-scheme', scheme);
        await Scripting.sleep(600);
        await capture(scheme === 'prefer-dark' ? '' : 'light');
    }
    appearance.set_string('color-scheme', originalScheme);
    extension._section.emit('clicked', Clutter.BUTTON_PRIMARY);
    await until(() => shown === 1 && refreshed === 1, 'clock actions');
    assert(activationToken.length > 0, 'Calendar clicks forward a Wayland activation token');
    dateMenu.menu.open();
    await Scripting.sleep(100);
    assert(refreshed === 1, 'Reopening the clock must not repeatedly fetch weather');
    data.temperature = '52°F';
    data.icon = 'weather-showers-symbolic';
    data.stale = true;
    data.source = 'enregistrées';
    data.hourly[0].temperature = '50°';
    service.emit_signal('Changed', new GLib.Variant('(s)', [JSON.stringify(data)]));
    await until(() => extension._temperature.text === '~52°F', 'units and saved-weather marker');
    assert(extension._icon.icon_name === 'weather-showers-symbolic', 'Condition changes must update the symbol');
    assert(extension._forecastGrid.get_children().some(child => child.text === '50°'),
        'Hourly temperatures must update with units');
    assert(extension._title.text.includes(data.source) && extension._section.accessible_name.includes(data.status),
        'Saved forecasts need a visible marker and an accessible timestamp');
    data.rtl = true;
    data.weather = 'الطقس';
    data.source = 'محفوظ';
    data.location = 'مدينة طويلة الاسم لاختبار اتجاه النص';
    service.emit_signal('Changed', new GLib.Variant('(s)', [JSON.stringify(data)]));
    await until(() => extension._title.text.startsWith('الطقس'), 'Arabic forecast');
    await Scripting.sleep(300);
    assert(extension._section.get_text_direction() === Clutter.TextDirection.RTL, 'Arabic card direction');
    await capture('rtl');
    data.hourly = [];
    service.emit_signal('Changed', new GLib.Variant('(s)', [JSON.stringify(data)]));
    await until(() => extension._status.text === data.empty, 'expired forecast fallback');
    data.available = false;
    service.emit_signal('Changed', new GLib.Variant('(s)', [JSON.stringify(data)]));
    await until(() => extension._status.text === data.choose && !extension._forecastGrid.visible, 'location empty state');
    Gio.bus_unown_name(owner);
    await until(() => !extension._box.visible, 'app exits');
    assert(nativeWeather.get_parent() === displays, 'Restore GNOME Weather when app exits');
    owner = Gio.bus_own_name_on_connection(Gio.DBus.session, BUS, Gio.BusNameOwnerFlags.NONE, null, null);
    await until(() => extension._box.visible, 'app restarts');
    Main.extensionManager.disableExtension(UUID);
    await until(() => extension._box === null, 'extension disabled');
    assert(parent.get_n_children() === originalCount, 'Disable must restore the original clock container');
    assert(clock.has_style_class_name('clock'), 'Restore the original clock styling');
    assert(clock.get_parent() === parent && clock.text.length > 0, 'Disable must preserve the clock');
    assert(nativeWeather.get_parent() === displays, 'Disable must restore native calendar content');
    // The first owner loss simulates an app restart and forgets its lease.
    await until(() => attached === 1, 'Detach on disable');
    // The app uses this public API: enabling a discovered extension must work
    // immediately without replacing the clock or restarting the Shell.
    const info = await shellCall('GetExtensionInfo');
    assert(info.path.get_string()[0].length > 0 && info.version.get_double() > 0, 'Installer metadata');
    assert(await shellCall('EnableExtension'), 'EnableExtension must accept a discovered extension');
    await until(() => extension._box?.visible, 'extension re-enabled');
    assert(parent.get_n_children() === originalCount, 'Re-enable must not duplicate weather');
    Main.extensionManager.disableExtension(UUID);
    await until(() => extension._box === null, 'final cleanup');
    Gio.bus_unown_name(owner);
    service.unexport();
    print('GNOME_CLOCK_SUITE_PASSED: shared clock button, five-hour calendar forecast, light/dark/RTL, empty and saved data, actions, updates, app restart and cleanup');
}
