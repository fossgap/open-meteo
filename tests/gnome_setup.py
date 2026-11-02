#!/usr/bin/env python3
"""Exercise the real installer against a private GNOME D-Bus service/settings.

Run under dbus-run-session. No graphical session or host settings are touched.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

from gi.repository import Gio, GLib

ROOT = Path(__file__).resolve().parents[1]
BINARY = Path(os.environ.get("OPEN_METEO_TEST_BINARY", ROOT / "target/debug/open-meteo")).resolve()
UUID = "open-meteo-clock@fossgap.github.io"
INTERFACE = "org.gnome.Shell.Extensions"
SOURCE = ROOT / "resources/gnome-extension"
VERSION = json.loads((SOURCE / "metadata.json").read_text())["version"]
XML = f"""<node><interface name="{INTERFACE}">
<method name="GetExtensionInfo"><arg type="s" direction="in"/><arg type="a{{sv}}" direction="out"/></method>
<method name="EnableExtension"><arg type="s" direction="in"/><arg type="b" direction="out"/></method>
<property name="ShellVersion" type="s" access="read"/>
<property name="UserExtensionsEnabled" type="b" access="read"/>
</interface></node>"""


def pump():
    while GLib.MainContext.default().pending():
        GLib.MainContext.default().iteration(False)
    time.sleep(0.01)


def checks():
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    info = Gio.DBusNodeInfo.new_for_xml(XML)
    scenario = {}

    def method(_bus, _sender, _path, _iface, name, params, invocation):
        assert params.unpack() == (UUID,)
        if name == "GetExtensionInfo":
            data = {} if scenario["unknown"] else {
                "path": GLib.Variant("s", str(SOURCE)),
                "state": GLib.Variant("d", float(scenario["state"])),
                "version": GLib.Variant("d", float(scenario["version"])),
            }
            invocation.return_value(GLib.Variant("(a{sv})", (data,)))
        elif name == "EnableExtension":
            scenario["enable_calls"] += 1
            invocation.return_value(GLib.Variant("(b)", (not scenario["refuse"],)))
            if not scenario["refuse"]:
                # Shell acknowledges before the extension's async enable finishes.
                def finish():
                    scenario["state"] = 3 if scenario["broken"] else 1
                    return False
                GLib.timeout_add(80, finish)

    def prop(_bus, _sender, _path, _iface, name):
        return {"ShellVersion": GLib.Variant("s", scenario["shell_version"]),
                "UserExtensionsEnabled": GLib.Variant("b", not scenario["disabled"])}[name]

    registration = bus.register_object("/org/gnome/Shell", info.interfaces[0], method, prop, None)
    acquired = []
    owner = Gio.bus_own_name_on_connection(bus, "org.gnome.Shell", Gio.BusNameOwnerFlags.NONE,
                                         lambda *_: acquired.append(True), None)
    deadline = time.monotonic() + 5
    while not acquired and time.monotonic() < deadline:
        pump()
    assert acquired
    try:
        with tempfile.TemporaryDirectory(prefix="open-meteo-setup-") as directory:
            root = Path(directory)
            schema = root / "share/glib-2.0/schemas"
            schema.mkdir(parents=True)
            (schema / "org.gnome.shell.gschema.xml").write_text("""<schemalist>
<schema id="org.gnome.shell" path="/org/gnome/shell/">
<key name="enabled-extensions" type="as"><default>['other@example']</default></key>
<key name="disabled-extensions" type="as"><default>['disabled@example', 'open-meteo-clock@fossgap.github.io']</default></key>
</schema></schemalist>""")
            subprocess.run(["glib-compile-schemas", str(schema)], check=True)

            def run(name, expected, success=True, **options):
                scenario.clear()
                scenario.update(unknown=False, state=2, version=VERSION, shell_version="50.4",
                                disabled=False, refuse=False, broken=False, enable_calls=0)
                scenario.update(options)
                env = os.environ | {"XDG_CONFIG_HOME": str(root / name / "config"),
                    "XDG_DATA_HOME": str(root / name / "data"), "GSETTINGS_BACKEND": "keyfile",
                    "GSETTINGS_SCHEMA_DIR": str(schema), "LANGUAGE": "en", "LC_ALL": "C.UTF-8",
                    # AppImage's GTK hook selects its bundled schemas first;
                    # GLib still searches XDG_DATA_DIRS for GNOME's schema.
                    "XDG_DATA_DIRS": f"{root / 'share'}:/usr/local/share:/usr/share"}
                if not Path(env["XDG_CONFIG_HOME"]).exists():
                    for key, value in [("enabled-extensions", "['other@example']"),
                                       ("disabled-extensions", f"['disabled@example', '{UUID}']")]:
                        subprocess.run(["gsettings", "set", "org.gnome.shell", key, value], env=env, check=True)
                process = subprocess.Popen([str(BINARY), "--install-gnome-extension"], env=env,
                                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
                deadline = time.monotonic() + 10
                while process.poll() is None and time.monotonic() < deadline:
                    pump()
                if process.poll() is None:
                    process.kill()
                    raise AssertionError(f"Installer hung: {name}")
                stdout, stderr = process.communicate()
                assert (process.returncode == 0) == success, (name, stdout, stderr)
                assert expected in stdout + stderr, (name, stdout, stderr)
                for source in SOURCE.iterdir():
                    assert (Path(env["XDG_DATA_HOME"]) / "gnome-shell/extensions" / UUID / source.name).read_bytes() == source.read_bytes()
                return env

            run("enable", "GNOME clock integration is enabled.")
            assert scenario["state"] == 1 and scenario["enable_calls"] == 1
            run("already-active", "GNOME clock integration is enabled.", state=1)
            env = run("first-install", "It will enable automatically.", unknown=True)
            assert scenario["enable_calls"] == 0, "An unknown extension cannot be enabled by this API"

            def setting(key):
                return subprocess.check_output(["gsettings", "get", "org.gnome.shell", key], env=env, text=True)

            assert UUID in setting("enabled-extensions") and "other@example" in setting("enabled-extensions")
            assert UUID not in setting("disabled-extensions") and "disabled@example" in setting("disabled-extensions")
            run("first-install", "It will enable automatically.", unknown=True)
            assert setting("enabled-extensions").count(UUID) == 1, "Setup must be idempotent"
            run("update", "has been updated", version=VERSION - 1)
            assert scenario["enable_calls"] == 0, "Cached old code must not be reported as updated live"
            run("disabled", "GNOME extensions are switched off.", success=False, disabled=True)
            assert scenario["enable_calls"] == 0
            run("unsupported", "not supported", success=False, shell_version="99.0")
            run("refused", "GNOME did not enable", success=False, refuse=True)
            run("broken", "GNOME did not enable", success=False, broken=True)
            Gio.bus_unown_name(owner)
            owner = None
            for _ in range(20):
                pump()
            run("no-shell", "Run clock setup from a GNOME session")
    finally:
        if owner is not None:
            Gio.bus_unown_name(owner)
        bus.unregister_object(registration)
    print("GNOME setup checks passed: live enable, already active, first install, idempotency, updates, disabled extensions, unsupported versions, refused activation, broken extension, and no Shell.")


if __name__ == "__main__":
    checks()
