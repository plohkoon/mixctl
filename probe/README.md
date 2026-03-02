# mixctl-probe

USB experimentation tool for reverse-engineering the Beacn Mix and Mix Create. Talks directly to the device via `rusb` — no daemon needed.

## Usage

```bash
# Find devices
mixctl-probe discover

# Initialize and print version info
mixctl-probe init

# Read dial/button input
mixctl-probe poll          # single poll
mixctl-probe listen        # continuous (Ctrl-C to stop)

# Display control
mixctl-probe brightness 200
mixctl-probe display-power on

# LED control
mixctl-probe led-brightness 128
mixctl-probe led-color dial1 255 0 0        # red
mixctl-probe led-color mix 0 255 0 128      # green, half alpha

# Send raw bytes (hex)
mixctl-probe raw 0000000500000000

# Push image to display
mixctl-probe image photo.raw --x 0 --y 0
```

LED zones: `dial1`, `dial2`, `dial3`, `dial4`, `mix`, `left`, `right`.

## How it works

Opens the first Beacn Mix/Mix Create found on USB, claims interface 0 (alternate setting 1), and sends/receives on the interrupt endpoints. On Linux, detaches the kernel driver if necessary.

## Dependencies

- `mixctl-protocol` — command encoding, input parsing, image chunking
- `rusb` — USB device access
- `clap` — argument parsing
- `hex` — raw byte I/O
