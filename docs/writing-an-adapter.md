# Writing a Device Adapter for mixctl

This guide walks you through building a device adapter that connects your hardware
controller (USB mixer, MIDI controller, Stream Deck, etc.) to the mixctl audio
routing daemon.

## Prerequisites

- Rust toolchain (1.85+, edition 2024)
- A running mixctl daemon (`cargo run -p mixctl-daemon`)
- Your hardware device connected via USB/MIDI/HID

## Quick Start

The fastest way to get started is with the project template:

```bash
cargo install cargo-generate
cargo generate --path /path/to/mixctl/template/adapter
```

This scaffolds a complete adapter project with the SDK dependency, a stubbed
`DeviceAdapter` trait implementation, a systemd service template, and udev rules.

## Architecture

```
┌───────────────┐     D-Bus      ┌──────────────────┐
│ mixctl-daemon  │◄═════════════►│  Your Adapter     │
│ (PipeWire)     │  MixCtlProxy  │  (DeviceAdapter)  │
└───────────────┘               │                    │
                                │  ┌──────────────┐  │
                                │  │ AdapterRunner │  │
                                │  │ (SDK: D-Bus   │  │
                                │  │  lifecycle)   │  │
                                │  └──────────────┘  │
                                │  ┌──────────────┐  │
                                │  │ Your device   │  │
                                │  │ I/O thread    │  │
                                │  └──────────────┘  │
                                └────────────────────┘
```

Your adapter is a separate process that:
1. Connects to the mixer daemon over D-Bus (the SDK handles this)
2. Registers its hardware capabilities (faders, buttons, screens, etc.)
3. Listens for mixer state changes and updates the hardware display/LEDs
4. Reads hardware input (dial turns, button presses) and translates them
   into D-Bus method calls on the mixer daemon

## The DeviceAdapter Trait

```rust
use mixctl_adapter_sdk::*;
use tokio::sync::mpsc;

struct MyAdapter {
    // your device state here
}

impl DeviceAdapter for MyAdapter {
    fn capabilities(&self) -> Vec<Capability> {
        vec![
            Capability::Fader { count: 4, range: (0.0, 1.0) },
            Capability::Button { count: 8, kind: ButtonKind::Momentary },
        ]
    }

    fn device_name(&self) -> &str {
        "my-device"
    }

    async fn run(
        &mut self,
        proxy: MixCtlProxy<'static>,
        mut mixer_events: mpsc::UnboundedReceiver<MixerEvent>,
    ) -> anyhow::Result<()> {
        loop {
            tokio::select! {
                Some(event) = mixer_events.recv() => {
                    self.handle_mixer_event(event, &proxy).await;
                }
                // ... also select over your hardware I/O
            }
        }
    }

    async fn shutdown(&mut self) {
        // cleanup hardware resources
    }
}
```

### What the SDK does for you

- **D-Bus connection + reconnection:** If the mixer daemon restarts, the SDK
  reconnects and calls your `run()` method again with a fresh proxy.
- **Signal subscription:** All mixer daemon signals are delivered as `MixerEvent`
  variants through a channel. You match on what you care about.
- **Device registration:** Your capabilities are advertised to the mixer daemon
  so the UI and CLI can discover connected devices.

### What you implement

- **`capabilities()`**: Declare what your hardware has (faders, buttons, screens, etc.)
- **`device_name()`**: A unique name for your device type
- **`run()`**: Your main loop. Select over mixer events AND your hardware I/O.
  You have full access to the `MixCtlProxy` for calling mixer daemon methods.
- **`shutdown()`**: Cleanup when the adapter stops.

## MixerEvent Types

The SDK delivers specific event types so you can handle them efficiently:

| Event | When | Typical response |
|-------|------|-----------------|
| `InputsChanged` | Inputs added/removed/renamed | Refresh your display |
| `OutputsChanged` | Outputs added/removed/renamed | Refresh your display |
| `OutputStateChanged { id }` | Output volume/mute changed | Update display for that output |
| `RouteChanged { input_id, output_id }` | Route volume/mute changed | Update display for that route |
| `StreamsChanged` | App streams assigned/unassigned | Update display if showing streams |
| `LevelsChanged { levels }` | Audio levels (~20Hz) | Update VU meters |
| `BroadcastLevelsChanged { enabled }` | Level monitoring toggled | Start/stop meter display |
| `ConfigSectionChanged { section }` | Config updated | Re-fetch your device config |
| `CustomInputChanged { id }` | Custom input value changed | Update display |

