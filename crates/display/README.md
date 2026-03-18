# mixctl-beacn-display

Rendering engine for hardware device displays. Converts mixer state into JPEG images for the Beacn Mix Create's 800x480 LCD.

## Architecture

The crate defines a `DisplayLayout` trait and provides three concrete layout implementations. Each layout takes a `DisplayState` snapshot and produces either a full-frame JPEG or incremental patches for changed regions.

```
DisplayState (snapshot of mixer state)
      │
      ▼
DisplayLayout::render_full()  → full 800x480 JPEG
DisplayLayout::render_diff()  → Vec<Patch> (JPEG + position)
```

## Layouts

### Column (`Column4Layout`) — default

Four vertical slider columns side-by-side. Each column has:
- Vertical fill bar (half column width, colored by input)
- Rotated channel name to the right of the bar, bottom-aligned
- Right-justified percentage below the bar
- Mute badge (red "X" for global, "M" for route) at top of bar

### Grid (`Grid2x2Layout`)

2x2 grid of horizontal sliders. Each cell has:
- Horizontal fill bar with color gradient
- Channel name + volume percentage below
- Mute badge in top-right corner

### Dial (`Dial4Layout`)

Four arc dials mimicking the native Beacn app. Each dial has:
- 225-degree arc sweep (180 to -45 degrees, gap at bottom-left)
- Track in dark gray, fill in input color
- Centered percentage (or mute indicator + percentage) inside the dial
- Channel name centered below

## Shared rendering

Common rendering logic lives in `render.rs`:
- `render_header()` / `render_header_onto()` — output name + tab bar
- `render_page_indicator()` — "1/3" page display
- `slot_fill_color()` — mute-aware color computation
- `draw_mute_badge()` — 28x28 badge with centered text
- `encode_jpeg()` — ImageBuffer to JPEG encoding
- `ImageBufferTarget` — `embedded_graphics::DrawTarget` adapter for `ImageBuffer`
- Shared constants (display dimensions, colors, quality levels)

## Types

- `DisplayState` — full snapshot: current output, all output tabs, 4 visible input slots, page info
- `OutputTab` — name, color, is_current
- `SlotView` — name, color, volume, route_muted, global_muted, level (optional `f32` for real-time audio level indicator)
- `Patch` — JPEG bytes + (x, y) position for incremental updates
- `DeviceLayoutKind` — enum (Column, Grid, Dial) with factory method

## Dependencies

- `image` — JPEG encoding
- `embedded-graphics` + `profont` — monospace text rendering onto pixel buffers
