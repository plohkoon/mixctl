# mixctl-applet

System tray applet for mixctl. Provides quick access to output volumes and route controls from the system tray.

Communicates with the mixer daemon over D-Bus using `MixCtlProxy` from `mixctl-core`.

## Configuration

On startup, the applet fetches its config from the mixer daemon via a blocking D-Bus call (`get_config_section("applet")`). Falls back to defaults if the daemon is unavailable.

```toml
[applet]
window_width = 380
poll_interval_ms = 30
open_ui_command = "mixctl-ui"
```

## Dependencies

- `mixctl-core` — D-Bus proxy, shared types, config section types
- `serde` / `serde_json` — config section deserialization
- `ksni` — system tray (StatusNotifierItem) integration
- `gtk4` / `libadwaita` — GTK4 UI toolkit
- `zbus` — D-Bus client (async + blocking)