Events you probably don't need: `AudioStatusChanged`, `ComponentChanged`,
`InputDspChanged`, `OutputDspChanged`, `ProfileChanged`.

## Calling Mixer Daemon Methods

Your `run()` receives a `MixCtlProxy` with the full mixer daemon API:

```rust
// Volume control
proxy.set_route_volume(input_id, output_id, new_volume).await?;
proxy.set_route_mute(input_id, output_id, muted).await?;

// Output control
proxy.set_output_volume(output_id, volume).await?;
proxy.set_output_mute(output_id, muted).await?;

// Read state
let inputs = proxy.list_inputs().await?;
let outputs = proxy.list_outputs().await?;
let route = proxy.get_route(input_id, output_id).await?;

// DSP toggles
proxy.set_input_eq_enabled(input_id, enabled).await?;
proxy.set_input_gate_enabled(input_id, enabled).await?;

// Profiles
proxy.load_profile("my-profile").await?;
```

See `crates/core/src/dbus.rs` for the full list of available methods and signals.

## Device I/O Patterns

### USB devices (channel-based)

Most USB devices run a dedicated I/O thread that pushes events through a channel:

```rust
use mixctl_adapter_sdk::channel_pair;

// Create channels for device communication
let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<MyDeviceCommand>();
let (event_tx, event_rx) = channel_pair::<MyDeviceEvent>();

// Spawn your device I/O thread
let device_thread = std::thread::spawn(move || {
    // Open USB device, poll for input, send events via event_tx
    // Receive commands via cmd_rx (display updates, LED changes, etc.)
});

// In your run() method, select over both channels:
tokio::select! {
    Some(event) = mixer_events.recv() => { /* mixer state change */ }
    Some(hw_event) = event_rx.recv() => { /* hardware input */ }
}
```

### MIDI devices

```rust
// Use the `midir` crate for MIDI input
let midi_in = midir::MidiInput::new("mixctl-adapter")?;
let port = midi_in.ports().first().unwrap().clone();
let (tx, rx) = channel_pair();

midi_in.connect(&port, "input", move |_, message, _| {
    // Parse MIDI CC messages and send via tx
    if message[0] & 0xF0 == 0xB0 {
        let cc = message[1];
        let value = message[2];
        tx.send(MidiEvent::ControlChange { cc, value }).ok();
    }
}, ())?;
```

## Systemd Service

Create a systemd user service for your adapter at
`~/.config/systemd/user/mixctl-YOURDEVICE-daemon.service`:

```ini
[Unit]
Description=MixCtl YOUR_DEVICE Device Daemon
After=mixctl-daemon.service
Wants=mixctl-daemon.service

[Service]
Type=simple
ExecStart=%h/.cargo/bin/mixctl-YOURDEVICE-daemon
Restart=on-failure
RestartSec=3

[Install]
WantedBy=default.target
```

Enable and start:

```bash
systemctl --user enable --now mixctl-YOURDEVICE-daemon
```

## udev Rules

If your device needs non-root USB access, create a udev rule at
`/etc/udev/rules.d/99-YOURDEVICE.rules`:

```
# YOUR_DEVICE - grant user access without root
SUBSYSTEM=="usb", ATTR{idVendor}=="XXXX", TAG+="uaccess"
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="XXXX", TAG+="uaccess"
```

Replace `XXXX` with your device's USB vendor ID. Reload rules:

```bash
sudo udevadm control --reload-rules && sudo udevadm trigger
```

## Reference Implementation

The Beacn Mix Create adapter at `devices/beacn-daemon/` is the canonical reference.
It demonstrates:

- USB device thread with push-based events (`crates/device/`)
- JPEG display rendering (`crates/display/`)
- Button hold detection state machine
- Config section handling
- All 18 device event types
- Graceful shutdown with drop guard

## Verifying Your Adapter

1. Start the mixer daemon: `cargo run -p mixctl-daemon`
2. Start your adapter: `cargo run -p mixctl-YOURDEVICE-daemon`
3. Check registration: `cargo run -p mixctl-cli -- adapter list`
4. You should see your device with its capabilities listed.

## Out-of-Tree Adapters

You can build your adapter as a standalone project that depends on the SDK:

```toml
[dependencies]
mixctl-adapter-sdk = { git = "https://github.com/plohkoon/mixctl" }
mixctl-core = { git = "https://github.com/plohkoon/mixctl" }
```
