#!/usr/bin/env python3
"""One real Rust StatusNotifier item with a native Ayatana icon + label."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from gi.repository import Gio, GLib
from ui_smoke import BINARY, ROOT, WATCHER, PORTAL, click, find, wait_for, xdo


def run():
    output = Path(os.environ.get("OPEN_METEO_TEST_OUTPUT_DIR", ROOT / "target/ci/tray-label"))
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="open-meteo-label-") as directory:
        private = Path(directory)
        env = os.environ | {
            "XDG_CONFIG_HOME": str(private / "config"), "XDG_CACHE_HOME": str(private / "cache"),
            "XDG_DATA_HOME": str(private / "data"), "XDG_CURRENT_DESKTOP": "ubuntu:GNOME",
            "GDK_BACKEND": "x11", "GSK_RENDERER": "cairo", "GIO_USE_VFS": "local", "GTK_A11Y": "atspi",
            "HTTPS_PROXY": "http://127.0.0.1:9", "HTTP_PROXY": "http://127.0.0.1:9",
            "LC_ALL": "C.UTF-8", "LANGUAGE": "en",
        }
        env.pop("GTK_THEME", None)
        helper = subprocess.Popen([sys.executable, str(ROOT / "tests/ui_smoke.py"), "--services"], env=env)
        app = None
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)

        def call(service, path, interface, method, args=None):
            return bus.call_sync(service, path, interface, method, args, None, Gio.DBusCallFlags.NONE, 3000, None).unpack()

        def get(service, path, interface, name):
            return call(service, path, "org.freedesktop.DBus.Properties", "Get", GLib.Variant("(ss)", (interface, name)))[0]

        try:
            wait_for(lambda: call("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
                "NameHasOwner", GLib.Variant("(s)", (PORTAL,)))[0], "mock desktop")
            with (output / "tray-label.log").open("w") as log:
                app = subprocess.Popen([str(BINARY), "--demo"], env=env, stdout=log, stderr=log)
                wait_for(lambda: find("21°"), "forecast")
                registered = lambda: get(WATCHER, "/StatusNotifierWatcher", WATCHER, "RegisteredStatusNotifierItems")
                wait_for(lambda: len(registered()) == 1, "one native labeled item")
                item = registered()[0]
                service, path = item.split("/", 1)
                path = "/" + path
                interface = "org.kde.StatusNotifierItem"
                prop = lambda name: get(service, path, interface, name)
                wait_for(lambda: prop("XAyatanaLabel") == "21°", "native temperature label")
                icon = prop("IconName")
                assert icon.startswith("weather-") and prop("IconPixmap") == [], "Use a full-size themed icon without a second bitmap temperature"
                assert prop("XAyatanaLabelGuide") and not prop("ItemIsMenu")
                labels, changed = [], []
                subscriptions = [
                    bus.signal_subscribe(service, interface, "XAyatanaNewLabel", path, None,
                        Gio.DBusSignalFlags.NONE, lambda _bus, _sender, _path, _interface, _signal, params, _data: labels.append(params.unpack()), None),
                    bus.signal_subscribe(service, "org.freedesktop.DBus.Properties", "PropertiesChanged", path, None,
                        Gio.DBusSignalFlags.NONE, lambda _bus, _sender, _path, _interface, _signal, params, _data: changed.append(params.unpack()), None),
                ]
                click("°F")
                wait_for(lambda: prop("XAyatanaLabel") == "70°" and any(label[0] == "70°" for label in labels), "native label update signal")
                wait_for(lambda: any(change[1].get("XAyatanaLabel") == "70°" for change in changed), "label property update")
                assert prop("IconName") == icon and registered() == [item]
                window = xdo("search", "--name", "^Open Meteo$").splitlines()[0]
                click("Close")
                wait_for(lambda: "IsUnMapped" in subprocess.check_output(["xwininfo", "-id", window], text=True), "hide forecast")
                call(service, path, interface, "Activate", GLib.Variant("(ii)", (0, 0)))
                wait_for(lambda: "IsViewable" in subprocess.check_output(["xwininfo", "-id", window], text=True), "shared activation")
                for subscription in subscriptions:
                    bus.signal_unsubscribe(subscription)
                xdo("windowfocus", window)
                xdo("key", "ctrl+q")
                app.wait(timeout=5)
                assert app.returncode == 0
        finally:
            if app and app.poll() is None:
                app.terminate()
                app.wait(timeout=5)
            helper.terminate()
            helper.wait(timeout=5)
    print("TRAY_LABEL_SUITE_PASSED: one native icon + temperature item, update signals, and activation")


if __name__ == "__main__":
    run()
