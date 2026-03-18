# mixctl-beacn-test

Standalone test harness for the Beacn Mix Create device integration. Simulates the mixer daemon with fake state — no PipeWire, no D-Bus, no daemon required.

## Usage

```bash
# From workspace root
./device-test.sh grid      # 2x2 grid layout
./device-test.sh column    # 4 vertical sliders (default)
./device-test.sh dial      # 4 arc dials

# Or directly
cargo run -p mixctl-beacn-test -- dial
```

Ctrl-C to stop (cleanly turns off display and LEDs).

## What it does

- Creates 6 fake inputs (System, Game, Music, Chat, Browser, Discord) across 2 pages
- Creates 3 fake outputs (Personal, Stream, VOD)
- Spawns the real USB device thread with full display rendering
- Handles all device events locally (volume adjustments, mute toggles, output/page switching)
- Simulates audio level indicators (when level monitoring would be enabled)
- Prints every state change to the terminal

## Controls

| Input | Action |
|-------|--------|
| Dials 1-4 | Adjust volume (current output) |
| Dial press | Toggle route mute (current output) |
| Audience 1-4 | Toggle global mute (all outputs) |
| AudienceMix | Next output tab |
| PageLeft/Right | Switch input page |

## Dependencies

- `mixctl-beacn-device` — USB device thread
- `mixctl-beacn-display` — display layout rendering
