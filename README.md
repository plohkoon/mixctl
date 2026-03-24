# mixctl

A Linux userspace controller for the [Beacn Mix / Mix Create](https://beacn.com/) USB audio mixer. The device has no official Linux support — mixctl aims to fill that gap with a daemon that manages channel configuration, PipeWire audio routing, hardware device control, a CLI, a TUI, a desktop UI, and a system tray applet.

## Project structure

```
mixctl/
├── daemon/                  # Mixer daemon (PipeWire + D-Bus service)
├── apps/
│   ├── cli/                 # Command-line client
│   ├── tui/                 # Terminal UI (ratatui matrix grid)
│   └── tauri/               # Desktop UI + system tray applet (React + Tauri)
├── crates/
│   ├── core/                # Shared types + D-Bus interface definition
│   ├── protocol/            # USB wire protocol (reverse-engineered)
│   ├── display/             # Display rendering engine (state → JPEG)
│   └── device/              # USB device thread + communication
├── devices/
│   └── beacn-daemon/        # Beacn hardware controller daemon (D-Bus client)
├── tools/
│   ├── beacn-probe/         # USB experimentation/debug tool
│   └── beacn-test/          # Standalone device test harness (fake state)
└── docker/                  # Dev container (D-Bus + PipeWire)
```

### Crate dependency graph

```
                    ┌─── mixctl-cli
                    │
                    ├─── mixctl-tui
mixctl-core ◄───────┤
(D-Bus types)       ├─── mixctl-tauri (desktop UI + applet)
                    │
                    └─── mixctl-beacn-daemon ──► mixctl-beacn-device ──► mixctl-protocol
                                               │                  (USB commands)
                                               └──► mixctl-beacn-display
                                                    (JPEG rendering)

mixctl-daemon ──► mixctl-core
              └──► pipewire

mixctl-beacn-probe ──► mixctl-beacn-device ──► mixctl-protocol
```

`core` handles D-Bus IPC. `protocol` handles USB wire format. `display` renders JPEG frames. `device` manages the USB thread. These are independent concerns — the mixer daemon has no device dependencies.

## Architecture

```
┌─────────────────────┐     D-Bus session bus     ┌──────────────────────┐
│   mixctl-daemon     │◄════════════════════════► │  mixctl-beacn-daemon │
│   (PipeWire engine) │    signals + methods      │  (USB device thread) │
│                     │                           │                      │
│  ┌───────────────┐  │                           │  ┌────────────────┐  │
│  │ PipeWire      │  │                           │  │ Device thread  │  │
│  │ main loop     │  │                           │  │ (poll/render)  │  │
│  └───────────────┘  │                           │  └────────────────┘  │
└─────────────────────┘                           └──────────────────────┘
        ▲                                                   ▲
        │ D-Bus                                             │ USB
        ▼                                                   ▼
  mixctl-cli                                        Beacn Mix Create
  mixctl-tui                                        (4 dials, buttons,
  mixctl-tauri                                       800x480 LCD, LEDs)
```

The mixer daemon and device daemon are separate processes:
- **mixctl-daemon** owns PipeWire audio routing and exposes the D-Bus API
- **mixctl-beacn-daemon** owns USB hardware and acts as a D-Bus client
- D-Bus round-trip latency is ~0.3ms — invisible in the 50ms device poll loop

This separation means device daemons can be added for other hardware (MIDI controllers, Stream Deck, etc.) without touching the mixer daemon.

## How it works

### Input/output mixer model

mixctl models audio routing as an **input/output matrix**, inspired by the Beacn Mix hardware:

- **Inputs** are virtual PipeWire sinks — audio destinations that apps play into (e.g., "System", "Game", "Music", "Chat")
- **Outputs** are virtual PipeWire sources — mixed audio that programs can capture from (e.g., "Personal Mix", "Voice Chat Mix", "Audience Mix")
- **Routes** connect inputs to outputs with per-route volume and mute, implemented as `libpipewire-module-loopback` instances

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

### Mixer daemon

The daemon runs two threads:
- **Tokio thread**: D-Bus service (`dev.greghuber.MixCtl`), config/state management, app rule matching
- **PipeWire thread**: Dedicated OS thread running `pipewire::MainLoop` with virtual sinks, sources, loopbacks, and metadata

Commands flow tokio → PW via `pipewire::channel`. Events flow PW → tokio via `tokio::mpsc`. Reconnection is automatic with exponential backoff. Graceful shutdown restores the original PipeWire default sink and stream targets.

See [daemon/README.md](daemon/README.md) for full architectural details.

### Hardware integration

The Beacn Mix Create has 4 rotary dials, 11 buttons, an 800x480 LCD, and RGB LEDs. The `mixctl-beacn-daemon` connects this hardware to the mixer:

1. Subscribes to D-Bus signals from the mixer daemon
2. Builds `DisplayState` snapshots and renders them as JPEG frames
3. Sends frames to the device LCD via USB
4. Polls for dial/button input at 50ms intervals
5. Translates hardware input into D-Bus method calls (set_route_volume, set_route_mute, etc.)
6. Updates LEDs to reflect input colors, mute states, and page navigation

Three display layouts are available:
- **column** (default) — 4 vertical sliders with rotated channel names
- **dial** — 4 arc dials mimicking the native Beacn app
- **grid** — 2x2 horizontal slider grid

### D-Bus interface

Bus name: `dev.greghuber.MixCtl`, object path: `/dev/greghuber/MixCtl1`

Key methods: `list_inputs`, `list_outputs`, `set_route_volume`, `set_route_mute`, `set_current_page`, `assign_stream`, `get_config_section`, `set_config_section`, `get_broadcast_levels`, `set_broadcast_levels`, etc.

Key signals: `inputs_config_changed`, `route_changed`, `output_state_changed`, `page_changed`, `input_levels_changed`, `config_section_changed`, etc.

### Level monitoring

When `broadcast_levels` is enabled, the daemon emits `input_levels_changed` signals at ~20Hz with per-input audio levels (0.0–1.0). Device daemons use this for real-time level indicators on hardware displays, applying exponential decay between updates.

See [crates/core/README.md](crates/core/README.md) for the full interface specification.

## Config and state

**Config** (`~/.config/mixctl.toml`) — channel definitions, app rules, component config sections:
```toml
version = 1
default_input = 1
broadcast_levels = false

[[inputs]]
id = 1
name = "System"
color = "#4A90D9"

[[outputs]]
id = 5
name = "Personal Mix"
color = "#8E44AD"
target_device = "alsa_output.pci-0000_00_1f.3.analog-stereo"

[[app_rules]]
app_name = "firefox"
input_id = 1

[beacn]
layout = "column"
dial_sensitivity = 2
level_decay = 0.8

[ui]
window_width = 750
window_height = 450
margin = 12

[applet]
window_width = 380
poll_interval_ms = 30

[cli]
color_output = true
output_format = "text"
```

Component config sections (`[beacn]`, `[ui]`, `[applet]`, `[cli]`) are optional — missing sections use defaults. Consumers fetch their config over D-Bus via `get_config_section`/`set_config_section`, never reading the TOML file directly.

**State** (`~/.local/state/mixctl.toml`) — runtime values (volumes, mutes, page):
```toml
version = 1
current_page = 0

[outputs.5]
volume = 100
muted = false

[routes."1:5"]
volume = 80
muted = false
```

## CLI usage

```bash
# Status
mixctl-cli ping
mixctl-cli status

# Inputs/outputs
mixctl-cli input list
mixctl-cli output set-volume <id> 85
mixctl-cli route set-volume <input_id> <output_id> 75

# Streams and rules
mixctl-cli stream list
mixctl-cli stream assign <pw_node_id> <input_id> --remember
mixctl-cli rule set "spotify*" 3

# Config sections
mixctl-cli config get beacn
mixctl-cli config set beacn '{"layout":"dial"}'

# Profiles
mixctl-cli profile save my-setup
mixctl-cli profile load my-setup

# Live monitoring
mixctl-cli listen all
```

See [apps/cli/README.md](apps/cli/README.md) for the full command reference.

## Running

### Mixer daemon + hardware controller

```bash
# Terminal 1: mixer daemon
cargo run -p mixctl-daemon

# Terminal 2: beacn hardware daemon
cargo run -p mixctl-beacn-daemon          # default column layout
cargo run -p mixctl-beacn-daemon -- dial  # dial layout

# Terminal 3: Desktop UI
cd apps/tauri && npm run tauri dev

# Terminal 4: TUI
cargo run -p mixctl-tui

# Terminal 5: CLI
cargo run -p mixctl-cli -- input list
```

### Hardware testing (no daemon needed)

```bash
# Test device with fake mixer state
./device-test.sh dial

# Low-level USB probe
cargo run -p mixctl-beacn-probe -- listen
cargo run -p mixctl-beacn-probe -- debug
```

### Docker (macOS / no PipeWire)

```bash
docker compose up daemon
docker compose run --rm cli cargo run -p mixctl-cli -- status
```

## Building

```bash
# Rust components (daemon, CLI, TUI, device daemons)
cargo build

# Tauri desktop app
cd apps/tauri && npm install && npm run tauri build
```

The daemon requires `libpipewire-0.3-dev` and `libclang-dev`. The CLI, TUI, and device daemons build without PipeWire. The Tauri app requires Node.js and system WebView libraries.

## Tests

```bash
cargo test                       # all tests
cargo test -p mixctl-beacn-display     # display rendering tests (10 tests)
cargo test -p mixctl-protocol    # USB protocol tests (28 tests)
cargo test -p mixctl-core        # color parsing tests
cargo test -p mixctl-daemon      # volume conversion tests
```

## License

MIT
