# mixctl-ui

GTK4/Libadwaita mixer UI for mixctl. Provides a graphical input/output matrix view with volume sliders, mute controls, and stream assignment.

Communicates with the mixer daemon over D-Bus using `MixCtlProxy` from `mixctl-core`.

## Configuration

On startup, the UI fetches its config from the mixer daemon via a blocking D-Bus call (`get_config_section("ui")`). Falls back to defaults if the daemon is unavailable.

```toml
[ui]
window_width = 750
window_height = 450
margin = 12
```

## Dependencies

- `mixctl-core` — D-Bus proxy, shared types, config section types
- `serde` / `serde_json` — config section deserialization
- `gtk4` / `libadwaita` — GTK4 UI toolkit
- `zbus` — D-Bus client (async + blocking)
- `futures-lite` — signal stream handling
