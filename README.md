# mixctl

A Linux userspace controller for the [Beacn Mix / Mix Create](https://beacn.com/) USB audio mixer. The device has no official Linux support — mixctl aims to fill that gap with a daemon that manages channel configuration, a CLI for control, and (eventually) direct USB communication with the hardware.

## Project structure

```
mixctl/
├── daemon/              # Long-running D-Bus service
│   └── src/
│       ├── main.rs          # Entry point, flush loop, signal handling
│       ├── config.rs        # ~/.config/mixctl.toml — channel definitions
│       ├── state.rs         # ~/.local/state/mixctl.toml — volumes, mutes
│       ├── service.rs       # Shared state behind Arc<Mutex>
│       └── dbus_adapter.rs  # D-Bus method implementations
├── cli/                 # Command-line client (talks to daemon over D-Bus)
│   └── src/
│       └── main.rs          # CLI entry point, clap subcommands
├── probe/               # USB experimentation tool for reverse-engineering
│   └── src/
│       ├── main.rs          # Probe CLI entry point
│       └── usb.rs           # USB device discovery, open, read/write
├── crates/
│   ├── core/            # Shared types: ChannelInfo, D-Bus proxy trait
│   │   └── src/
│   │       ├── lib.rs       # ChannelInfo, parse_hex_color
│   │       └── dbus.rs      # MixCtl trait + proxy generation
│   └── protocol/        # USB wire protocol (reverse-engineered)
│       └── src/
│           ├── lib.rs       # Re-exports
│           ├── command.rs   # Outbound 8-byte USB commands
│           ├── input.rs     # Poll response parsing (dials, buttons)
│           ├── image.rs     # Image chunking for display transfer
│           ├── init.rs      # Device initialization + version info
│           ├── enums.rs     # Button, Dial, ButtonLighting, Color
│           └── consts.rs    # USB vendor/product IDs, endpoints
├── docker/              # Dev container with D-Bus (for building on macOS)
└── docker-compose.yml   # daemon + cli + shared D-Bus session bus
```

### Crate dependency graph

```
mixctl-cli ──> mixctl-core <── mixctl-daemon
mixctl-probe ──> mixctl-protocol
```

`core` and `protocol` are independent — core handles D-Bus IPC, protocol handles USB.

## How it works

**Daemon** (`mixctl-daemon`) loads config and state from disk, exposes a D-Bus interface (`dev.greghuber.MixCtl1`), and periodically flushes dirty state back to disk. Config holds channel definitions (id, name, color); state holds runtime values (volume, mute). On startup, state is reconciled with config — missing channels get defaults, stale entries are pruned.

**CLI** (`mixctl-cli`) connects to the daemon over the session bus. All mutations go through D-Bus.

**Probe** (`mixctl-probe`) talks directly to the USB device via `rusb`. Used for reverse-engineering the protocol — can discover devices, send raw commands, read dial/button input, set LED colors, transfer images to the display, etc.

**Protocol** (`mixctl-protocol`) encodes the USB wire format: 8-byte commands, input event parsing, image chunking (1024-byte packets), LED color zones, button/dial enums.

## Channel model

Channels are identified by a stable integer ID. The name is a mutable display label.

**Config** (`~/.config/mixctl.toml`):
```toml
version = 1

[[channels]]
id = 1
name = "System"
color = "#4A90D9"

[[channels]]
id = 2
name = "Game"
color = "#E74C3C"
```

**State** (`~/.local/state/mixctl.toml`):
```toml
version = 1
current_page = 0

[channels.1]
muted = false
volume = 100

[channels.2]
muted = false
volume = 100
```

Channel order is determined by position in the config array, not by ID. The `move` command rearranges this order.

If a config file is hand-edited with missing or duplicate IDs, the daemon fixes them up on load (assigns the next unused positive integer) and logs a warning.

## CLI usage

```bash
# Basics
mixctl-cli ping
mixctl-cli channel list
mixctl-cli channel get 1

# Mutating channels
mixctl-cli channel add "Voice" "#9B59B6"
mixctl-cli channel remove 5
mixctl-cli channel set-name 1 "System Audio"
mixctl-cli channel set-color 1 "#FF0000"
mixctl-cli channel move 1 3

# Volume and mute
mixctl-cli channel set-volume 1 85
mixctl-cli channel set-mute 1 true

# Pagination
mixctl-cli page get
mixctl-cli page set 0
```

## Development

### Building

```bash
cargo build
```

### On macOS (no D-Bus)

Use the Docker setup to get a Linux environment with D-Bus:

```bash
docker compose run daemon    # start the daemon
docker compose run cli       # run CLI commands against it
```

### Running locally (Linux)

```bash
# Terminal 1
cargo run -p mixctl-daemon

# Terminal 2
cargo run -p mixctl-cli -- channel list
```

### USB probe

```bash
cargo run -p mixctl-probe -- discover
cargo run -p mixctl-probe -- listen      # live dial/button input
cargo run -p mixctl-probe -- led-color dial1 255 0 0
```

### Tests

```bash
cargo test -p mixctl-core
```

## What needs testing

The channel management layer is currently speculative — it models what we *think* the right abstraction is, but hasn't been validated against the actual hardware yet. Specific things to test once hardware integration is in place:

- **Round-trip config persistence**: add/remove/rename/reorder channels, kill daemon, restart, verify config and state are correct
- **State reconciliation**: hand-edit config to add/remove channels, restart daemon, verify state is cleaned up
- **ID fixup**: remove `id` fields from config, add duplicates, verify daemon assigns valid unique IDs
- **Page clamping**: remove channels until page count shrinks, verify `current_page` is clamped
- **Concurrent access**: multiple CLI clients mutating channels simultaneously

## Future work

- **USB integration**: connect the daemon to the actual Beacn Mix hardware via `mixctl-protocol` — sync volume/mute state to physical dials and LEDs
- **Input handling**: read dial turns and button presses from the device, update channel state in real-time
- **Display rendering**: push channel names, volumes, and colors to the device's built-in screen
- **LED sync**: set dial LED colors from channel config
- **PipeWire/PulseAudio integration**: map channels to actual audio sinks/sources so volume changes affect system audio
- **Desktop integration**: system tray, media key support
- **Hot-plug**: detect device connect/disconnect via udev

## License

MIT
