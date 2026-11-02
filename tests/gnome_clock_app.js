// Run the real packaged app with both the clock and AppIndicator extensions.
import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as Scripting from 'resource:///org/gnome/shell/ui/scripting.js';

export const METRICS = {};
const UUID = 'open-meteo-clock@fossgap.github.io';
const INDICATOR = 'appindicatorsupport@rgcjonas.gmail.com';
const BUS = 'io.github.open_meteo.OpenMeteo';
function assert(value, message) { if (!value) throw new Error(message); }
async function until(predicate, message) {
    for (let i = 0; i < 200; i++) {
        if (predicate()) return;
        await Scripting.sleep(100);
    }
    throw new Error(`Timed out: ${message}`);
}
function call(path, iface, method, parameters) {
    return new Promise((resolve, reject) => Gio.DBus.session.call(BUS, path, iface, method,
        parameters, null, Gio.DBusCallFlags.NO_AUTO_START, 3000, null,
        (bus, result) => { try { resolve(bus.call_finish(result)); } catch (e) { reject(e); } }));
}
export async function run() {
    await until(() => Main.extensionManager.lookup(UUID)?.stateObj?._box, 'clock extension enabled');
    assert(Main.extensionManager.lookup(INDICATOR), 'System AppIndicator extension is installed');
    Main.extensionManager.enableExtension(INDICATOR);
    await until(() => Main.extensionManager.lookup(INDICATOR)?.stateObj, 'AppIndicator enabled alongside clock');
    const root = GLib.dir_make_tmp('open-meteo-clock-app-XXXXXX');
    const location = {name: 'Paris', latitude: 48.85341, longitude: 2.3488, country: 'France', admin1: 'Île-de-France'};
    const fixture = Gio.File.new_for_uri(import.meta.url).get_parent().resolve_relative_path('fixtures/forecast.json');
    const [, bytes] = fixture.load_contents(null);
    const forecast = JSON.parse(new TextDecoder().decode(bytes));
    const now = Math.floor(Date.now() / 1000);
    const shift = Math.floor(now / 3600) * 3600 - forecast.current.time;
    forecast.current.time += shift;
    for (const values of [forecast.hourly.time, forecast.daily.time, forecast.daily.sunrise, forecast.daily.sunset])
        values.forEach((value, i) => { if (value !== null) values[i] += shift; });
    for (const folder of ['config', 'cache', 'data'])
        GLib.mkdir_with_parents(`${root}/${folder}/open-meteo`, 0o700);
    GLib.file_set_contents(`${root}/config/open-meteo/settings.json`, JSON.stringify({
        location, units: 'celsius', keep_in_tray: true, language: 'fr', recent: [location],
    }));
    GLib.file_set_contents(`${root}/cache/open-meteo/48.85341_2.34880.json`,
        JSON.stringify({location, forecast, fetched_at: now - 60}));
    const launcher = new Gio.SubprocessLauncher({flags: Gio.SubprocessFlags.NONE});
    for (const [key, value] of Object.entries({
        XDG_CURRENT_DESKTOP: 'GNOME', XDG_CONFIG_HOME: `${root}/config`,
        XDG_CACHE_HOME: `${root}/cache`, XDG_DATA_HOME: `${root}/data`,
        APPIMAGE_EXTRACT_AND_RUN: '1', GDK_BACKEND: 'wayland', GSK_RENDERER: 'cairo', GIO_USE_VFS: 'local',
        HTTPS_PROXY: 'http://127.0.0.1:9', HTTP_PROXY: 'http://127.0.0.1:9', ALL_PROXY: 'http://127.0.0.1:9', NO_PROXY: '',
    })) launcher.setenv(key, value, true);
    const app = launcher.spawnv([GLib.getenv('OPEN_METEO_TEST_BINARY')]);
    try {
        const extension = Main.extensionManager.lookup(UUID).stateObj;
        let cardClicks = 0;
        extension._section.connect('clicked', () => cardClicks++);
        const trays = () => Object.values(Main.panel.statusArea).filter(item => item._indicator?.id === 'open-meteo');
        const window = () => global.get_window_actors().map(actor => actor.meta_window)
            .find(win => win.get_gtk_application_id() === BUS);
        const pointer = global.stage.context.get_backend().get_default_seat()
            .create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
        const moveTo = async (x, y) => until(() => {
            pointer.notify_absolute_motion(GLib.get_monotonic_time(), x, y);
            const [px, py] = global.get_pointer();
            return Math.abs(px - x) <= 1 && Math.abs(py - y) <= 1;
        }, 'virtual pointer reaches its target');
        const click = async () => {
            pointer.notify_button(GLib.get_monotonic_time(), Clutter.BUTTON_PRIMARY, Clutter.ButtonState.PRESSED);
            await Scripting.sleep(40);
            pointer.notify_button(GLib.get_monotonic_time(), Clutter.BUTTON_PRIMARY, Clutter.ButtonState.RELEASED);
        };
        const openCalendar = async () => {
            const dateMenu = Main.panel.statusArea.dateMenu;
            const [x, y] = dateMenu.get_transformed_position();
            const [width, height] = dateMenu.get_transformed_size();
            await moveTo(x + width / 2, y + height / 2);
            await click();
            await until(() => dateMenu.menu.isOpen && extension._section.mapped, 'clock click opens the forecast card');
            await Scripting.sleep(300);
        };
        await until(() => extension._box.visible && extension._temperature.text.includes('21°C'), 'real clock weather');
        await until(() => trays().length === 0, 'clock removes the real AppIndicator item');
        assert(extension._forecastGrid.get_n_children() === 15, 'Five real hourly forecasts');
        await until(() => window(), 'forecast window');
        Main.overview.hide();
        await until(() => !Main.overview.visible, 'overview closes');
        Main.activateWindow(window());
        await until(() => window().has_focus(), 'initial forecast window receives focus');
        window().minimize();
        await until(() => window().minimized && global.display.focus_window !== window(),
            'minimize finishes before opening the calendar');
        await Scripting.sleep(200);
        await openCalendar();
        const output = GLib.getenv('OPEN_METEO_GNOME_CLOCK_APP_SCREENSHOT');
        if (output) {
            const stream = Gio.File.new_for_path(output).replace(null, false, Gio.FileCreateFlags.NONE, null);
            await new Shell.Screenshot().screenshot_area(0, 0, global.stage.width, global.stage.height, stream);
            stream.close(null);
        }
        const clickForecast = async () => {
            const [x, y] = extension._section.get_transformed_position();
            await moveTo(x + 40, y + 20);
            let hit = global.stage.get_actor_at_pos(Clutter.PickMode.REACTIVE, x + 40, y + 20);
            const picked = String(hit);
            while (hit && hit !== extension._section) hit = hit.get_parent();
            assert(hit === extension._section,
                `Forecast card hit target: ${picked}; reactive=${extension._section.reactive}`);
            await click();
            try {
                await until(() => window() && !window().minimized && global.display.focus_window === window(),
                    'forecast card restores and focuses the real Wayland window');
            } catch (error) {
                print(`CLOCK_ACTIVATION_DIAGNOSTIC ${JSON.stringify({cardClicks,
                    minimized: window()?.minimized, focus: global.display.focus_window?.get_title(),
                    pointer: global.get_pointer(), target: [x + 40, y + 20],
                    menuOpen: Main.panel.statusArea.dateMenu.menu.isOpen})}`);
                throw error;
            }
        };
        await clickForecast();
        window().delete(global.get_current_time());
        await until(() => !window(), 'closing to the clock hides the forecast window');
        await openCalendar();
        await clickForecast();
        Main.extensionManager.disableExtension(UUID);
        await until(() => extension._box === null && trays().length === 1 && trays()[0]._label?.text === '21°',
            'disabling clock restores one working native tray');
        Main.extensionManager.enableExtension(UUID);
        await until(() => extension._box?.visible && trays().length === 0, 'reenabling clock removes the fallback');
        await call('/io/github/open_meteo/OpenMeteo', 'org.gtk.Actions', 'Activate',
            new GLib.Variant('(sava{sv})', ['quit', [], {}]));
        await until(() => !extension._box.visible, 'quit clears clock weather');
        print('GNOME_CLOCK_APP_SUITE_PASSED: real hourly forecast, one clock indicator, Wayland activation, tray handover and cleanup');
    } finally {
        app.force_exit();
    }
}
