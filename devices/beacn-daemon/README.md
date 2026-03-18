# mixctl-beacn-daemon

Standalone daemon that connects Beacn Mix Create hardware to the mixctl mixer daemon. Runs as a separate process, communicating with the mixer over D-Bus.

## Architecture

```
mixctl-daemon (PipeWire + D-Bus)
     ↕ D-Bus session bus (~0.3ms round-trip)
mixctl-beacn-daemon
     ↕ USB interrupt transfers (~50ms poll)
Beacn Mix Create hardware
```

The beacn daemon is a **D-Bus client** — it subscribes to the mixer daemon's signals and calls its methods. This separation means:
- The mixer daemon has no knowledge of hardware controllers
- The beacn daemon can be restarted independently
- Multiple device daemons can run simultaneously (e.g., Beacn + MIDI controller)
- Device daemons can run on different machines (D-Bus over TCP)

## How it works

1. Connects to the mixer daemon's D-Bus interface (`dev.greghuber.MixCtl`)
2. Fetches initial state (inputs, outputs, routes, page) via D-Bus methods
3. Spawns a `DeviceThread` for USB communication
4. Subscribes to D-Bus signals to refresh state when changes occur externally (CLI, UI, other clients)
5. Handles device events (dial turns, button presses) by calling D-Bus methods on the mixer daemon
6. Uses **optimistic local updates** — updates the local state mirror and display immediately without waiting for the D-Bus signal roundtrip, keeping the hardware responsive

## Usage

```bash
# Default layout (from config, falls back to column)
mixctl-beacn-daemon

# Override layout via CLI arg
mixctl-beacn-daemon dial
mixctl-beacn-daemon grid
mixctl-beacn-daemon column
```

The mixer daemon (`mixctl-daemon`) must be running first.

## Configuration

On startup, the beacn daemon fetches its config from the mixer daemon via D-Bus (`get_config_section("beacn")`). CLI args override config values.

```toml
[beacn]
layout = "column"        # display layout (column, grid, dial)
dial_sensitivity = 2     # multiplier for dial delta
level_decay = 0.8        # exponential decay factor for level indicators
```

- `dial_sensitivity` controls how fast dials adjust volume (delta × sensitivity)
- `level_decay` controls how quickly level indicators fade between updates (multiplied per frame)
- Layout changes via `config_section_changed` signal log a warning — restart required to take effect
- `dial_sensitivity` and `level_decay` changes take effect immediately via the signal

## State management

`BeacnState` mirrors the mixer state locally:
- Inputs, outputs, routes fetched from D-Bus
- `current_output_index` — which output the device display shows (local to this daemon)
- `current_page` — which page of inputs is visible (synced via D-Bus)
- `input_levels` — real-time audio levels per input (0.0-1.0), decayed exponentially
- `dial_sensitivity` and `level_decay` — from config, updated on `config_section_changed`

On any D-Bus signal, the full state is refreshed from D-Bus and the display is updated. Level indicators update at ~20Hz when `broadcast_levels` is enabled.

## Dependencies

- `mixctl-core` — D-Bus proxy (`MixCtlProxy`), shared types
- `mixctl-beacn-device` — USB device thread
- `mixctl-beacn-display` — display layout rendering
- `serde` / `serde_json` — config section deserialization
- `zbus` — D-Bus client
- `futures-lite` — signal stream iteration
