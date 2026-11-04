# Using Open Meteo

[README](../README.md) · [Development](development.md)

Choose **Find a location**, search for your city, and select a result. Scroll the
hourly strip sideways to see all 24 hours, or select a day to expand its details.
All forecast times use the selected location's time zone, including daylight
saving changes.

To explore the interface without a connection or changes to your saved settings:

```sh
open-meteo --demo
```

The preview is clearly labelled and uses synthetic Paris weather. It does not
fetch live data. Run without `--demo` for normal location search and forecasts.

## Refresh interval

Choose **Main menu → Refresh interval** for automatic updates every **5, 15, 30,
or 60 minutes**, or **Manual only**. The default is 15 minutes. Changes take effect
immediately and are remembered between sessions. Automatic updates continue while
the app runs in the tray or GNOME clock.

**Manual only** stops background updates, including requests from the GNOME
calendar. Launching the app with a saved location or choosing a location still
fetches its forecast. The refresh button, **Ctrl+R**, **Retry**, and the tray's
**Refresh** action always request an update. A manual request restarts the
selected interval; changing the interval starts a new countdown.

## System tray and shortcuts

On desktops with a StatusNotifier/AppIndicator host, closing the window keeps the
app in the tray by default. Click the icon to reopen the forecast, or open its menu
for a quick outlook. Turn off **Keep running in system tray** in the main menu to
quit when the window closes. **Quit** always exits the app.

The tray contains one item with the condition symbol and temperature side by
side. GNOME's AppIndicator host and Unity use a native icon and text label in
one button. Icon-only hosts use a combined horizontal image, whose size is
limited by the panel's icon setting. Its tooltip includes the location,
condition, unit, and whether the forecast is saved. If neither a tray nor the
GNOME clock extension is available, closing quits normally. Hidden windows stay
hidden across screen lock and unlock. If the last panel connection disappears,
the app waits three seconds for it to reconnect before reopening the window;
this wait starts after unlocking when the screen is locked.

### GNOME system clock

Choose **Main menu → GNOME clock setup** to install and enable the integration
in one action, or run:

```sh
open-meteo --install-gnome-extension
# For an AppImage, use its filename instead:
# ./Open_Meteo-0.2.1-x86_64.AppImage --install-gnome-extension
```

If GNOME already knows the extension, setup enables it immediately and checks
that it started. There is no separate command or Extensions toggle to find.

For a **new local installation**, GNOME may not have discovered the extension
yet. Setup detects this and saves its enabled state for the next session. Only
in that case, sign out and back in on Wayland, or restart GNOME Shell with
**Alt+F2 → `r`** on X11; it then enables automatically. GNOME loads local
extensions at Shell startup and caches their JavaScript modules ([GNOME's
extension guide](https://gjs.guide/extensions/development/creating.html#testing-the-extension)).
An updated extension may also need a new Shell session to load its new code.
The app never restarts your session or changes the global extension switch.
Setup reports disabled extensions or unsupported Shell versions separately.

Launch Open Meteo and select a location. The extension shows the weather directly
inside the existing clock button. Click it to see a weather card below the
calendar, with the location and five upcoming hours of icons and temperatures.
Click the weather card to open the full forecast. Opening the calendar respects
the refresh interval selected in the app. Closing the app window keeps
it running when the panel preference is enabled; **Quit** removes the indicator.
The extension does not launch the app automatically.

The extension targets GNOME Shell 45–50; automated Shell integration tests run on
GNOME 50. It shares the app's forecast over the session bus; weather requests run
in the app. A `~` before the temperature marks saved weather. While the clock
extension is connected, the ordinary tray item is unregistered. It returns if
the extension is disabled or disconnects. GNOME's original weather card is restored
when Open Meteo exits or the extension is disabled.
Native packages also install the extension under `/usr/share/gnome-shell/extensions`.
The user installer works with every package format and source builds.

To disable the clock integration:

```sh
gnome-extensions disable open-meteo-clock@fossgap.github.io
```

### Language

The app follows `LANGUAGE`, then the desktop's `LC_ALL` / `LC_MESSAGES` / `LANG`
locale, with English as fallback. Choose **Main menu → Language (restart required)**
to override it, then quit and reopen the app. Available languages are English,
Français, Deutsch, Español, 简体中文, हिन्दी, and العربية. Forecast dates use the
Gregorian calendar and the selected location's time zone. Search requests ask for
place names in the selected language; the provider may return native or English names.

Translations are embedded in the binary. To contribute, edit the UTF-8 catalogs in
[`resources/locales`](../resources/locales), preserving named fields such as
`{temperature}`. Tests check catalog completeness and matching fields.

| Shortcut | Action |
| --- | --- |
| `Ctrl+L` | Search locations |
| `Ctrl+R` | Refresh weather |
| `Ctrl+Q` | Quit, including the tray |
| `F1` | About and data credits |
| `Esc` | Close location search |

## Data, storage, and privacy

Forecasts come from the [Open-Meteo Forecast API](https://open-meteo.com/en/docs).
Search uses the [Open-Meteo Geocoding API](https://open-meteo.com/en/docs/geocoding-api),
with location data from [GeoNames](https://www.geonames.org/). Credits are also
linked inside the app.

The public endpoints require no key for non-commercial use and are subject to
the provider's [usage terms and data attribution requirements](https://open-meteo.com/en/terms).
This client currently supports those public endpoints; commercial endpoint and
API-key configuration are not implemented.

The app has no telemetry. Searching sends your search text to the geocoding
service; fetching weather sends the chosen coordinates to the forecast service.
Those services also receive normal connection information, including your IP
address.

| File | Purpose |
| --- | --- |
| `$XDG_CONFIG_HOME/open-meteo/settings.json` | Location, recent places, units, language, refresh interval, and panel preference |
| `$XDG_CACHE_HOME/open-meteo/<latitude>_<longitude>.json` | Last successful forecast for a location |

The defaults are `~/.config` and `~/.cache`. Settings are written atomically;
unreadable settings and failed writes are reported. Invalid cached forecasts are
ignored. Removing the app's directories resets its saved state.
