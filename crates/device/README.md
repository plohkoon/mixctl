# mixctl-beacn-device

USB device communication layer for Beacn Mix and Mix Create hardware. Provides the `Device` struct for low-level USB I/O and the `DeviceThread` for managed device interaction with display rendering, input polling, and LED control.

## Components

### `usb` module

Adapted from the original probe tool. Handles USB device lifecycle:

- `Device::open()` — find device by VID/PID, detach kernel driver, claim interface, send init, read version
- `write_command()` / `write_raw()` / `write_raw_timeout()` — interrupt OUT transfers
- `read()` — interrupt IN transfers
- `discover()` — enumerate all Beacn devices on the USB bus
- Automatic cleanup on drop (release interface, reattach kernel driver)

### `DeviceThread`

Spawns a dedicated OS thread that manages the full device lifecycle:

- **Outer loop**: Attempts `Device::open()` with 2-second backoff (up to 30s). Sends `Connected`/`Disconnected` events.
- **Inner loop** (~50ms tick):
  - Receives `DeviceCommand::UpdateState` — triggers display rendering
  - Sends `Command::Poll`, reads response, parses button/dial input
  - Rising-edge button detection (fires once per press, not on hold)
  - Maps hardware input to `DeviceEvent` variants
  - Renders display updates (full frame or incremental patches via `DisplayLayout`)
  - Updates LED colors based on mixer state
- **Shutdown**: Turns off LEDs and display, exits thread

### Channel types

```
Host daemon                          Device OS thread
     │                                     │
     │  DeviceCommand (unbounded mpsc)     │
     ├────────────────────────────────────>│  try_recv() in poll loop
     │                                     │
     │  DeviceEvent (unbounded mpsc)       │
     │<────────────────────────────────────┤  send() from sync code
     │                                     │
     │  Arc<AtomicBool> shutdown_flag      │
     ├─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ >│  checked each iteration
```

### Button/dial mapping

| Input | DeviceEvent |
|-------|------------|
| Dial 1-4 turn | `AdjustRouteVolume { delta }` |
| Dial 1-4 press | `ToggleRouteMute` (current output) |
| Audience 1-4 | `ToggleGlobalMute` (all outputs) |
| AudienceMix | `NextOutput` |
| PageLeft/Right | `PageLeft` / `PageRight` |

### LED mapping

| Zone | Color |
|------|-------|
| Dial 1-4 | Input color at 70% (red if global muted, dim gray if route muted) |
| Mix | Current output color |
| Left/Right | White if page available, dim gray otherwise |

## Dependencies

- `mixctl-protocol` — USB command encoding, image chunking, input parsing
- `mixctl-beacn-display` — display layout rendering
- `rusb` — USB device access
- `tokio` (sync feature) — mpsc channels
