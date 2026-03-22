# Streaming Setup Guide

## Typical Streaming Configuration

A common streaming setup uses 4 inputs and 3-4 outputs:

### Inputs (audio sources)
| Input | Purpose | App Rules |
|-------|---------|-----------|
| System | Desktop audio, browser | Default for all apps |
| Game | Game audio | `Factorio*`, `Steam*` |
| Music | Background music | `spotify`, `Strawberry` |
| Chat | Voice chat | `Discord`, `TeamSpeak` |

### Outputs (destinations)
| Output | Target Device | Purpose |
|--------|---------------|---------|
| Personal Mix | Headphones (SDAC) | What you hear |
| Stream Mix | Virtual cable / OBS | What viewers hear |
| VOD Track | Virtual cable / OBS | DMCA-safe recording (no music) |

### Route Matrix

|          | Personal | Stream | VOD |
|----------|----------|--------|-----|
| System   | 100%     | 100%   | 100% |
| Game     | 100%     | 100%   | 100% |
| Music    | 80%      | 0% (muted) | 0% (muted) |
| Chat     | 100%     | 50%    | 50% |

Music is muted on Stream and VOD outputs to avoid DMCA issues.

## OBS Configuration

1. In OBS, add an **Audio Input Capture** source
2. Select `mixctl.output.6` (Stream Mix) as the device
3. For VOD recording, add a second Audio Input Capture with `mixctl.output.7`
4. Mute the default desktop audio capture in OBS (mixctl handles routing)

## Discord Setup

1. In Discord → Settings → Voice & Video
2. Set Output Device to `mixctl.input.4` (Chat channel)
3. Discord's audio will be routed through the Chat input
4. The app rule `Discord → Chat` auto-assigns it

## CLI Quick Setup

```bash
# Create the configuration
mixctl-cli input add "System" "#4A90D9"
mixctl-cli input add "Game" "#E74C3C"
mixctl-cli input add "Music" "#2ECC71"
mixctl-cli input add "Chat" "#F39C12"

mixctl-cli output add "Personal Mix" "#8E44AD"
mixctl-cli output add "Stream Mix" "#3498DB"
mixctl-cli output add "VOD Track" "#1ABC9C"

# Set output targets
mixctl-cli output set-target 5 "alsa_output.usb-Grace_Design_SDAC-00.analog-stereo"

# Set app rules
mixctl-cli rule set "spotify" 3
mixctl-cli rule set "Discord" 4
mixctl-cli rule set "Factorio*" 2

# Mute music on stream/VOD
mixctl-cli route set-mute 3 6 true
mixctl-cli route set-mute 3 7 true

# Lower chat on stream
mixctl-cli route set-volume 4 6 50
```
