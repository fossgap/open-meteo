// Verify the native icon + label as one button in a real, isolated GNOME Shell.
import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as Scripting from 'resource:///org/gnome/shell/ui/scripting.js';

export const METRICS = {};
const WATCHER = 'org.kde.StatusNotifierWatcher';
const INTERFACE = 'org.kde.StatusNotifierItem';
const UUID = 'appindicatorsupport@rgcjonas.gmail.com';
const ID = 'open-meteo-indicator-test';

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
    await until(() => Main.extensionManager.lookup(UUID)?.stateObj, 'AppIndicator enabled');
    let temperature = '—';
    let activated = 0;
    const properties = {
        Id: ['s', ID], Category: ['s', 'ApplicationStatus'], Status: ['s', 'Active'],
        Title: ['s', 'Paris · 21°C · Partly cloudy'], WindowId: ['i', 0],
        IconName: ['s', 'weather-few-clouds-symbolic'], IconPixmap: ['a(iiay)', []],
        IconThemePath: ['s', ''], AttentionIconName: ['s', ''], AttentionIconPixmap: ['a(iiay)', []],
        OverlayIconName: ['s', ''], OverlayIconPixmap: ['a(iiay)', []],
        AttentionMovieName: ['s', ''], Menu: ['o', '/Menu'], ItemIsMenu: ['b', false],
        XAyatanaLabelGuide: ['s', '−100°'],
        ToolTip: ['(sa(iiay)ss)', ['', [], 'Paris · 21°C', 'Open forecast']],
    };
    const xml = `<node><interface name="${INTERFACE}">
        ${Object.entries(properties).map(([name, [type]]) => `<property name="${name}" type="${type}" access="read"/>`).join('')}
        <property name="XAyatanaLabel" type="s" access="read"/>
        <method name="Activate"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
        <method name="SecondaryActivate"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
        <signal name="XAyatanaNewLabel"><arg type="s"/><arg type="s"/></signal>
    </interface></node>`;
    const implementation = {Activate() { activated++; }, SecondaryActivate() { activated++; }};
    for (const [name, [, value]] of Object.entries(properties))
        Object.defineProperty(implementation, name, {get: () => value});
    Object.defineProperty(implementation, 'XAyatanaLabel', {get: () => temperature});
    const service = Gio.DBusExportedObject.wrapJSObject(xml, implementation);
    service.export(Gio.DBus.session, '/StatusNotifierItem');
    const menuXml = `<node><interface name="com.canonical.dbusmenu">
        <property name="Version" type="u" access="read"/>
        <property name="TextDirection" type="s" access="read"/>
        <property name="Status" type="s" access="read"/>
        <property name="IconThemePath" type="as" access="read"/>
        <method name="GetLayout"><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="as" direction="in"/><arg type="u" direction="out"/><arg type="(ia{sv}av)" direction="out"/></method>
        <method name="GetGroupProperties"><arg type="ai" direction="in"/><arg type="as" direction="in"/><arg type="a(ia{sv})" direction="out"/></method>
        <method name="Event"><arg type="i" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="in"/><arg type="u" direction="in"/></method>
        <method name="AboutToShow"><arg type="i" direction="in"/><arg type="b" direction="out"/></method>
    </interface></node>`;
    const menuProps = {label: new GLib.Variant('s', 'Open forecast'), enabled: new GLib.Variant('b', true), visible: new GLib.Variant('b', true)};
    const menu = Gio.DBusExportedObject.wrapJSObject(menuXml, {
        Version: 4, TextDirection: 'ltr', Status: 'normal', IconThemePath: [],
        GetLayout() { return [1, [0, {}, [new GLib.Variant('(ia{sv}av)', [1, menuProps, []])]]]; },
        GetGroupProperties() { return [[1, menuProps]]; },
        Event(_id, event) { if (event === 'clicked') activated++; },
        AboutToShow() { return false; },
    });
    menu.export(Gio.DBus.session, '/Menu');
    await new Promise((resolve, reject) => Gio.DBus.session.call(WATCHER, '/StatusNotifierWatcher',
        WATCHER, 'RegisterStatusNotifierItem', new GLib.Variant('(s)', ['/StatusNotifierItem']),
        null, Gio.DBusCallFlags.NONE, 3000, null, (connection, result) => {
            try { connection.call_finish(result); resolve(); } catch (error) { reject(error); }
        }));
    const buttons = () => Object.values(Main.panel.statusArea).filter(button => button._indicator?.id === ID);
    await until(() => buttons().length === 1, 'single registered indicator');
    // Match the app: registration is followed by the initial forecast update.
    temperature = '21°';
    service.emit_signal('XAyatanaNewLabel', new GLib.Variant('(ss)', [temperature, '−100°']));
    service.emit_property_changed('XAyatanaLabel', new GLib.Variant('s', temperature));
    try {
        await until(() => buttons().length === 1 && buttons()[0]._label?.text === '21°', 'one native icon-and-temperature button');
    } catch (error) {
        print(`INDICATOR_DIAGNOSTIC ${JSON.stringify(Object.entries(Main.panel.statusArea).map(([key, value]) => ({key,
            id: value._indicator?.id, label: value._indicator?.label, text: value._label?.text,
            ready: value._indicator?.isReady, owner: value._indicator?.hasNameOwner, menu: value._indicator?.menuPath})))}`);
        print(`STARTUP ${Main.layoutManager._startingUp}`);
        throw error;
    }
    const button = buttons()[0];
    Main.overview.hide();
    await Scripting.sleep(500);
    assert(button.contains(button._icon) && button.contains(button._label), 'Both elements must belong to the same clickable item');
    assert(button._box.get_n_children() === 2, 'One horizontal group: weather symbol and label');
    const [iconX, iconY] = button._icon.get_transformed_position();
    const [iconWidth, iconHeight] = button._icon.get_transformed_size();
    const [textX, textY] = button._label.get_transformed_position();
    const [textWidth, textHeight] = button._label.get_transformed_size();
    assert(iconX + iconWidth <= textX, 'Temperature must follow the icon on one line');
    assert(Math.abs(iconY + iconHeight / 2 - textY - textHeight / 2) <= 2, 'Icon and temperature must share a vertical center');
    assert(iconWidth >= 16 && textHeight >= 14, 'Keep native, readable symbol and text sizes');
    // Stage hit-testing must resolve both halves and their gap to this button.
    for (const x of [iconX + iconWidth / 2, (iconX + iconWidth + textX) / 2, textX + textWidth / 2]) {
        let hit = global.stage.get_actor_at_pos(Clutter.PickMode.REACTIVE, x, iconY + iconHeight / 2);
        while (hit && hit !== button)
            hit = hit.get_parent();
        assert(hit === button, 'Every point must use the same click target');
    }
    const output = GLib.getenv('OPEN_METEO_GNOME_INDICATOR_SCREENSHOT');
    if (output) {
        const stream = Gio.File.new_for_path(output).replace(null, false, Gio.FileCreateFlags.NONE, null);
        await new Shell.Screenshot().screenshot_area(0, 0, 1280, 40, stream);
        stream.close(null);
    }
    temperature = '70°';
    service.emit_signal('XAyatanaNewLabel', new GLib.Variant('(ss)', [temperature, '−100°']));
    service.emit_property_changed('XAyatanaLabel', new GLib.Variant('s', temperature));
    await until(() => button._label.text === '70°', 'native label update');
    assert(buttons().length === 1 && buttons()[0] === button, 'Updates must keep the same single item');
    await button._indicator.open(iconX, iconY, 0);
    await until(() => activated === 1, 'shared activation');
    Main.extensionManager.disableExtension(UUID);
    await until(() => buttons().length === 0, 'indicator cleanup');
    service.unexport();
    menu.unexport();
    print('GNOME_INDICATOR_SUITE_PASSED: one native icon + temperature button, geometry, hit targets, updates and activation');
}
