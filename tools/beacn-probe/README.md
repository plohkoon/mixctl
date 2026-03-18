# mixctl-beacn-probe

USB experimentation tool for the Beacn Mix and Mix Create. Talks directly to the device via `rusb` — no daemon needed.

## Usage

```bash
# Find devices
mixctl-beacn-probe discover

# Initialize and print version info
mixctl-beacn-probe init

# Read dial/button input
mixctl-beacn-probe poll          # single poll
mixctl-beacn-probe listen        # continuous (Ctrl-C to stop)

# Display control
mixctl-beacn-probe brightness 200
mixctl-beacn-probe display-power on

# LED control
mixctl-beacn-probe led-brightness 128
mixctl-beacn-probe led-color dial1 255 0 0        # red
mixctl-beacn-probe led-color mix 0 255 0 128      # green, half alpha

# Send raw bytes (hex)
mixctl-beacn-probe raw 0000000500000000

# Scan command space
mixctl-beacn-probe scan --b0 0x01 --b1-start 0x00 --b1-end 0xff

# Push image to display
mixctl-beacn-probe image photo.jpg --x 0 --y 0

# Interactive debug mode (HSLA/RGB color picker on dials)
mixctl-beacn-probe debug
```

LED zones: `dial1`, `dial2`, `dial3`, `dial4`, `mix`, `left`, `right`.

## Debug mode

The `debug` command turns the device into an interactive color picker:
- Page 1 (HSLA): dials control Hue, Saturation, Lightness, Alpha
- Page 2 (RGB+B): dials control Red, Green, Blue, Brightness
- PageLeft/Right to switch pages
- Display shows live slider bars, LEDs show the current color
- Ctrl-C for clean shutdown

## How it works

Uses `mixctl_beacn_device::usb::Device` for USB access. Opens the first Beacn Mix/Mix Create found, claims interface 0 (alternate setting 1), and sends/receives on interrupt endpoints. On Linux, detaches the kernel driver if necessary.

## Dependencies

- `mixctl-protocol` — command encoding, input parsing, image chunking
- `mixctl-beacn-device` — USB device access (`Device` struct)
- `rusb` — USB device access (for direct use in debug mode)
- `clap` — argument parsing
- `image` — JPEG encoding (for debug mode display rendering)
