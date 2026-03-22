use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;
use image::{ImageBuffer, Rgb};
use profont::{PROFONT_10_POINT, PROFONT_24_POINT};

use crate::layout::{DisplayLayout, Patch};
use crate::render::{
    self, ImageBufferTarget, encode_jpeg, format_streams, BG, BORDER, DIM_TEXT, DISPLAY_H,
    DISPLAY_W, FULL_QUALITY, LEVEL_COLOR, PAD, PATCH_QUALITY, TAB_BAR_H, TAB_BAR_Y, TEXT_COLOR,
    TRACK_COLOR,
};
use crate::types::{DisplayState, SlotView};

// Layout constants specific to grid2x2
const GRID_TOP: u32 = TAB_BAR_Y + TAB_BAR_H + 40;
const GRID_GAP_X: u32 = 20;
const GRID_GAP_Y: u32 = 12;
const CELL_W: u32 = (DISPLAY_W - 2 * PAD - GRID_GAP_X) / 2;
const BAR_H: u32 = 60;
const LABEL_H: u32 = 64;
const CELL_H: u32 = BAR_H + LABEL_H;
const BAR_INSET: u32 = 3;
const PAGE_INDICATOR_Y: u32 = 450;

/// Max tab width for this layout.
const MAX_TAB_W: i32 = 160;

pub struct Grid2x2Layout {
    cell_template: ImageBuffer<Rgb<u8>, Vec<u8>>,
}

impl Grid2x2Layout {
    pub fn new() -> Self {
        Self {
            cell_template: make_cell_template(),
        }
    }
}

impl Default for Grid2x2Layout {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns (x, y) of the top-left of cell i (0=top-left, 1=top-right, 2=bottom-left, 3=bottom-right)
fn cell_origin(i: u32) -> (u32, u32) {
    let col = i % 2;
    let row = i / 2;
    let x = PAD + col * (CELL_W + GRID_GAP_X);
    let y = GRID_TOP + row * (CELL_H + GRID_GAP_Y);
    (x, y)
}

fn make_cell_template() -> ImageBuffer<Rgb<u8>, Vec<u8>> {
    let mut img = ImageBuffer::new(CELL_W, CELL_H);

    for pixel in img.pixels_mut() {
        *pixel = BG;
    }

    // Bar track
    for y in 0..BAR_H {
        for x in 0..CELL_W {
            img.put_pixel(x, y, TRACK_COLOR);
        }
    }

    // Border
    for y in 0..BAR_H {
        img.put_pixel(0, y, BORDER);
        img.put_pixel(CELL_W - 1, y, BORDER);
    }
    for x in 0..CELL_W {
        img.put_pixel(x, 0, BORDER);
        img.put_pixel(x, BAR_H - 1, BORDER);
    }

    img
}

/// Draw cell content (bar fill, mute badge, label) at position (ox, oy) on any ImageBuffer.
/// The template must already be blitted at (ox, oy) before calling this.
fn draw_cell_content(
    img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>,
    slot: &SlotView,
    ox: u32,
    oy: u32,
) {
    let bar_inner = CELL_W - 2 * BAR_INSET;
    let fill_w = (slot.volume as f32 / 100.0 * bar_inner as f32) as u32;

    // Fill bar
    for x in 0..fill_w.min(bar_inner) {
        let frac = x as f32 / bar_inner as f32;
        let b = if slot.route_muted {
            0.3 + 0.4 * frac
        } else {
            0.4 + 0.6 * frac
        };
        let color = render::slot_fill_color(slot, b);
        for y in BAR_INSET..BAR_H - BAR_INSET {
            let px = ox + x + BAR_INSET;
            let py = oy + y;
            if px < img.width() && py < img.height() {
                img.put_pixel(px, py, color);
            }
        }
    }

    // Level indicator: bright vertical line at peak position on horizontal bar
    if let Some(level) = slot.level {
        let level_clamped = level.clamp(0.0, 1.0);
        let level_x = BAR_INSET + (level_clamped * bar_inner as f32) as u32;
        for thickness in 0..3u32 {
            let px = ox + level_x + BAR_INSET + thickness;
            if px < ox + CELL_W - BAR_INSET {
                for y in BAR_INSET..BAR_H - BAR_INSET {
                    let py = oy + y;
                    if px < img.width() && py < img.height() {
                        img.put_pixel(px, py, LEVEL_COLOR);
                    }
                }
            }
        }
    }

    // Mute badge in top-right of bar
    if slot.global_muted || slot.route_muted {
        let badge_x = ox + CELL_W - 34;
        let badge_y = oy + 5;
        render::draw_mute_badge(img, badge_x, badge_y, slot.global_muted);
    }

    // Label: input name + volume
    let label = format!("{} {}%", slot.name, slot.volume);
    let text_color = if slot.global_muted || slot.route_muted {
        DIM_TEXT
    } else {
        TEXT_COLOR
    };

    let mut target = ImageBufferTarget {
        img: std::mem::take(img),
    };
    let style = MonoTextStyle::new(
        &PROFONT_24_POINT,
        embedded_graphics::pixelcolor::Rgb888::new(text_color.0[0], text_color.0[1], text_color.0[2]),
    );
    let _ = Text::new(
        &label,
        Point::new((ox + 8) as i32, (oy + BAR_H + 32) as i32),
        style,
    )
    .draw(&mut target);

    // Stream names below the label
    let streams_text = format_streams(&slot.streams, 50);
    if !streams_text.is_empty() {
        let stream_style = MonoTextStyle::new(
            &PROFONT_10_POINT,
            embedded_graphics::pixelcolor::Rgb888::new(DIM_TEXT.0[0], DIM_TEXT.0[1], DIM_TEXT.0[2]),
        );
        let _ = Text::new(
            &streams_text,
            Point::new((ox + 8) as i32, (oy + BAR_H + 50) as i32),
            stream_style,
        )
        .draw(&mut target);
    }

    *img = target.img;
}

fn render_cell(
    template: &ImageBuffer<Rgb<u8>, Vec<u8>>,
    slot: &SlotView,
    quality: u8,
) -> Vec<u8> {
    let mut img = template.clone();
    draw_cell_content(&mut img, slot, 0, 0);
    encode_jpeg(&img, quality)
}

impl DisplayLayout for Grid2x2Layout {
    fn render_full(&self, state: &DisplayState) -> Vec<u8> {
        let mut target = ImageBufferTarget::new_with_color(DISPLAY_W, DISPLAY_H, BG);

        // Header + tab bar
        render::render_header_onto(&mut target.img, state, MAX_TAB_W);

        // Render cells
        for i in 0..4u32 {
            if let Some(slot) = &state.visible_inputs[i as usize] {
                let (cx, cy) = cell_origin(i);
                // Blit template
                for y in 0..CELL_H.min(self.cell_template.height()) {
                    for x in 0..CELL_W.min(self.cell_template.width()) {
                        let px = cx + x;
                        let py = cy + y;
                        if px < target.img.width() && py < target.img.height() {
                            target.img.put_pixel(px, py, *self.cell_template.get_pixel(x, y));
                        }
                    }
                }
                draw_cell_content(&mut target.img, slot, cx, cy);
            }
        }

        // Page indicator
        if state.total_pages > 1 {
            let text = format!("{}/{}", state.page + 1, state.total_pages);
            let style = MonoTextStyle::new(
                &profont::PROFONT_18_POINT,
                embedded_graphics::pixelcolor::Rgb888::new(DIM_TEXT.0[0], DIM_TEXT.0[1], DIM_TEXT.0[2]),
            );
            let x = (DISPLAY_W / 2 - 30) as i32;
            let _ = Text::new(&text, Point::new(x, PAGE_INDICATOR_Y as i32 + 16), style)
                .draw(&mut target);
        }

        encode_jpeg(&target.img, FULL_QUALITY)
    }

