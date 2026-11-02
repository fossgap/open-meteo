# Forecast fixture

`forecast.json` contains synthetic weather for Paris starting 3 June 2026.
Temperatures are Celsius, wind is km/h, precipitation is a probability in percent,
and timestamps are UTC Unix seconds. The response's daily offset is UTC+2.

It is used by unit tests and the explicitly labelled `--demo` mode, never as a
fallback for a live request. Keep all parallel arrays aligned when editing it.
