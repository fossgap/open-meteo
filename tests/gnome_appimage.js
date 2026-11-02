// Run against a packaged app in the isolated GNOME session.
import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as Scripting from 'resource:///org/gnome/shell/ui/scripting.js';
export const METRICS = {};
function assert(value, message) { if (!value) throw new Error(message); }
async function until(predicate, message) {
    for (let i = 0; i < 200; i++) {
        if (predicate()) return;
        await Scripting.sleep(100);
    }
    throw new Error(`Timed out: ${message}`);
}
export async function run() {
    const launcher = new Gio.SubprocessLauncher({flags: Gio.SubprocessFlags.NONE});
    for (const [name, value] of Object.entries({XDG_CURRENT_DESKTOP: 'GNOME',
        APPIMAGE_EXTRACT_AND_RUN: '1', GDK_BACKEND: 'wayland', GSK_RENDERER: 'cairo', GIO_USE_VFS: 'local'}))
        launcher.setenv(name, value, true);
    const app = launcher.spawnv([GLib.getenv('OPEN_METEO_TEST_BINARY'), '--demo']);
    const buttons = () => Object.values(Main.panel.statusArea).filter(item => item._indicator?.id === 'open-meteo');
    await until(() => buttons().length === 1 && buttons()[0]._label?.text === '21°', 'real AppImage native label');
    const button = buttons()[0];
    let activations = 0;
    const originalOpen = button._indicator.open.bind(button._indicator);
    button._indicator.open = (...args) => { activations++; return originalOpen(...args); };
    Main.overview.hide();
    await Scripting.sleep(500);
    const [ix, iy] = button._icon.get_transformed_position();
    const [iw, ih] = button._icon.get_transformed_size();
    const [tx, ty] = button._label.get_transformed_position();
    const [tw, th] = button._label.get_transformed_size();
    assert(button.contains(button._label) && button.contains(button._icon), 'One native button');
    assert(ix + iw <= tx && Math.abs(iy + ih / 2 - ty - th / 2) <= 2, 'One horizontal line');
    assert(iw >= 16 && th >= 14, 'Native readable sizes');
    for (const x of [ix + iw / 2, (ix + iw + tx) / 2, tx + tw / 2]) {
        let hit = global.stage.get_actor_at_pos(Clutter.PickMode.REACTIVE, x, iy + ih / 2);
        while (hit && hit !== button) hit = hit.get_parent();
        assert(hit === button, 'Shared hit target across icon and temperature');
    }
    const window = global.get_window_actors().map(actor => actor.meta_window).find(win => win.get_title() === 'Open Meteo');
    assert(window, 'Real forecast window');
    window.minimize();
    await Scripting.sleep(200);
    // Deliver real pointer events: Wayland activation requires user input,
    // and GNOME's AppIndicator uses double-click to open the application.
    const pointer = global.stage.context.get_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
    await Scripting.sleep(100);
    pointer.notify_absolute_motion(GLib.get_monotonic_time(), ix + iw / 2, iy + ih / 2);
    await Scripting.sleep(100);
    for (let click = 0; click < 2; click++) {
        pointer.notify_button(GLib.get_monotonic_time(), Clutter.BUTTON_PRIMARY, Clutter.ButtonState.PRESSED);
        await Scripting.sleep(30);
        pointer.notify_button(GLib.get_monotonic_time(), Clutter.BUTTON_PRIMARY, Clutter.ButtonState.RELEASED);
        await Scripting.sleep(80);
    }
    try {
        await until(() => !window.minimized, 'Real tray activation restores the forecast');
    } catch (error) {
        print(`ACTIVATION_DIAGNOSTIC ${JSON.stringify({activations, tokenSupported: button._indicator._hasProvideXdgActivationToken,
            lastClick: button._lastClickTime, clickCount: button._clickCount, menuOpen: button.menu.isOpen,
            windowTitle: window.get_title(), minimized: window.minimized, pointer: global.get_pointer(), target: [ix + iw / 2, iy + ih / 2]})}`);
        throw error;
    }
    const stream = Gio.File.new_for_path(GLib.getenv('OPEN_METEO_GNOME_APPIMAGE_SCREENSHOT')).replace(null, false, Gio.FileCreateFlags.NONE, null);
    await new Shell.Screenshot().screenshot_area(0, 0, 1280, 40, stream);
    stream.close(null);
    await new Promise((resolve, reject) => Gio.DBus.session.call('io.github.open_meteo.OpenMeteo.Demo',
        '/io/github/open_meteo/OpenMeteo/Demo', 'org.gtk.Actions', 'Activate',
        new GLib.Variant('(sava{sv})', ['quit', [], {}]), null, Gio.DBusCallFlags.NONE, 3000, null,
        (connection, result) => { try { connection.call_finish(result); resolve(); } catch (error) { reject(error); } }));
    await until(() => buttons().length === 0, 'Quit removes the single item');
    print('GNOME_REAL_APPIMAGE_PASSED: one native weather + temperature button, readable geometry, shared click target, activation and quit');
}