    fn render_diff(&self, prev: &DisplayState, next: &DisplayState) -> Vec<Patch> {
        let mut patches = Vec::new();

        // Check if header/tabs changed
        let header_changed = prev.current_output_index != next.current_output_index
            || prev.outputs != next.outputs;

        if header_changed {
            let (jpeg, x, y) = render::render_header(next, MAX_TAB_W);
            patches.push(Patch { jpeg, x, y });
        }

        // Check each cell slot
        for i in 0..4usize {
            if prev.visible_inputs[i] != next.visible_inputs[i] {
                if let Some(slot) = &next.visible_inputs[i] {
                    let jpeg = render_cell(&self.cell_template, slot, PATCH_QUALITY);
                    let (cx, cy) = cell_origin(i as u32);
                    patches.push(Patch { jpeg, x: cx, y: cy });
                } else {
                    // Empty slot — render blank cell
                    let jpeg = encode_jpeg(&self.cell_template, PATCH_QUALITY);
                    let (cx, cy) = cell_origin(i as u32);
                    patches.push(Patch { jpeg, x: cx, y: cy });
                }
            }
        }

        // Check page indicator
        if prev.page != next.page || prev.total_pages != next.total_pages {
            let (jpeg, x, y) = render::render_page_indicator(next);
            patches.push(Patch { jpeg, x, y });
        }

        patches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::test_display_state;

    #[test]
    fn render_full_produces_jpeg() {
        let layout = Grid2x2Layout::new();
        let state = test_display_state();
        let jpeg = layout.render_full(&state);
        assert!(!jpeg.is_empty());
        // JPEG magic bytes
        assert_eq!(jpeg[0], 0xFF);
        assert_eq!(jpeg[1], 0xD8);
    }

    #[test]
    fn render_diff_no_changes() {
        let layout = Grid2x2Layout::new();
        let state = test_display_state();
        let patches = layout.render_diff(&state, &state);
        assert!(patches.is_empty());
    }

    #[test]
    fn render_diff_volume_change() {
        let layout = Grid2x2Layout::new();
        let prev = test_display_state();
        let mut next = test_display_state();
        next.visible_inputs[0].as_mut().unwrap().volume = 50;
        let patches = layout.render_diff(&prev, &next);
        assert_eq!(patches.len(), 1);
        // Should be at cell 0 position
        let (cx, cy) = cell_origin(0);
        assert_eq!(patches[0].x, cx);
        assert_eq!(patches[0].y, cy);
    }

    #[test]
    fn render_diff_output_switch() {
        let layout = Grid2x2Layout::new();
        let prev = test_display_state();
        let mut next = test_display_state();
        next.current_output_index = 1;
        next.outputs[0].is_current = false;
        next.outputs[1].is_current = true;
        let patches = layout.render_diff(&prev, &next);
        // Should have header patch
        assert!(patches.iter().any(|p| p.y == PAD));
    }
}
