# mixctl-core

Shared types and D-Bus interface definition used by both the daemon and CLI.

## What's in here

### `ChannelInfo`

The wire type sent over D-Bus when querying channels. Contains `id`, `name`, `color`, `muted`, and `volume`. Derives `zvariant::Type` for D-Bus serialization.

### `parse_hex_color`

Parses `"#RRGGBB"` strings into `(u8, u8, u8)`. Used by the daemon to validate color arguments.

### `dbus` module

Defines the `MixCtl` trait with the `#[zbus::proxy]` attribute, which generates `MixCtlProxy` for the CLI to call. The daemon implements this same interface via `#[zbus::interface]` on its `Service` type.

**D-Bus coordinates:**
- Bus name: `dev.greghuber.MixCtl`
- Object path: `/dev/greghuber/MixCtl1`
- Interface: `dev.greghuber.MixCtl1`

**Methods:**
| Method | Args | Returns |
|---|---|---|
| `ping` | — | `String` |
| `list_channels` | — | `Vec<ChannelInfo>` |
| `get_channel` | `id: u32` | `ChannelInfo` |
| `add_channel` | `name, color` | `u32` (assigned id) |
| `remove_channel` | `id: u32` | — |
| `set_channel_name` | `id, name` | — |
| `move_channel` | `id, position` | — |
| `set_channel_color` | `id, color` | — |
| `set_channel_mute` | `id, muted` | — |
| `set_channel_volume` | `id, volume` | — |
| `get_current_page` | — | `u32` |
| `set_current_page` | `page` | — |

## Dependencies

- `serde` — serialization for `ChannelInfo`
- `zbus` / `zvariant` — D-Bus proxy generation and type marshalling
