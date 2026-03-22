use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Alignment, Text, TextStyleBuilder};
use image::{ImageBuffer, Rgb};
use profont::{PROFONT_9_POINT, PROFONT_18_POINT, PROFONT_24_POINT};

use crate::layout::{DisplayLayout, Patch};
use crate::render::{
    self, ImageBufferTarget, encode_jpeg, format_streams, BG, DIM_TEXT, DISPLAY_H, DISPLAY_W,
    FULL_QUALITY, LEVEL_COLOR, MUTED_COLOR, PAD, PATCH_QUALITY, TAB_BAR_H, TAB_BAR_Y, TEXT_COLOR,
    TRACK_COLOR,
};
use crate::types::{DisplayState, SlotView};

// Layout constants specific to dial4
const GRID_TOP: u32 = TAB_BAR_Y + TAB_BAR_H + 20;
const NUM_COLS: u32 = 4;
const COL_W: u32 = (DISPLAY_W - 2 * PAD) / NUM_COLS;

// Dial geometry
const DIAL_RADIUS: u32 = 80;
const DIAL_CX_OFFSET: u32 = COL_W / 2;
const ARC_WIDTH: u32 = 15;
const NAME_BELOW_DIAL: u32 = 30;

// Arc angles
const ARC_START_RAD: f32 = std::f32::consts::PI;

// Vertical centering
const PAGE_INDICATOR_Y: u32 = DISPLAY_H - 30;
const STREAMS_BELOW_NAME: u32 = 16;
const DIAL_CELL_H: u32 = DIAL_RADIUS * 2 + NAME_BELOW_DIAL + 24 + STREAMS_BELOW_NAME;
const AVAIL_H: u32 = PAGE_INDICATOR_Y - GRID_TOP;
const DIAL_TOP_PAD: u32 = (AVAIL_H - DIAL_CELL_H) / 2;
const DIAL_CY: u32 = GRID_TOP + DIAL_TOP_PAD + DIAL_RADIUS;
const NAME_Y: u32 = DIAL_CY + DIAL_RADIUS + NAME_BELOW_DIAL;

/// Max tab width for this layout.
const MAX_TAB_W: i32 = 200;

pub struct Dial4Layout;

