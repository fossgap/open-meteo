#!/usr/bin/env python3
"""Verify a real horizontal tray's geometry, not just exported icon pixels."""
import os
from pathlib import Path
import subprocess
import tempfile
import time
from types import SimpleNamespace

import gi
gi.require_version("Atspi", "2.0")
from gi.repository import Atspi, Gio, GLib
from ui_smoke import BINARY, ROOT, click, nodes, wait_for, xdo

CONFIG = '''<?xml version="1.0" encoding="UTF-8"?>
<channel name="xfce4-panel" version="1.0">
  <property name="configver" type="int" value="2"/>
  <property name="panels" type="array"><value type="int" value="1"/>
    <property name="panel-1" type="empty">
      <property name="position" type="string" value="p=6;x=0;y=0"/>
      <property name="position-locked" type="bool" value="true"/>
      <property name="length" type="uint" value="100"/>
      <property name="size" type="uint" value="32"/>
      <property name="plugin-ids" type="array"><value type="int" value="1"/></property>
    </property>
  </property>
  <property name="plugins" type="empty">
    <property name="plugin-1" type="string" value="systray">
      <property name="icon-size" type="int" value="24"/>
      <property name="single-row" type="bool" value="true"/>
    </property>
  </property>
</channel>
'''


def run():
    if os.environ.get("OPEN_METEO_TRAY_TEST_SESSION") != "1":
        raise RuntimeError("Use scripts/check-tray.sh for an isolated session.")
    output = Path(os.environ.get("OPEN_METEO_TEST_OUTPUT_DIR", ROOT / "target/ci/tray-panel"))
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="open-meteo-panel-") as directory:
        private = Path(directory)
        env = os.environ | {
            "XDG_CONFIG_HOME": str(private / "config"), "XDG_DATA_HOME": str(private / "data"),
            "XDG_CACHE_HOME": str(private / "cache"), "XDG_CURRENT_DESKTOP": "XFCE",
            "GTK_A11Y": "atspi", "GDK_BACKEND": "x11", "GSK_RENDERER": "cairo",
            "GIO_USE_VFS": "local", "LANGUAGE": "en", "LC_ALL": "C.UTF-8",
            "HTTPS_PROXY": "http://127.0.0.1:9", "HTTP_PROXY": "http://127.0.0.1:9",
        }
        env.pop("GTK_THEME", None)
        config = private / "config/xfce4/xfconf/xfce-perchannel-xml"
        config.mkdir(parents=True)
        (config / "xfce4-panel.xml").write_text(CONFIG)
        gtk_config = private / "config/gtk-3.0"
        gtk_config.mkdir()
        (gtk_config / "settings.ini").write_text("[Settings]\ngtk-enable-animations=false\n")
        # D-Bus-activated xfconfd needs the private directories too. Setting only
        # the panel subprocess environment would leave its settings unisolated.
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        bus.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
            "UpdateActivationEnvironment", GLib.Variant("(a{ss})", ({key: env[key] for key in
                ["XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "XDG_CURRENT_DESKTOP", "GIO_USE_VFS"]},)),
            None, Gio.DBusCallFlags.NONE, 3000, None)

        def tray_buttons():
            buttons = []
            for node in nodes():
                if node.get_role_name() not in {"button", "push button"}:
                    continue
                application = node.get_application()
                if application and application.get_name() == "xfce4-panel" and node.get_state_set().contains(Atspi.StateType.SHOWING):
                    rect = node.get_extents(Atspi.CoordType.SCREEN)
                    if rect.width >= 20 and rect.height >= 20:
                        buttons.append((node, rect))
            return sorted(buttons, key=lambda item: item[1].x)

        def capture(name):
            time.sleep(0.3)
            file = output / name
            subprocess.run(["import", "-window", "root", "-crop", "960x32+0+0", str(file)], check=True)
            return file

        def pixels(file, rect):
            return subprocess.check_output(["convert", str(file), "-crop",
                f"{rect.width}x{rect.height}+{rect.x}+{rect.y}", "-depth", "8", "rgba:-"])

        theme_name = os.environ.get("OPEN_METEO_PANEL_THEME", "Adwaita").replace(":", "-")
        with (output / f"tray-panel-{theme_name}.log").open("w") as log:
            for theme in [os.environ.get("OPEN_METEO_PANEL_THEME", "Adwaita")]:
                panel = subprocess.Popen(["xfce4-panel", "--disable-wm-check"],
                    env=env | {"GTK_THEME": theme}, stdout=log, stderr=log)
                app = None
                try:
                    wait_for(lambda: any(n.get_name() == "xfce4-panel" for n in nodes()), "real tray host")
                    # Prove the settings daemon is using the test panel configuration.
                    plugins = subprocess.check_output(["xfconf-query", "-c", "xfce4-panel", "-p", "/panels/panel-1/plugin-ids"], env=env, text=True)
                    assert plugins.strip().endswith("1"), plugins
                    app = subprocess.Popen([str(BINARY), "--demo"], env=env, stdout=log, stderr=log)
                    wait_for(lambda: any(n.get_name() == "21°" for n in nodes()), "forecast ready")
                    window = xdo("search", "--name", "^Open Meteo$").splitlines()[0]
                    xdo("windowmove", window, 0, 80)
                    xdo("mousemove", 900, 700)
                    def visible_item():
                        # Xvfb has no window manager to keep windows clear of
                        # dock panels while GTK finishes its initial layout.
                        xdo("windowmove", window, 0, 80)
                        return len(tray_buttons()) == 1
                    try:
                        wait_for(visible_item, "one combined weather tray button")
                    except AssertionError:
                        capture("tray-failure.png")
                        for node in nodes():
                            if node.get_role_name() in {"button", "push button"}:
                                application = node.get_application()
                                rect = node.get_extents(Atspi.CoordType.SCREEN)
                                print("BUTTON", application.get_name() if application else None,
                                    node.get_name(), rect.x, rect.y, rect.width, rect.height,
                                    node.get_state_set().contains(Atspi.StateType.SHOWING), flush=True)
                        raise
                    _, rect = tray_buttons()[0]
                    # The host must expose one clickable widget for the complete
                    # weather + temperature display, including the space between.
                    weather_width = int(rect.width * 0.38)
                    left = SimpleNamespace(x=rect.x, y=rect.y, width=weather_width, height=rect.height)
                    right = SimpleNamespace(x=rect.x + weather_width, y=rect.y,
                        width=rect.width - weather_width, height=rect.height)
                    suffix = "dark" if theme.endswith(":dark") else "light"
                    before = capture(f"tray-{suffix}-celsius.png")
                    click("°F")
                    wait_for(lambda: any(n.get_name() == "70°" for n in nodes()), "Fahrenheit update")
                    after = capture(f"tray-{suffix}-fahrenheit.png")
                    assert len(tray_buttons()) == 1, "Unit changes must not create a second tray button"
                    assert pixels(before, left) == pixels(after, left), "The left element must remain the weather symbol"
                    assert pixels(before, right) != pixels(after, right), "The right element must be the changing temperature"
                    # Hover either half: the entire same button is highlighted.
                    xdo("mousemove", rect.x + rect.width // 4, rect.y + rect.height // 2)
                    weather_hover = capture(f"tray-{suffix}-hover-weather.png")
                    xdo("mousemove", rect.x + 3 * rect.width // 4, rect.y + rect.height // 2)
                    temperature_hover = capture(f"tray-{suffix}-hover-temperature.png")
                    assert pixels(weather_hover, rect) == pixels(temperature_hover, rect), "Both halves must share the same hover area"
                    for fraction in [0.2, 0.42, 0.75]:
                        click("Close")
                        wait_for(lambda: "IsUnMapped" in subprocess.check_output(["xwininfo", "-id", window], text=True), "hide forecast")
                        xdo("mousemove", rect.x + int(rect.width * fraction), rect.y + rect.height // 2)
                        xdo("click", 1)
                        wait_for(lambda: "IsViewable" in subprocess.check_output(["xwininfo", "-id", window], text=True), "open from any part of the single tray item")
                    xdo("windowfocus", window)
                    xdo("key", "ctrl+q")
                    app.wait(timeout=5)
                    wait_for(lambda: len(tray_buttons()) == 0, "quit removes the tray item")
                finally:
                    if app and app.poll() is None:
                        app.terminate()
                        app.wait(timeout=5)
                    panel.terminate()
                    panel.wait(timeout=5)
    print(f"TRAY_PANEL_SUITE_PASSED ({theme_name}): one weather + temperature item, units, shared hover/click area, and cleanup")


if __name__ == "__main__":
    run()
