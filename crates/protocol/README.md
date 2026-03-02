# mixctl-protocol

USB wire protocol for the Beacn Mix and Mix Create devices. There is no official documentation — the protocol was reverse-engineered by [beacn-on-linux/beacn-lib](https://github.com/beacn-on-linux/beacn-lib) and re-implemented in this crate.

## Modules

### `consts`

USB identifiers (vendor `0x33ae`, product `0x0004` for Mix, `0x0007` for Mix Create), endpoint addresses, and `DeviceType` enum.

### `enums`

Device-level enumerations:
- `Button` — 11 buttons (AudienceMix, PageLeft, PageRight, Dial1–4, Audience1–4)
- `Dial` — 4 rotary encoders
- `ButtonLighting` — 7 LED zones (Dial1–4, Mix, Left, Right)
- `Color` — RGBA color value

### `command`

Outbound USB commands. Each serializes to an 8-byte array:

| Command | Bytes |
|---|---|
| `DisplayBrightness(u8)` | `00 00 00 04 <val> 00 00 00` |
| `DisplayPower(bool)` | `00 01 00 04 <flag> ...` |
| `ButtonLedBrightness(u8)` | `01 07 00 04 <val> ...` |
| `ButtonLedColor { zone, color }` | `01 <zone> 00 04 <B> <G> <R> <A>` |
| `Wake` | `00 00 00 F1 00 00 00 00` |
| `Poll` | `00 00 00 05 00 00 00 00` |

Note: LED colors are sent in **BGR** order.

### `input`

Parses 10+ byte poll responses into `InputEvent`:
- Bytes 4–7: dial deltas (signed `i8`, one per dial)
- Bytes 8–9: button bitmask (big-endian `u16`)

### `init`

Initialization payload (`[0x00, 0x00, 0x00, 0x01]`) and `VersionInfo` response wrapper.

### `image`

`ImageChunker` splits image data into 1024-byte USB packets for display transfer:
- Data packets: `[index_LE(3), 0x50, payload(1020)]`
- Final packet: `[0xFF x4, total_size_LE(4), x_LE(2), y_LE(2)]`

## USB details

- Interrupt OUT endpoint: `0x03` (1024 bytes max)
- Interrupt IN endpoint: `0x83` (64 bytes max)
- Interface 0, alternate setting 1