impl Dial4Layout {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Dial4Layout {
    fn default() -> Self {
        Self::new()
    }
}

fn col_x(i: u32) -> u32 {
    PAD + i * COL_W
}

/// Convert a pixel's atan2 angle to a 0.0..1.0 fraction along the arc.
/// Returns None if the angle is in the dead zone (the bottom gap).
fn angle_to_frac(angle: f32) -> Option<f32> {
    let two_pi = 2.0 * std::f32::consts::PI;
    let shifted = angle - ARC_START_RAD;
    let norm = ((shifted % two_pi) + two_pi) % two_pi;
    let cw = (two_pi - norm) % two_pi;
    let sweep = 225.0_f32.to_radians();
    if cw <= sweep {
        Some(cw / sweep)
    } else {
        None
    }
}

/// Draw a thick arc on an image.
fn draw_arc(
    img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>,
    cx: i32,
    cy: i32,
    radius: f32,
    width: u32,
    frac_start: f32,
    frac_end: f32,
    color: Rgb<u8>,
) {
    let r_outer = radius + width as f32 / 2.0;
    let r_inner = radius - width as f32 / 2.0;

    let scan = (r_outer + 1.0) as i32;
    for dy in -scan..=scan {
        for dx in -scan..=scan {
            let dist = ((dx * dx + dy * dy) as f32).sqrt();
            if dist < r_inner || dist > r_outer {
                continue;
            }
            let angle = (-(dy as f32)).atan2(dx as f32);
            if let Some(frac) = angle_to_frac(angle) {
                if frac >= frac_start && frac <= frac_end {
                    let px = (cx + dx) as u32;
                    let py = (cy + dy) as u32;
                    if px < img.width() && py < img.height() {
                        img.put_pixel(px, py, color);
                    }
                }
            }
        }
    }
}

/// Compute the fill color for a dial arc.
fn dial_fill_color(slot: &SlotView) -> Rgb<u8> {
    if slot.global_muted {
        Rgb([120, 30, 30])
    } else if slot.route_muted {
        Rgb([60, 60, 60])
    } else {
        let (r, g, b) = slot.color;
        Rgb([r, g, b])
    }
}

/// Draw dial content (arc, center text, name) at the given center position.
fn draw_dial_content(
    img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>,
    cx: i32,
    cy: i32,
    name_y: i32,
    slot: &SlotView,
) {
    let radius = DIAL_RADIUS as f32;

    // Track (full arc)
    draw_arc(img, cx, cy, radius, ARC_WIDTH, 0.0, 1.0, TRACK_COLOR);

    // Fill arc based on volume
    if slot.volume > 0 {
        let vol_frac = slot.volume as f32 / 100.0;
        let fill_color = dial_fill_color(slot);
        draw_arc(img, cx, cy, radius, ARC_WIDTH, 0.0, vol_frac, fill_color);
    }

    // Level indicator: bright tick on the arc at peak position
    if let Some(level) = slot.level {
        let level_clamped = level.clamp(0.0, 1.0);
        if level_clamped > 0.01 {
            let tick_half = 0.015; // ~3 degrees of arc
            let frac_start = (level_clamped - tick_half).max(0.0);
            let frac_end = (level_clamped + tick_half).min(1.0);
            draw_arc(img, cx, cy, radius, ARC_WIDTH + 4, frac_start, frac_end, LEVEL_COLOR);
        }
    }

    // Center text: percentage or mute indicator
    let text_color = if slot.global_muted || slot.route_muted {
        DIM_TEXT
    } else {
        TEXT_COLOR
    };

    let mut target = ImageBufferTarget {
        img: std::mem::take(img),
    };
    let text_style = TextStyleBuilder::new().alignment(Alignment::Center).build();

    if slot.global_muted || slot.route_muted {
        let badge = if slot.global_muted { "X" } else { "M" };
        let style = MonoTextStyle::new(
            &PROFONT_24_POINT,
            embedded_graphics::pixelcolor::Rgb888::new(
                MUTED_COLOR.0[0],
                MUTED_COLOR.0[1],
                MUTED_COLOR.0[2],
            ),
        );
        let _ = Text::with_text_style(badge, Point::new(cx, cy - 14), style, text_style)
            .draw(&mut target);

        let pct = format!("{}%", slot.volume);
        let pct_style = MonoTextStyle::new(
            &PROFONT_18_POINT,
            embedded_graphics::pixelcolor::Rgb888::new(
                text_color.0[0],
                text_color.0[1],
                text_color.0[2],
            ),
        );
        let _ = Text::with_text_style(&pct, Point::new(cx, cy + 14), pct_style, text_style)
            .draw(&mut target);
    } else {
        let pct = format!("{}%", slot.volume);
        let style = MonoTextStyle::new(
            &PROFONT_24_POINT,
            embedded_graphics::pixelcolor::Rgb888::new(
                text_color.0[0],
                text_color.0[1],
                text_color.0[2],
            ),
        );
        let _ = Text::with_text_style(
            &pct,
            Point::new(cx, cy + (PROFONT_24_POINT.baseline / 2) as i32),
            style,
            text_style,
        )
        .draw(&mut target);
    }

    // Channel name below dial
    let name_style = MonoTextStyle::new(
        &PROFONT_18_POINT,
        embedded_graphics::pixelcolor::Rgb888::new(
            text_color.0[0],
            text_color.0[1],
            text_color.0[2],
        ),
    );
    let _ = Text::with_text_style(&slot.name, Point::new(cx, name_y), name_style, text_style)
        .draw(&mut target);

    // Stream names centered below channel name
    let streams_text = format_streams(&slot.streams, 28);
    if !streams_text.is_empty() {
        let stream_style = MonoTextStyle::new(
            &PROFONT_9_POINT,
            embedded_graphics::pixelcolor::Rgb888::new(
                DIM_TEXT.0[0],
                DIM_TEXT.0[1],
                DIM_TEXT.0[2],
            ),
        );
        let _ = Text::with_text_style(
            &streams_text,
            Point::new(cx, name_y + STREAMS_BELOW_NAME as i32),
            stream_style,
            text_style,
        )
        .draw(&mut target);
    }

    *img = target.img;
}

fn render_dial_patch(slot: &SlotView, quality: u8) -> Vec<u8> {
    let patch_h = NAME_Y + 24 + STREAMS_BELOW_NAME - GRID_TOP;
    let mut img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(COL_W, patch_h);
    for pixel in img.pixels_mut() {
        *pixel = BG;
    }

    let cx = DIAL_CX_OFFSET as i32;
    let cy = (DIAL_CY - GRID_TOP) as i32;
    let name_y = (NAME_Y - GRID_TOP) as i32;

    draw_dial_content(&mut img, cx, cy, name_y, slot);

    encode_jpeg(&img, quality)
}

impl DisplayLayout for Dial4Layout {
    fn render_full(&self, state: &DisplayState) -> Vec<u8> {
        let mut target = ImageBufferTarget::new_with_color(DISPLAY_W, DISPLAY_H, BG);

        // Header + tab bar
        render::render_header_onto(&mut target.img, state, MAX_TAB_W);

        // Render dials
        for i in 0..4u32 {
            if let Some(slot) = &state.visible_inputs[i as usize] {
                let cx = (col_x(i) + DIAL_CX_OFFSET) as i32;
                let cy = DIAL_CY as i32;
                let name_y = NAME_Y as i32;
                draw_dial_content(&mut target.img, cx, cy, name_y, slot);
            }
        }

        // Page indicator
        if state.total_pages > 1 {
            let text = format!("{}/{}", state.page + 1, state.total_pages);
            let style = MonoTextStyle::new(
                &PROFONT_18_POINT,
                embedded_graphics::pixelcolor::Rgb888::new(
                    DIM_TEXT.0[0],
                    DIM_TEXT.0[1],
                    DIM_TEXT.0[2],
                ),
            );
            let x = (DISPLAY_W / 2 - 30) as i32;
            let _ = Text::new(
                &text,
                Point::new(x, (DISPLAY_H - 10) as i32),
                style,
            )
            .draw(&mut target);
        }

        encode_jpeg(&target.img, FULL_QUALITY)
    }

