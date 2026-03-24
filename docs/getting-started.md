# Getting Started with mixctl

## What is mixctl?

mixctl is a PipeWire audio mixer for Linux. It creates virtual audio channels that let you route applications to different outputs (speakers, headphones, stream, recording) with independent volume and mute controls per route.

Think of it like a virtual mixing console: applications play into **inputs** (channels), and you control how much of each input goes to each **output** (mix).

## Mental Model

```
Apps (Spotify, Discord, Games)
        │
        ▼
┌─────────────────┐
│  Input Channels  │  "System", "Game", "Music", "Chat"
│  (virtual sinks) │  Apps are routed here automatically via rules
└────────┬────────┘
         │
    Volume Matrix    ← You control this (per input × output)
         │
┌─────────────────┐
│ Output Channels  │  "Personal Mix", "Stream Mix", "VOD Track"
│ (virtual sources)│  Each goes to a hardware device
└────────┬────────┘
         │
         ▼
   Hardware Devices   (Speakers, Headphones, Virtual Cable)
```

**Key concepts:**
- **Input** = a virtual sink that receives audio from apps
- **Output** = a virtual source that sends mixed audio to hardware
- **Route** = the volume/mute setting for a specific input→output pair
- **App Rule** = auto-assigns apps to inputs (e.g., "spotify" → Music channel)

## Installation

### From source (Cargo)

```bash
cargo install --path daemon
cargo install --path apps/cli
cargo install --path apps/tui
cargo install --path devices/beacn-daemon  # only if you have BEACN hardware

# Desktop UI (Tauri — requires Node.js)
cd apps/tauri && npm install && npm run tauri build
```

### systemd service

```bash
# Copy service files
cp dist/systemd/mixctl-daemon.service ~/.config/systemd/user/
cp dist/systemd/mixctl-beacn-daemon.service ~/.config/systemd/user/

# Enable and start
systemctl --user daemon-reload
systemctl --user enable --now mixctl-daemon

# For BEACN hardware:
sudo cp dist/udev/99-beacn-mix.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
systemctl --user enable --now mixctl-beacn-daemon
```

## First Run

1. Start the daemon: `mixctl-daemon` (or via systemd)
2. Open the TUI: `mixctl-tui`
3. You'll see your configured inputs and outputs with volume sliders
4. Use `Tab` to switch between panels, `hjkl` or arrow keys to navigate
5. `h`/`l` adjust volume, `m` toggles mute, `?` shows help

## Configuration

Config file: `~/.config/mixctl.toml`

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

[[inputs]]
id = 3
name = "Music"
color = "#2ECC71"

[[inputs]]
id = 4
name = "Chat"
color = "#F39C12"

[[outputs]]
id = 5
name = "Personal Mix"
color = "#8E44AD"
target_device = "alsa_output.usb-Blue_Microphones_Yeti-00.analog-stereo"

[[outputs]]
id = 6
name = "Stream Mix"
color = "#3498DB"

[[app_rules]]
app_name = "spotify"
input_id = 3

[[app_rules]]
app_name = "Discord"
input_id = 4
```

## Next Steps

- [Streaming Setup](streaming-setup.md) — configure for OBS + Discord
- [EQ and Processing](eq-and-processing.md) — per-channel EQ, compression, noise gate
- [Capture Devices](capture-devices.md) — binding microphones
- [Configuration Reference](configuration.md) — complete TOML reference
- [Troubleshooting](troubleshooting.md) — common issues and solutions
