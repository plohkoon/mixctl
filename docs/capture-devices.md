# Capture Devices (Microphone Binding)

## Overview

mixctl can bind hardware capture devices (microphones) to input channels. When bound, the microphone audio feeds into the channel alongside any routed application audio. This is useful for:

- Routing your microphone through the mixer for streaming
- Applying DSP (EQ, noise gate) to your mic before it reaches Discord
- Creating a monitor mix (hearing yourself in headphones)

## Listing Available Devices

```bash
mixctl-cli capture list
```

Shows all detected capture devices (microphones, line inputs, webcam mics):
```
[818] Yeti Stereo Microphone (alsa_input.usb-Blue_Microphones_Yeti...) - available
[885] Live Streamer CAM 313 (alsa_input.usb-Sunplus...) - available
[63]  Starship Analog Stereo (alsa_input.pci-0000...) - available
```

## Binding a Microphone to a Channel

```bash
# Bind Yeti to the Chat input (id=4)
mixctl-cli capture bind 4 "alsa_input.usb-Blue_Microphones_Yeti_Stereo_Microphone_REV8-00.analog-stereo"
```

The microphone will appear in the Streams panel with a `[mic]` prefix:
```
[mic] Yeti Stereo Microphone → Chat
```

## Capture Volume and Mute

```bash
# Set capture volume (0.0 to 1.0)
mixctl-cli capture set-volume 4 0.8

# Mute capture
mixctl-cli capture set-mute 4 true
```

## Unbinding

```bash
mixctl-cli capture remove 4
```

## Monitor Mix (Hear Yourself)

To hear your microphone in your headphones:

1. Bind the microphone to an input (e.g., Chat)
2. Ensure that input routes to your headphone output with non-zero volume
3. The microphone audio flows through the mixer and out your headphones

**Warning:** If your headphone output feeds back to your microphone (e.g., speakers instead of headphones), this will create a feedback loop. mixctl will warn you when this is detected.

## Noise Suppression

Enable AI-powered noise suppression on a capture input to remove background noise:

```bash
mixctl-cli capture noise-suppression enable 4
```

This uses RNNoise (via nnnoiseless) to filter out keyboard, fan, and ambient noise while preserving voice. Runs at ~60x real-time speed with negligible CPU cost.

```bash
# Disable noise suppression
mixctl-cli capture noise-suppression disable 4
```
