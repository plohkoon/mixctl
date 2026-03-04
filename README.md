# mixctl

A Linux userspace controller for the [Beacn Mix / Mix Create](https://beacn.com/) USB audio mixer. The device has no official Linux support — mixctl aims to fill that gap with a daemon that manages channel configuration, PipeWire audio routing, a CLI for control, a GTK4 UI, and a system tray applet.

## Project structure

```
mixctl/
├── daemon/              # Long-running D-Bus service + PipeWire audio engine
│   └── src/
│       ├── main.rs          # Entry point, PW engine bootstrap, flush loop
│       ├── config.rs        # ~/.config/mixctl.toml — inputs, outputs, rules
│       ├── state.rs         # ~/.local/state/mixctl.toml — volumes, mutes, routes
│       ├── service.rs       # Shared state behind Arc<Mutex>
│       ├── dbus_adapter.rs  # D-Bus method implementations
│       └── audio/           # PipeWire audio routing engine
│           ├── mod.rs           # Module root, re-exports
│           ├── engine.rs        # PW thread, virtual sinks/sources, loopbacks
│           ├── commands.rs      # PwCommand enum (tokio → PW thread)
│           ├── events.rs        # PwEvent enum (PW thread → tokio)
│           └── volume.rs        # Cubic volume conversion (u8 ↔ f32)
├── cli/                 # Command-line client (talks to daemon over D-Bus)
│   └── src/main.rs
├── ui/                  # GTK4 mixer UI (input/output matrix view)
│   └── src/
│       ├── main.rs          # App entry, window setup
│       ├── strips.rs        # Volume strip widgets
│       ├── sidebar.rs       # Input/output sidebar
│       └── dbus.rs          # D-Bus proxy connection
├── applet/              # System tray applet (quick volume access)
│   └── src/
│       ├── main.rs          # Tray entry point
│       ├── tray.rs          # System tray integration
│       ├── output_strips.rs # Output volume strips
│       ├── route_strips.rs  # Route volume strips
│       └── dbus.rs          # D-Bus proxy connection
├── probe/               # USB experimentation tool for reverse-engineering
│   └── src/
│       ├── main.rs          # Probe CLI entry point
│       └── usb.rs           # USB device discovery, open, read/write
├── crates/
│   ├── core/            # Shared types and D-Bus interface definition
│   │   └── src/
│   │       ├── lib.rs       # InputInfo, OutputInfo, RouteInfo, StreamInfo, etc.
│   │       └── dbus.rs      # MixCtl proxy trait (generates MixCtlProxy)
│   └── protocol/        # USB wire protocol (reverse-engineered)
│       └── src/
│           ├── lib.rs       # Re-exports
│           ├── command.rs   # Outbound 8-byte USB commands
│           ├── input.rs     # Poll response parsing (dials, buttons)
│           ├── image.rs     # Image chunking for display transfer
│           ├── init.rs      # Device initialization + version info
│           ├── enums.rs     # Button, Dial, ButtonLighting, Color
│           └── consts.rs    # USB vendor/product IDs, endpoints
├── docker/              # Dev container with D-Bus + PipeWire
│   ├── Dockerfile.dev       # Rust + PipeWire + WirePlumber
│   ├── dbus-entrypoint.sh   # D-Bus session bus launcher
│   └── pipewire-entrypoint.sh # PipeWire + WirePlumber launcher
└── docker-compose.yml   # daemon + cli + pipewire + dbus + shell
```

### Crate dependency graph

```
mixctl-cli ──> mixctl-core <── mixctl-daemon (+ pipewire)
mixctl-ui ──>  mixctl-core
mixctl-applet ──> mixctl-core
mixctl-probe ──> mixctl-protocol
```

`core` and `protocol` are independent — core handles D-Bus IPC, protocol handles USB.

## How it works

### Input/output mixer model

mixctl models audio routing as an **input/output matrix**, inspired by the Beacn Mix hardware:

- **Inputs** are virtual sinks — audio destinations that apps play into. Each input becomes a PipeWire sink node (`Audio/Sink`). Examples: "System", "Game", "Music", "Chat".
- **Outputs** are virtual sources — mixed audio that other programs (OBS, Discord) can capture from. Each output becomes a PipeWire source node (`Audio/Source/Virtual`). Examples: "Personal Mix", "Voice Chat Mix", "Audience Mix".
- **Routes** connect inputs to outputs with per-route volume and mute. Each route is a `libpipewire-module-loopback` instance that carries audio from an input sink's monitor to an output source.

```
                    Outputs
                    Personal  Voice  Audience  VOD
            ┌──────┬────────┬──────┬─────────┬─────┐
  Inputs    │      │  100%  │ 50%  │  100%   │ 80% │
            ├──────┼────────┼──────┼─────────┼─────┤
  System    │      │   ●    │  ●   │    ●    │  ●  │
  Game      │      │   ●    │      │    ●    │  ●  │
  Music     │      │   ●    │      │         │  ●  │
  Chat      │      │   ●    │  ●   │    ●    │     │
            └──────┴────────┴──────┴─────────┴─────┘

  ● = route enabled (volume > 0, not muted)
```

App audio streams are assigned to inputs (automatically via rules or manually), and the matrix determines which outputs they appear on and at what volume.

### Architecture

```
 tokio thread (D-Bus + state)              PipeWire thread (MainLoop)
 ───────────────────────────────          ─────────────────────────────
 │ Service / Shared / D-Bus    │ Command  │ MainLoop + Registry       │
 │                              │ ═══════>│ create/destroy nodes      │
 │  tokio::mpsc relay ──────── │ ──────> │ create/destroy links      │
 │                              │pw::chan │ set volume/mute props     │
 │                              │ Event   │ monitor registry events   │
 │                              │ <═══════│                           │
 │                              │tokio::  │                           │
 │                              │mpsc     │                           │
 ───────────────────────────────          ─────────────────────────────
```

The daemon runs two threads:
- **Tokio thread**: D-Bus service, config/state management, app rule matching. Exposes `dev.greghuber.MixCtl1` on the session bus.
- **PipeWire thread**: Dedicated OS thread running `pipewire::MainLoop`. All PipeWire objects (nodes, modules, metadata) live here since they're not `Send`/`Sync`.

Communication:
- **Commands** (tokio → PW): `tokio::mpsc` relay task forwards to `pipewire::channel::Sender`, which wakes the PW main loop. The PW channel sender is wrapped in `Arc<Mutex<Option<Sender>>>` and swapped on reconnection.
- **Events** (PW → tokio): `tokio::mpsc::UnboundedSender` from PW thread to tokio for state updates and signal emission.
- **Reconnection**: If PipeWire disconnects, the PW thread retries with exponential backoff (1s → 30s). On reconnection it creates a fresh `pipewire::channel` and sends a `ChannelReady` event so the tokio relay swaps to the new sender. Shutdown is coordinated via a shared `AtomicBool`.

### PipeWire audio routing (detailed)

#### Why virtual sinks and sources?

PipeWire's native link system connects port-to-port with no volume control. A raw link from node A to node B passes audio at unity gain with no way to attenuate. This doesn't work for a mixer where every crosspoint needs independent volume.

#### How loopback modules solve this

Each route in the matrix uses `libpipewire-module-loopback`, which creates an intermediate node with its own volume control. The data path for a single route is:

```
App audio → [Input Sink] → monitor ports → [Loopback Node (volume)] → [Output Source]
```

The loopback module is loaded via `pw_context_load_module("libpipewire-module-loopback", ...)` with `capture.props` targeting the input sink and `playback.props` targeting the output source. Volume is set as a combined `f32` (route × output, cubic-scaled) via `channelmix.volume` in playback props. When a volume slider moves, the daemon updates the existing loopback node in-place via SPA `set_param` with `channelVolumes` — no module destroy/recreate needed, avoiding audio glitches.

#### Why 7.1 channel layout?

All virtual nodes use 8-channel layout (`FL,FR,FC,LFE,RL,RR,SL,SR`). This is deliberate:
- Stereo apps only use FL/FR — PipeWire fills the rest with silence automatically.
- Surround apps (games, movies) use all channels without lossy downmix.
- PipeWire handles channel conversion at link boundaries, so a stereo app connecting to a 7.1 sink just works.
- Using a narrower layout would force downmix of surround content, losing spatial information.

#### Combined volume

Each route loopback carries a combined volume that folds both the per-route volume and the output master volume into a single PipeWire value: `combined = cubic(route_vol) × cubic(output_vol)`. There are no separate mixer nodes or output-volume loopbacks — this keeps the graph minimal. When an output volume or mute changes, the daemon recomputes and updates all route loopbacks on that output.

#### Mute strategy

- **Route mute**: Set the loopback node's combined volume to `0.0`. The link stays alive (no graph churn, no reconnection glitch on unmute). Saved volume is restored on unmute.
- **Output master mute**: Fans out as combined volume `0.0` on every route loopback for that output. Same mechanism as route mute — no separate mute property on the output source node.

#### Volume conversion

The daemon uses `u8` 0-100 for the user-facing volume. PipeWire uses `f32` on a cubic scale. The conversion uses `linear^3` (cubic), which maps perceptually: 50% on the slider is approximately -18dB, which sounds like "half volume" to human ears.

```rust
fn u8_to_pw_volume(v: u8) -> f32 {
    let linear = (v as f32) / 100.0;
    linear * linear * linear  // cubic
}
```

#### Default sink

The default PipeWire sink (where unassigned apps play) is set via the PipeWire metadata system: `metadata.set_property(0, "default.audio.sink", "Spa:String:JSON", "{\"name\": \"mixctl.input.{id}\"}")`. This is found by monitoring registry globals for the metadata object with `metadata.name = "default"`.

#### Stream auto-assignment

The PipeWire engine monitors the registry for `Stream/Output/Audio` nodes (app playback streams). When a new stream appears:
1. The `app_name` is matched against configured app rules (exact match, then glob via `glob-match` crate).
2. If a rule matches, the stream is moved to that input via metadata `target.node`.
3. If no rule matches, it goes to the default input.
4. If no default input, it goes to the first input.

#### Capture devices

Hardware capture devices (microphones, line-in) are discovered by monitoring for `Audio/Source` nodes in the PipeWire registry (excluding `mixctl.*` nodes). They can be added as mixer inputs, which creates a virtual sink + a loopback from the hardware device to that sink, keeping the architecture uniform — all inputs are sinks with monitors.

### PipeWire node naming

| Node type | `node.name` pattern | `media.class` |
|---|---|---|
| Input sink | `mixctl.input.{id}` | `Audio/Sink` |
| Output source | `mixctl.output.{id}` | `Audio/Source/Virtual` |
| Route loopback | `mixctl.route.{input_id}.{output_id}` | (module-created) |
| Output target | `mixctl.output-target.{id}` | (module-created) |
| Capture loopback | `mixctl.capture.{id}` | (module-created) |

## Config and state

**Config** (`~/.config/mixctl.toml`) — channel definitions, app rules:
```toml
version = 1
default_input = 1

[[inputs]]
id = 1
name = "System"
color = "#4A90D9"

[[inputs]]
id = 2
name = "Game"
color = "#E74C3C"

[[outputs]]
id = 5
name = "Personal Mix"
color = "#8E44AD"
target_device = "alsa_output.pci-0000_00_1f.3.analog-stereo"

[[outputs]]
id = 6
name = "Voice Chat Mix"
color = "#3498DB"

[[app_rules]]
app_name = "firefox"
input_id = 1

[[app_rules]]
app_name = "spotify"
input_id = 3
```

**State** (`~/.local/state/mixctl.toml`) — runtime values:
```toml
version = 1
current_page = 0

[outputs.5]
volume = 100
muted = false

[routes."1:5"]
volume = 80
muted = false

[routes."2:5"]
volume = 100
muted = false

[capture_volumes.7]
volume = 80
muted = false
```

Channel order is determined by position in the config arrays, not by ID. The `move` command rearranges this order. IDs are shared across inputs and outputs (no collisions).

## CLI usage

```bash
# Status and connectivity
mixctl-cli ping
mixctl-cli status

# Input management
mixctl-cli input list
mixctl-cli input get <id>
mixctl-cli input add "Voice" "#9B59B6"
mixctl-cli input remove <id>
mixctl-cli input set-name <id> "New Name"
mixctl-cli input set-color <id> "#FF0000"
mixctl-cli input move <id> <position>
mixctl-cli input get-default
mixctl-cli input set-default <id>

# Output management
mixctl-cli output list
mixctl-cli output get <id>
mixctl-cli output add "Stream Mix" "#FF5500" 0    # 0 = default routes
mixctl-cli output add "Copy Mix" "#FF5500" 5      # copy routes from output 5
mixctl-cli output remove <id>
mixctl-cli output set-volume <id> 85
mixctl-cli output set-mute <id> true
mixctl-cli output set-target <id> "alsa_output.pci-0000_00_1f.3.analog-stereo"

# Route management (input→output matrix cells)
mixctl-cli route list <output_id>
mixctl-cli route get <input_id> <output_id>
mixctl-cli route set-volume <input_id> <output_id> 75
mixctl-cli route set-mute <input_id> <output_id> true

# Audio stream assignment
mixctl-cli stream list
mixctl-cli stream assign <pw_node_id> <input_id> --remember

# App rules (auto-assign streams)
mixctl-cli rule list
mixctl-cli rule set firefox 1
mixctl-cli rule set "spotify*" 3    # glob patterns supported
mixctl-cli rule remove firefox

# Hardware capture devices
mixctl-cli capture list
mixctl-cli capture add <pw_node_id> "Mic" "#FF5555"

# Pagination
mixctl-cli page get
mixctl-cli page set 0

# Live signal monitoring
mixctl-cli listen all
mixctl-cli listen state     # output volume/mute + route changes
mixctl-cli listen config    # input/output additions/removals
```

## Development

### Building

```bash
cargo build
```

The daemon requires `libpipewire-0.3-dev` and `libclang-dev` to build. The CLI, UI, and applet build on any platform (they just talk D-Bus).

### On macOS (no D-Bus or PipeWire)

Use the Docker setup to get a Linux environment with D-Bus and PipeWire:

```bash
# Start the daemon (also starts dbus + pipewire services)
docker compose up daemon

# Run CLI commands
docker compose run --rm cli cargo run -p mixctl-cli -- status
docker compose run --rm cli cargo run -p mixctl-cli -- input list

# Interactive shell for PipeWire debugging
docker compose run --rm shell bash -c 'wpctl status'
docker compose run --rm shell bash -c 'pw-dump'

# Monitor D-Bus traffic
docker compose --profile debug up monitor
```

### Docker services

| Service | Purpose |
|---|---|
| `dbus` | Session bus (shared via `/bus` volume) |
| `pipewire` | PipeWire + WirePlumber (shared via `/run/pipewire` volume) |
| `daemon` | mixctl-daemon (depends on dbus + pipewire) |
| `cli` | Runs a single CLI command |
| `shell` | Interactive shell with PipeWire tools (profile: debug) |
| `monitor` | D-Bus traffic monitor (profile: debug) |

### Running locally (Linux)

```bash
# Terminal 1
cargo run -p mixctl-daemon

# Terminal 2
cargo run -p mixctl-cli -- input list
cargo run -p mixctl-cli -- status
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
cargo test -p mixctl-daemon    # volume conversion tests
```

## Future work

- **USB integration**: connect the daemon to the actual Beacn Mix hardware via `mixctl-protocol` — sync volume/mute state to physical dials and LEDs
- **Input handling**: read dial turns and button presses from the device, update channel state in real-time
- **Display rendering**: push channel names, volumes, and colors to the device's built-in screen
- **LED sync**: set dial LED colors from channel config
- **Hot-plug**: detect device connect/disconnect via udev

## License

MIT
