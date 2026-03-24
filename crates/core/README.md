# mixctl-core

Shared types and D-Bus interface definition used by the daemon, CLI, TUI, and desktop UI.

## Types

### Wire types (sent over D-Bus)

| Type | Fields | Description |
|---|---|---|
| `InputInfo` | `id, name, color` | Input channel (config-only, no volume) |
| `OutputInfo` | `id, name, color, volume, muted` | Output channel (has master volume) |
| `RouteInfo` | `input_id, output_id, volume, muted` | Matrix cell (input→output) |
| `StreamInfo` | `pw_node_id, app_name, media_name, input_id` | Active PipeWire stream |
| `AppRuleInfo` | `app_name, input_id` | Auto-assignment rule |
| `CaptureDeviceInfo` | `pw_node_id, name, device_name, is_added, input_id` | Hardware capture device |

All types derive `Serialize`, `Deserialize`, and `zvariant::Type` for D-Bus marshalling.

### Config section types (`config_sections` module)

Shared configuration structs for component subsections, fetched over D-Bus as JSON:

| Type | Fields | Description |
|---|---|---|
| `BeacnConfig` | `layout, dial_sensitivity, level_decay` | Beacn hardware daemon settings |
| `UiConfig` | `window_width, window_height, margin` | Desktop UI settings |
| `AppletConfig` | `window_width, poll_interval_ms` | System tray applet settings |
| `CliConfig` | `color_output, output_format` | CLI output settings |

All types derive `Serialize`, `Deserialize`, `Default`, and `PartialEq`. Each field has `#[serde(default)]` for field-level defaults matching current hardcoded values.

### Utilities

- `parse_hex_color(s: &str) -> Option<(u8, u8, u8)>` — parses `"#RRGGBB"` strings.

## D-Bus interface

The `dbus` module defines the `MixCtl` trait with `#[zbus::proxy]`, which generates `MixCtlProxy` for clients. The daemon implements this same interface via `#[zbus::interface]` on its `Service` type.

**D-Bus coordinates:**
- Bus name: `dev.greghuber.MixCtl`
- Object path: `/dev/greghuber/MixCtl1`
- Interface: `dev.greghuber.MixCtl1`

### Methods

| Method | Args | Returns | Description |
|---|---|---|---|
| `ping` | — | `String` | Health check |
| `get_audio_status` | — | `String` | PipeWire connection status |
| `get_default_input` | — | `u32` | Default input ID (0 = none) |
| `set_default_input` | `id` | — | Set PipeWire default sink |
| `list_inputs` | — | `Vec<InputInfo>` | All inputs |
| `get_input` | `id` | `InputInfo` | Single input |
| `add_input` | `name, color` | `u32` | Add input, returns ID |
| `remove_input` | `id` | — | Remove input |
| `move_input` | `id, position` | — | Reorder input |
| `set_input_name` | `id, name` | — | Rename input |
| `set_input_color` | `id, color` | — | Change color |
| `list_outputs` | — | `Vec<OutputInfo>` | All outputs |
| `get_output` | `id` | `OutputInfo` | Single output |
| `add_output` | `name, color, source_output_id` | `u32` | Add output, copy routes from source |
| `remove_output` | `id` | — | Remove output |
| `move_output` | `id, position` | — | Reorder output |
| `set_output_name` | `id, name` | — | Rename output |
| `set_output_color` | `id, color` | — | Change color |
| `set_output_volume` | `id, volume` | — | Set master volume (0-100) |
| `set_output_mute` | `id, muted` | — | Set master mute |
| `set_output_target` | `id, device_name` | — | Link to physical device |
| `get_route` | `input_id, output_id` | `RouteInfo` | Single route |
| `list_routes_for_output` | `output_id` | `Vec<RouteInfo>` | All routes for an output |
| `set_route_volume` | `input_id, output_id, volume` | — | Set route volume (0-100) |
| `set_route_mute` | `input_id, output_id, muted` | — | Set route mute |
| `list_streams` | — | `Vec<StreamInfo>` | Active audio streams |
| `assign_stream` | `pw_node_id, input_id, remember` | — | Move stream, optionally save rule |
| `list_app_rules` | — | `Vec<AppRuleInfo>` | All auto-assignment rules |
| `set_app_rule` | `app_name, input_id` | — | Add/update rule |
| `remove_app_rule` | `app_name` | — | Delete rule |
| `list_capture_devices` | — | `Vec<CaptureDeviceInfo>` | Available hardware devices |
| `add_capture_input` | `pw_node_id, name, color` | `u32` | Add device as mixer input |
| `remove_capture_input` | `id` | — | Remove capture loopback (keep input) |
| `set_capture_volume` | `id, volume` | — | Set capture gain (0-100) |
| `set_capture_mute` | `id, muted` | — | Set capture mute |
| `get_current_page` | — | `u32` | Current display page |
| `set_current_page` | `page` | — | Set display page |
| `get_broadcast_levels` | — | `bool` | Level monitoring enabled? |
| `set_broadcast_levels` | `enabled` | — | Toggle level monitoring |
| `get_input_levels` | — | `Vec<(u32, f64)>` | Current input audio levels |
| `get_config_section` | `section` | `String` | Get config section as JSON |
| `set_config_section` | `section, json` | — | Update config section from JSON |

### Signals

| Signal | Args | Description |
|---|---|---|
| `inputs_config_changed` | — | Input added/removed/reordered |
| `outputs_config_changed` | — | Output added/removed/reordered |
| `output_state_changed` | `id` | Output volume/mute changed |
| `route_changed` | `input_id, output_id` | Route volume/mute changed |
| `page_changed` | `page` | Display page changed |
| `audio_status_changed` | — | PipeWire connection state changed |
| `streams_changed` | — | Stream appeared/removed/reassigned |
| `app_rules_changed` | — | Rule added/updated/removed |
| `capture_devices_changed` | — | Capture device appeared/removed |
| `input_levels_changed` | `levels: Vec<(u32, f64)>` | Per-input audio levels updated |
| `broadcast_levels_changed` | `enabled` | Level monitoring toggled |
| `config_section_changed` | `section` | Config section updated |

## Dependencies

- `serde` — serialization for all types
- `serde_json` — JSON serialization for config sections
- `zbus` / `zvariant` — D-Bus proxy generation and type marshalling