    fn render_diff(&self, prev: &DisplayState, next: &DisplayState) -> Vec<Patch> {
        let mut patches = Vec::new();

        let header_changed =
            prev.current_output_index != next.current_output_index || prev.outputs != next.outputs;
        if header_changed {
            let (jpeg, x, y) = render::render_header(next, MAX_TAB_W);
            patches.push(Patch { jpeg, x, y });
        }

        for i in 0..4usize {
            if prev.visible_inputs[i] != next.visible_inputs[i] {
                let cx = col_x(i as u32);
                if let Some(slot) = &next.visible_inputs[i] {
                    let jpeg = render_dial_patch(slot, PATCH_QUALITY);
                    patches.push(Patch {
                        jpeg,
                        x: cx,
                        y: GRID_TOP,
                    });
                } else {
                    let patch_h = NAME_Y + 24 + STREAMS_BELOW_NAME - GRID_TOP;
                    let mut blank = ImageBuffer::new(COL_W, patch_h);
                    for pixel in blank.pixels_mut() {
                        *pixel = BG;
                    }
                    let jpeg = encode_jpeg(&blank, PATCH_QUALITY);
                    patches.push(Patch {
                        jpeg,
                        x: cx,
                        y: GRID_TOP,
                    });
                }
            }
        }

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
        let layout = Dial4Layout::new();
        let state = test_display_state();
        let jpeg = layout.render_full(&state);
        assert!(!jpeg.is_empty());
        assert_eq!(jpeg[0], 0xFF);
        assert_eq!(jpeg[1], 0xD8);
    }

    #[test]
    fn render_diff_no_changes() {
        let layout = Dial4Layout::new();
        let state = test_display_state();
        let patches = layout.render_diff(&state, &state);
        assert!(patches.is_empty());
    }

    #[test]
    fn render_diff_volume_change() {
        let layout = Dial4Layout::new();
        let prev = test_display_state();
        let mut next = test_display_state();
        next.visible_inputs[1].as_mut().unwrap().volume = 30;
        let patches = layout.render_diff(&prev, &next);
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].x, col_x(1));
        assert_eq!(patches[0].y, GRID_TOP);
    }
}
