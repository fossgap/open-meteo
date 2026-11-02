// Run only in the isolated Shell started by scripts/check-gnome.sh.
import Gio from 'gi://Gio';
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

export async function run() {
    await until(() => Main.extensionManager.lookup(UUID)?.stateObj?._box, 'extension enabled');
    const extension = Main.extensionManager.lookup(UUID).stateObj;
    const clock = Main.panel.statusArea.dateMenu._clockDisplay;
    const parent = clock.get_parent();
    const originalCount = parent.get_n_children();
    let attached = 0;
    let shown = 0;
    let refreshed = 0;
    const data = {version: 1, available: true, location: 'Paris', temperature: '11°C',
        condition: 'Overcast', icon: 'weather-overcast-symbolic', stale: false,
        status: 'Updated · test fixture', open: 'Open forecast', refresh: 'Refresh', choose: 'Choose a location'};
    const service = Gio.DBusExportedObject.wrapJSObject(XML, {
        Attach() { attached++; return JSON.stringify(data); },
        Detach() { attached--; }, Show() { shown++; }, Refresh() { refreshed++; },
    });
    service.export(Gio.DBus.session, PATH);
    let owner = Gio.bus_own_name_on_connection(Gio.DBus.session, BUS, Gio.BusNameOwnerFlags.NONE, null, null);
    await until(() => extension._temperature.text === '11°C', 'clock temperature');
    assert(attached === 1, 'Extension must attach once');
    assert(parent.get_children().indexOf(extension._box) + 1 === parent.get_children().indexOf(clock), 'Weather must be immediately beside the original clock');
    assert(clock.text.length > 0, 'Native clock text must remain visible');
    Main.overview.hide();
    await Scripting.sleep(300);
    const output = GLib.getenv('OPEN_METEO_GNOME_SCREENSHOT');
    if (output) {
        const screenshot = new Shell.Screenshot();
        const stream = Gio.File.new_for_path(output).replace(null, false, Gio.FileCreateFlags.NONE, null);
        await screenshot.screenshot_area(0, 0, 1280, 40, stream);
        stream.close(null);
    }
    Main.panel.statusArea.dateMenu.menu.open();
    assert(extension._section.actor.visible, 'Forecast actions must be in the clock menu');
    extension._open.activate(null);
    extension._refresh.activate(null);
    await until(() => shown === 1 && refreshed === 1, 'clock actions');
    data.temperature = '52°F';
    data.stale = true;
    service.emit_signal('Changed', new GLib.Variant('(s)', [JSON.stringify(data)]));
    await until(() => extension._temperature.text === '~52°F', 'units and saved-weather marker');
    Gio.bus_unown_name(owner);
    await until(() => !extension._box.visible, 'app exits');
    owner = Gio.bus_own_name_on_connection(Gio.DBus.session, BUS, Gio.BusNameOwnerFlags.NONE, null, null);
    await until(() => extension._box.visible, 'app restarts');
    Main.extensionManager.disableExtension(UUID);
    await until(() => extension._box === null, 'extension disabled');
    assert(parent.get_n_children() === originalCount - 1, 'Disable must remove the injected actor');
    assert(clock.get_parent() === parent && clock.text.length > 0, 'Disable must preserve the clock');
    // The first owner loss simulates an app restart and forgets its lease.
    await until(() => attached === 1, 'Detach on disable');
    Main.extensionManager.enableExtension(UUID);
    await until(() => extension._box?.visible, 'extension re-enabled');
    assert(parent.get_n_children() === originalCount, 'Re-enable must not duplicate weather');
    Main.extensionManager.disableExtension(UUID);
    await until(() => extension._box === null, 'final cleanup');
    Gio.bus_unown_name(owner);
    service.unexport();
    print('GNOME_CLOCK_SUITE_PASSED: placement, clock preservation, updates, stale data, actions, app restart, disable and re-enable');
}
