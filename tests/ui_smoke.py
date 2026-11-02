#!/usr/bin/env python3
"""Native UI integration checks. Run inside dbus-run-session and xvfb-run.

Uses an isolated settings portal and StatusNotifier host, real GTK accessibility,
and a refused localhost proxy for deterministic offline tests. No external API.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

import gi
gi.require_version("Gio", "2.0")
gi.require_version("Atspi", "2.0")
from gi.repository import Atspi, Gio, GLib

ROOT = Path(__file__).resolve().parents[1]
BINARY = Path(os.environ.get("OPEN_METEO_TEST_BINARY", ROOT / "target/debug/open-meteo")).resolve()
PORTAL = "org.freedesktop.portal.Desktop"
WATCHER = "org.kde.StatusNotifierWatcher"
SETTINGS = "org.freedesktop.portal.Settings"


def services():
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    scheme = [2]
    items = []
    xml = """<node>
    <interface name="org.kde.StatusNotifierWatcher">
      <method name="RegisterStatusNotifierItem"><arg type="s" direction="in"/></method>
      <property name="ProtocolVersion" type="i" access="read"/>
      <property name="IsStatusNotifierHostRegistered" type="b" access="read"/>
      <property name="RegisteredStatusNotifierItems" type="as" access="read"/>
      <signal name="StatusNotifierItemRegistered"><arg type="s"/></signal>
    </interface>
    <interface name="org.freedesktop.portal.Settings">
      <method name="Read"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method>
      <method name="ReadAll"><arg type="as" direction="in"/><arg type="a{sa{sv}}" direction="out"/></method>
      <method name="SetColorScheme"><arg type="u" direction="in"/></method>
      <method name="RemoveTray"/>
      <property name="version" type="u" access="read"/>
      <signal name="SettingChanged"><arg type="s"/><arg type="s"/><arg type="v"/></signal>
    </interface></node>"""
    watcher_owner = Gio.bus_own_name_on_connection(bus, WATCHER, Gio.BusNameOwnerFlags.NONE, None, None)

    def method(connection, sender, path, interface, name, params, invocation):
        if name == "RegisterStatusNotifierItem":
            item = params.unpack()[0]
            item = sender + item if item.startswith("/") else item + "/StatusNotifierItem"
            items.append(item)
            invocation.return_value(GLib.Variant("()", ()))
            connection.emit_signal(None, path, WATCHER, "StatusNotifierItemRegistered", GLib.Variant("(s)", (item,)))
        elif name == "Read":
            namespace, key = params.unpack()
            if namespace == "org.freedesktop.appearance" and key == "color-scheme":
                # The portal's deprecated Read method returns a nested variant.
                invocation.return_value(GLib.Variant("(v)", (GLib.Variant("v", GLib.Variant("u", scheme[0])),)))
            else:
                invocation.return_dbus_error("org.freedesktop.portal.Error.NotFound", "Setting not found")
        elif name == "ReadAll":
            invocation.return_value(GLib.Variant("(a{sa{sv}})", ({"org.freedesktop.appearance": {"color-scheme": GLib.Variant("u", scheme[0])}},)))
        elif name == "SetColorScheme":
            scheme[0] = params.unpack()[0]
            connection.emit_signal(None, path, SETTINGS, "SettingChanged", GLib.Variant("(ssv)", ("org.freedesktop.appearance", "color-scheme", GLib.Variant("u", scheme[0]))))
            invocation.return_value(GLib.Variant("()", ()))
        elif name == "RemoveTray":
            Gio.bus_unown_name(watcher_owner)
            invocation.return_value(GLib.Variant("()", ()))

    def prop(_connection, _sender, _path, _interface, name):
        return {"version": GLib.Variant("u", 1), "ProtocolVersion": GLib.Variant("i", 0),
                "IsStatusNotifierHostRegistered": GLib.Variant("b", True),
                "RegisteredStatusNotifierItems": GLib.Variant("as", items)}.get(name)

    info = Gio.DBusNodeInfo.new_for_xml(xml)
    bus.register_object("/StatusNotifierWatcher", info.interfaces[0], method, prop, None)
    bus.register_object("/org/freedesktop/portal/desktop", info.interfaces[1], method, prop, None)
    Gio.bus_own_name_on_connection(bus, PORTAL, Gio.BusNameOwnerFlags.NONE, None, None)
    GLib.MainLoop().run()


def wait_for(predicate, description, timeout=12):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        while GLib.MainContext.default().pending():
            GLib.MainContext.default().iteration(False)
        result = predicate()
        if result:
            return result
        time.sleep(0.1)
    raise AssertionError(f"Timed out: {description}")


def nodes(root=None):
    root = root or Atspi.get_desktop(0)
    yield root
    for i in range(root.get_child_count()):
        try:
            yield from nodes(root.get_child_at_index(i))
        except GLib.Error:
            continue


def find(name):
    return next((n for n in nodes() if n.get_name() == name), None)


def text_present(text):
    return any(text in (n.get_name() or "") for n in nodes())


def click(name):
    node = wait_for(lambda: next((n for n in nodes() if n.get_name() == name and n.get_n_actions() > 0), None) or find(name), f"control {name}")
    if node.get_n_actions() == 0:
        rectangle = node.get_extents(Atspi.CoordType.SCREEN)
        xdo("mousemove", rectangle.x + rectangle.width // 2, rectangle.y + rectangle.height // 2)
        xdo("click", 1)
        return
    assert node.do_action(0), f"Could not activate {name}"


def xdo(*args):
    return subprocess.check_output(["xdotool", *map(str, args)], text=True).strip()


def capture(name, window, output):
    xdo("windowmove", window, 0, 0)
    xdo("mousemove", 0, 0)
    time.sleep(0.4)
    geometry = dict(line.split("=", 1) for line in xdo("getwindowgeometry", "--shell", window).splitlines())
    crop = f'{geometry["WIDTH"]}x{geometry["HEIGHT"]}+{geometry["X"]}+{geometry["Y"]}'
    subprocess.run(["import", "-window", "root", "-crop", crop, str(output / f"{name}.png")], check=True)


def check_weather_icon(screenshot):
    # The light preview has a blue symbolic weather icon in this area. A missing
    # icon silently renders a gray placeholder even when GTK logs no warning.
    coverage = subprocess.check_output([
        "convert", str(screenshot), "-crop", "150x170+780+160", "+repage",
        "-colorspace", "sRGB", "-fx", "(b-r > 0.25) ? 1 : 0",
        "-format", "%[fx:mean]", "info:",
    ], text=True, env=os.environ | {"LC_ALL": "C"})
    assert float(coverage) > 0.015, "Weather icon did not render (possible missing icon theme or SVG resources)"


def run_checks():
    output = Path(os.environ.get("OPEN_METEO_TEST_OUTPUT_DIR", ROOT / ".impeccable/review")).resolve()
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="open-meteo-ui-") as directory:
        temp = Path(directory)
        env = os.environ.copy()
        env.update({"XDG_CONFIG_HOME": str(temp / "config"), "XDG_CACHE_HOME": str(temp / "cache"),
                    "GDK_BACKEND": "x11", "GSK_RENDERER": "cairo", "GIO_USE_VFS": "local", "GTK_A11Y": "atspi",
                    "HTTPS_PROXY": "http://127.0.0.1:9", "HTTP_PROXY": "http://127.0.0.1:9",
                    "ALL_PROXY": "http://127.0.0.1:9", "NO_PROXY": "", "LC_ALL": "C.UTF-8", "LANGUAGE": "en"})
        # GTK_THEME is a debugging override that disables libadwaita's styling.
        env.pop("GTK_THEME", None)
        helper = subprocess.Popen([sys.executable, __file__, "--services"], env=env)
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)

        def call(service, path, interface, method, args=None):
            return bus.call_sync(service, path, interface, method, args, None, Gio.DBusCallFlags.NONE, 3000, None)

        def has_name(name):
            return call("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "NameHasOwner", GLib.Variant("(s)", (name,))).unpack()[0]

        def setting(method, args=None):
            return call(PORTAL, "/org/freedesktop/portal/desktop", SETTINGS, method, args)

        app = None
        log = (output / "ui-smoke.log").open("w")
        try:
            wait_for(lambda: has_name(PORTAL), "test settings service")
            app = subprocess.Popen([str(BINARY), "--demo"], env=env, stdout=log, stderr=log)
            wait_for(lambda: text_present("Sample weather"), "sample forecast")
            window = xdo("search", "--name", "^Open Meteo$").splitlines()[0]
            xdo("windowsize", window, 960, 960)
            xdo("windowfocus", window)
            capture("desktop-light", window, output)
            check_weather_icon(output / "desktop-light.png")
            click("°F")
            wait_for(lambda: find("70°"), "Fahrenheit temperature")
            click("°C")
            wait_for(lambda: find("21°"), "Celsius temperature")
            # Expand the daily forecast through the native accessible action.
            click("Today forecast")
            wait_for(lambda: text_present("Maximum wind"), "expanded daily details")
            capture("day-details", window, output)
            click("Today forecast")
            xdo("key", "--window", window, "ctrl+l")
            wait_for(lambda: find("Find a location"), "location popover")
            xdo("type", "--window", window, "Paris")
            wait_for(lambda: find("Paris, Île-de-France, France"), "search results")
            capture("search", window, output)
            xdo("key", "--window", window, "ctrl+a")
            xdo("type", "--window", window, "Atlantis")
            wait_for(lambda: text_present("Preview uses sample weather"), "empty preview search")
            xdo("key", "--window", window, "Escape")
            xdo("windowsize", window, 390, 960)
            capture("narrow-light", window, output)
            xdo("mousemove", "--window", window, 340, 850)
            xdo("click", "--repeat", 8, "--delay", 70, 5)
            capture("narrow-days", window, output)
            xdo("windowsize", window, 960, 960)
            xdo("mousemove", "--window", window, 300, 200)
            xdo("click", "--repeat", 12, "--delay", 30, 4)
            setting("SetColorScheme", GLib.Variant("(u)", (1,)))
            time.sleep(1)
            capture("desktop-dark", window, output)
            # The real Rust tray must register and expose weather via D-Bus.
            def registered():
                return call(WATCHER, "/StatusNotifierWatcher", "org.freedesktop.DBus.Properties", "Get", GLib.Variant("(ss)", (WATCHER, "RegisteredStatusNotifierItems"))).unpack()[0]
            item = wait_for(registered, "tray registration")[-1]
            service, path = item.split("/", 1)
            path = "/" + path
            title = call(service, path, "org.freedesktop.DBus.Properties", "Get", GLib.Variant("(ss)", ("org.kde.StatusNotifierItem", "Title"))).unpack()[0]
            assert "Paris" in title and "21°" in title and "sample" in title, title
            def tray_property(name):
                return call(service, path, "org.freedesktop.DBus.Properties", "Get", GLib.Variant("(ss)", ("org.kde.StatusNotifierItem", name))).unpack()[0]
            assert tray_property("IconName") == "", "Host must render the temperature pixmap"
            pixmaps = tray_property("IconPixmap")
            assert len(pixmaps) == 4 and all(len(pixels) == width * height * 4 for width, height, pixels in pixmaps), "Missing temperature pixmaps"
            click("°F")
            wait_for(lambda: tray_property("IconPixmap") != pixmaps, "temperature icon changes with units")
            click("°C")
            menu_path = call(service, path, "org.freedesktop.DBus.Properties", "Get", GLib.Variant("(ss)", ("org.kde.StatusNotifierItem", "Menu"))).unpack()[0]
            layout = call(service, menu_path, "com.canonical.dbusmenu", "GetLayout", GLib.Variant("(iias)", (0, -1, []))).unpack()
            assert "Next 7 days" in str(layout) and "Next hours" in str(layout), layout
            click("Close")
            wait_for(lambda: "IsUnMapped" in subprocess.check_output(["xwininfo", "-id", window], text=True), "close to tray")
            call(service, path, "org.kde.StatusNotifierItem", "Activate", GLib.Variant("(ii)", (0, 0)))
            wait_for(lambda: "IsViewable" in subprocess.check_output(["xwininfo", "-id", window], text=True), "restore from tray")
            click("Close")
            setting("RemoveTray")
            wait_for(lambda: "IsViewable" in subprocess.check_output(["xwininfo", "-id", window], text=True), "tray loss restores the hidden window")
            # The GNOME clock keeps the app reachable without an SNI host.
            bridge_service = "io.github.open_meteo.OpenMeteo.Demo"
            bridge_path = "/io/github/open_meteo/OpenMeteo/Weather"
            bridge_interface = "io.github.open_meteo.OpenMeteo.Weather1"
            def bridge(method):
                return call(bridge_service, bridge_path, bridge_interface, method)
            snapshot = json.loads(bridge("Attach").unpack()[0])
            assert snapshot["temperature"] == "21°C" and snapshot["location"] == "Paris"
            click("Close")
            wait_for(lambda: "IsUnMapped" in subprocess.check_output(["xwininfo", "-id", window], text=True), "close to GNOME clock")
            bridge("Show")
            wait_for(lambda: "IsViewable" in subprocess.check_output(["xwininfo", "-id", window], text=True), "GNOME forecast action")
            click("Close")
            bridge("Detach")
            wait_for(lambda: "IsViewable" in subprocess.check_output(["xwininfo", "-id", window], text=True), "extension disable restores hidden window")
            # A vanished consumer must release its lease without calling Detach.
            peer = Gio.DBusConnection.new_for_address_sync(os.environ["DBUS_SESSION_BUS_ADDRESS"],
                Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION, None, None)
            peer.call_sync(bridge_service, bridge_path, bridge_interface, "Attach", None, None, Gio.DBusCallFlags.NONE, 3000, None)
            click("Close")
            wait_for(lambda: "IsUnMapped" in subprocess.check_output(["xwininfo", "-id", window], text=True), "close with independent clock peer")
            peer.close_sync(None)
            wait_for(lambda: "IsViewable" in subprocess.check_output(["xwininfo", "-id", window], text=True), "clock crash restores hidden window")
            click("Close")
            app.wait(timeout=5)
            assert app.returncode == 0, app.returncode
            assert not (temp / "config/open-meteo/settings.json").exists(), "Preview must not persist settings"
            # A real first run must not silently choose a city.
            setting("SetColorScheme", GLib.Variant("(u)", (2,)))
            app = subprocess.Popen([str(BINARY)], env=env, stdout=log, stderr=log)
            wait_for(lambda: find("Your weather, at a glance"), "first-run welcome")
            window = xdo("search", "--name", "^Open Meteo$").splitlines()[0]
            capture("welcome", window, output)
            xdo("windowfocus", window)
            xdo("key", "ctrl+q")
            app.wait(timeout=5)
            # Load a saved forecast with no network; verify recovery and timestamps.
            location = {"name": "Paris", "latitude": 48.85341, "longitude": 2.3488, "country": "France", "admin1": "Île-de-France"}
            config = temp / "config/open-meteo"
            cache = temp / "cache/open-meteo"
            config.mkdir(parents=True, exist_ok=True)
            cache.mkdir(parents=True, exist_ok=True)
            (config / "settings.json").write_text(json.dumps({"location": location, "recent": [location], "units": "celsius", "keep_in_tray": True}))
            fixture = json.loads((ROOT / "tests/fixtures/forecast.json").read_text())
            (cache / "48.85341_2.34880.json").write_text(json.dumps({"location": location, "fetched_at": fixture["current"]["time"], "forecast": fixture}))
            app = subprocess.Popen([str(BINARY)], env=env, stdout=log, stderr=log)
            wait_for(lambda: text_present("Showing saved weather"), "offline cache fallback")
            assert text_present("saved hourly forecast has ended"), "Expired hours must not be labelled current"
            window = xdo("search", "--name", "^Open Meteo$").splitlines()[0]
            capture("offline", window, output)
            click("°F")
            wait_for(lambda: json.loads((config / "settings.json").read_text())["units"] == "fahrenheit", "saved Fahrenheit preference")
            xdo("windowfocus", window)
            xdo("key", "--window", window, "ctrl+l")
            xdo("type", "--window", window, "London")
            wait_for(lambda: find("Couldn’t connect. Check your internet connection and try again."), "search connection failure")
            xdo("key", "Escape")
            wait_for(lambda: not find("Find a location"), "search popover closes")
            click("Main menu")
            capture("main-menu", window, output)
            # GTK 4.24 can omit names on native GMenu items. Their semantic
            # menu/radio roles and order remain available to accessibility.
            language_menu = wait_for(lambda: find("Language (restart required)") or next(
                (n for n in nodes() if n.get_role_name() == "menu item"), None), "language submenu")
            assert language_menu.do_action(0)
            def german_option():
                named = find("Deutsch")
                radios = [n for n in nodes() if n.get_role_name() == "radio menu item"]
                return named or (radios[3] if len(radios) == 8 else None)
            german = wait_for(german_option, "German language option")
            capture("language-menu", window, output)
            assert german.do_action(0)
            wait_for(lambda: json.loads((config / "settings.json").read_text())["language"] == "de", "language selection saved")
            # GTK 4.8 can retain a popover keyboard grab without a window
            # manager. Invoke the same exported Quit action for this restart.
            call("io.github.open_meteo.OpenMeteo", "/io/github/open_meteo/OpenMeteo",
                 "org.gtk.Actions", "Activate", GLib.Variant("(sava{sv})", ("quit", [], {})))
            app.wait(timeout=5)
            # Real translated controls, scripts, RTL direction and narrow layouts.
            for language in ("fr", "de", "es", "zh", "hi", "ar"):
                messages = json.loads((ROOT / f"resources/locales/{language}.json").read_text())
                app = subprocess.Popen([str(BINARY), "--demo"], env=env | {"LANGUAGE": language}, stdout=log, stderr=log)
                wait_for(lambda: find(messages["Next 24 hours"]), f"{language} hourly heading")
                assert find(messages["Next 7 days"]), f"{language} daily heading"
                state = json.loads(bridge("GetSnapshot").unpack()[0])
                assert state["language"] == language and state["rtl"] == (language == "ar"), state
                assert state["temperature"] == ("٢١°C" if language == "ar" else "21°C"), state
                window = xdo("search", "--name", "^Open Meteo$").splitlines()[0]
                xdo("windowsize", window, 960, 960)
                capture(f"language-{language}", window, output)
                # Leave room for native titlebars and distro font metrics.
                compact_width = 420
                xdo("windowsize", window, compact_width, 960)
                capture(f"language-{language}-narrow", window, output)
                width = dict(line.split("=", 1) for line in xdo("getwindowgeometry", "--shell", window).splitlines())["WIDTH"]
                assert int(width) == compact_width, f"{language} expands narrow window to {width}px"
                current_hour = find(messages["Now"]).get_extents(Atspi.CoordType.SCREEN)
                assert 0 <= current_hour.x < compact_width, f"{language} hourly strip must start at the current hour"
                xdo("windowfocus", window)
                xdo("key", "ctrl+q")
                app.wait(timeout=5)
            # Saved language takes precedence over the desktop locale on restart.
            app = subprocess.Popen([str(BINARY)], env=env | {"LANGUAGE": "fr"}, stdout=log, stderr=log)
            wait_for(lambda: text_present("Gespeicherte Wetterdaten werden angezeigt."), "German saved language and translated offline error")
            xdo("windowfocus", xdo("search", "--name", "^Open Meteo$").splitlines()[0])
            xdo("key", "ctrl+q")
            app.wait(timeout=5)
            # The AppImage and native binary carry an identical installable extension.
            extension_data = temp / "extension-data"
            subprocess.run([str(BINARY), "--install-gnome-extension"], env=env | {"XDG_DATA_HOME": str(extension_data)}, check=True, stdout=log, stderr=log)
            for file in (ROOT / "resources/gnome-extension").iterdir():
                assert (extension_data / "gnome-shell/extensions/open-meteo-clock@fossgap.github.io" / file.name).read_bytes() == file.read_bytes()
            log.flush()
            log_text = (output / "ui-smoke.log").read_text()
            for failure in ["panicked at", "CRITICAL", "Failed to load icon", "Failed to load module:", "Unrecognized image file format"]:
                assert failure not in log_text, log_text
            print("UI checks passed: theme changes, units, search, narrow layout, tray registration/menu/restore/loss, first run, offline cache, persistence, and quit.")
        finally:
            if app and app.poll() is None:
                app.terminate()
                app.wait(timeout=5)
            helper.terminate()
            helper.wait(timeout=5)
            log.close()


if __name__ == "__main__":
    services() if "--services" in sys.argv else run_checks()
