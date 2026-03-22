# Configuration Reference

## File Locations

| File | Purpose |
|------|---------|
| `~/.config/mixctl.toml` | Configuration (inputs, outputs, rules, DSP settings) |
| `~/.local/state/mixctl.toml` | Runtime state (volumes, mutes, current page) |

## Config File Format

```toml
version = 1
default_input = 1

# Input channels — apps play audio into these
[[inputs]]
id = 1
name = "System"
color = "#4A90D9"
# capture_device = "alsa_input.usb-..." # optional: bind a microphone

[[inputs]]
id = 2
name = "Game"
color = "#E74C3C"

# Output channels — mixed audio goes to hardware
[[outputs]]
id = 5
name = "Personal Mix"
color = "#8E44AD"
target_device = "alsa_output.usb-Blue_Microphones_Yeti-00.analog-stereo"

[[outputs]]
id = 6
name = "Stream Mix"
color = "#3498DB"

# App rules — auto-assign apps to inputs
[[app_rules]]
app_name = "spotify"     # exact match
input_id = 3

[[app_rules]]
app_name = "Factorio*"   # glob pattern
input_id = 2

# BEACN Mix Create hardware config
[beacn]
layout = "dial"              # "column", "grid", or "dial"
dial_sensitivity = 2         # volume change per dial tick (1-10)
level_decay = 0.8            # VU meter decay rate (0.0-1.0)
display_brightness = 40      # LCD brightness (0-255)
led_brightness = 255         # Button LED brightness (0-255)

[beacn.button_mappings]
dial1_press = "toggle_route_mute"
dial2_press = "toggle_route_mute"
dial3_press = "toggle_route_mute"
dial4_press = "toggle_route_mute"
audience1 = "toggle_global_mute"
audience2 = "toggle_global_mute"
audience3 = "toggle_global_mute"
audience4 = "toggle_global_mute"
mix = "next_output"
page_left = "page_left"
page_right = "page_right"

# Button action options: toggle_route_mute, toggle_global_mute,
#   next_output, prev_output, page_left, page_right, none
```

## State File Format

Managed automatically by the daemon. Do not edit while daemon is running.

```toml
version = 1
current_page = 0

[outputs.5]
volume = 100
muted = false

[routes."1:5"]
volume = 80
muted = false

[routes."3:6"]
volume = 100
muted = true

[capture_volumes]
# Per-capture volume/mute state
```

## Color Format

Colors are specified as `#RRGGBB` hex strings:
- `#4A90D9` = steel blue
- `#E74C3C` = red
- `#2ECC71` = green
- `#F39C12` = orange
- `#8E44AD` = purple

## Target Device Names

Find device names with:
```bash
mixctl-cli playback list
```

Or:
```bash
pw-cli list-objects Node | grep node.name
```

Device names look like: `alsa_output.usb-Blue_Microphones_Yeti_Stereo_Microphone_REV8-00.analog-stereo`
