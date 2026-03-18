use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;
use image::{ImageBuffer, Rgb};
use profont::{PROFONT_18_POINT, PROFONT_24_POINT};

use crate::layout::{DisplayLayout, Patch};
use crate::render::{
    self, ImageBufferTarget, encode_jpeg, BG, BORDER, DIM_TEXT, DISPLAY_H, DISPLAY_W,
    FULL_QUALITY, LEVEL_COLOR, PAD, PATCH_QUALITY, TAB_BAR_H, TAB_BAR_Y, TEXT_COLOR, TRACK_COLOR,
};
use crate::types::{DisplayState, SlotView};

// Layout constants specific to column4
const GRID_TOP: u32 = TAB_BAR_Y + TAB_BAR_H + 40;
const NUM_COLS: u32 = 4;
const COL_W: u32 = (DISPLAY_W - 2 * PAD) / NUM_COLS;
const BAR_W: u32 = COL_W / 2;
const BAR_X_OFFSET: u32 = 10;
const BAR_INSET: u32 = 3;
const GRID_BOTTOM: u32 = DISPLAY_H - PAD - 30;
const BAR_H: u32 = GRID_BOTTOM - GRID_TOP - 36;

/// Max tab width for this layout.
const MAX_TAB_W: i32 = 200;

pub struct Column4Layout {
    col_template: ImageBuffer<Rgb<u8>, Vec<u8>>,
}

impl Column4Layout {
    pub fn new() -> Self {
        Self {
            col_template: make_col_template(),
        }
    }
}

impl Default for Column4Layout {
    fn default() -> Self {
        Self::new()
    }
}

fn col_x(i: u32) -> u32 {
    PAD + i * COL_W
}

fn make_col_template() -> ImageBuffer<Rgb<u8>, Vec<u8>> {
    let h = BAR_H + 36;
    let mut img = ImageBuffer::new(COL_W, h);

    for pixel in img.pixels_mut() {
        *pixel = BG;
    }

    // Bar track
    for y in 0..BAR_H {
        for x in BAR_X_OFFSET..BAR_X_OFFSET + BAR_W {
            img.put_pixel(x, y, TRACK_COLOR);
        }
    }

    // Border
    for y in 0..BAR_H {
        img.put_pixel(BAR_X_OFFSET, y, BORDER);
        img.put_pixel(BAR_X_OFFSET + BAR_W - 1, y, BORDER);
    }
    for x in BAR_X_OFFSET..BAR_X_OFFSET + BAR_W {
        img.put_pixel(x, 0, BORDER);
        img.put_pixel(x, BAR_H - 1, BORDER);
    }

    img
}

/// Render text into a horizontal buffer, then rotate 90deg CW and blit onto target.
/// The rotated text reads top-to-bottom. Bottom of rotated text aligns at (dst_x, dst_bottom_y).
fn draw_rotated_text(
    img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>,
    text: &str,
    font: &embedded_graphics::mono_font::MonoFont,
    color: Rgb<u8>,
    dst_x: u32,
    dst_bottom_y: u32,
) {
    let char_w = font.character_size.width;
    let char_h = font.character_size.height;
    let text_w = text.len() as u32 * char_w + 4;
    let text_h = char_h + 4;

    let mut tmp = ImageBufferTarget::new(text_w, text_h);
    let style = MonoTextStyle::new(
        font,
        embedded_graphics::pixelcolor::Rgb888::new(color.0[0], color.0[1], color.0[2]),
    );
    let _ = Text::new(text, Point::new(0, char_h as i32), style).draw(&mut tmp);

    let rot_h = text_w;
    let dst_top_y = dst_bottom_y.saturating_sub(rot_h);

    for sy in 0..text_h {
        for sx in 0..text_w {
            let pixel = tmp.img.get_pixel(sx, sy);
            if pixel.0[0] == 0 && pixel.0[1] == 0 && pixel.0[2] == 0 {
                continue;
            }
            let rx = sy;
            let ry = text_w - 1 - sx;
            let px = dst_x + rx;
            let py = dst_top_y + ry;
            if px < img.width() && py < img.height() {
                img.put_pixel(px, py, *pixel);
            }
        }
    }
}

/// Draw column content (bar fill, mute badge, rotated name, percentage) at position (ox, oy)
/// on any ImageBuffer. The template must already be blitted at (ox, oy) before calling this.
fn draw_col_content(
    img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>,
    slot: &SlotView,
    ox: u32,
    oy: u32,
) {
    let bar_inner_h = BAR_H - 2 * BAR_INSET;
    let bar_inner_w = BAR_W - 2 * BAR_INSET;
    let fill_h = (slot.volume as f32 / 100.0 * bar_inner_h as f32) as u32;
    let fill_start_y = BAR_INSET + (bar_inner_h - fill_h);

    // Fill bar from bottom up
    for y in fill_start_y..BAR_INSET + bar_inner_h {
        let frac = (BAR_INSET + bar_inner_h - y) as f32 / bar_inner_h as f32;
        let b = if slot.route_muted {
            0.3 + 0.4 * frac
        } else {
            0.4 + 0.6 * frac
        };
        let color = render::slot_fill_color(slot, b);

        for x in (BAR_X_OFFSET + BAR_INSET)..(BAR_X_OFFSET + BAR_INSET + bar_inner_w) {
            let px = ox + x;
            let py = oy + y;
            if px < img.width() && py < img.height() {
                img.put_pixel(px, py, color);
            }
        }
    }

    // Level indicator: bright horizontal line at peak position
    if let Some(level) = slot.level {
        let level_clamped = level.clamp(0.0, 1.0);
        let level_y = BAR_INSET + ((1.0 - level_clamped) * bar_inner_h as f32) as u32;
        for thickness in 0..3u32 {
            let py = oy + level_y + thickness;
            if py < oy + BAR_H - BAR_INSET {
                for x in (BAR_X_OFFSET + BAR_INSET)..(BAR_X_OFFSET + BAR_INSET + bar_inner_w) {
                    let px = ox + x;
                    if px < img.width() && py < img.height() {
                        img.put_pixel(px, py, LEVEL_COLOR);
                    }
                }
            }
        }
    }

    // Mute badge
    if slot.global_muted || slot.route_muted {
        let badge_x = ox + BAR_X_OFFSET + (BAR_W - 28) / 2;
        let badge_y = oy + 5;
        render::draw_mute_badge(img, badge_x, badge_y, slot.global_muted);
    }

    // Rotated channel name
    let text_color = if slot.global_muted || slot.route_muted {
        DIM_TEXT
    } else {
        TEXT_COLOR
    };
    let name_x = ox + BAR_X_OFFSET + BAR_W + 6;
    let bar_bottom_y = oy + BAR_H;
    draw_rotated_text(img, &slot.name, &PROFONT_18_POINT, text_color, name_x, bar_bottom_y);

    // Percent below bar (right-justified under the bar)
    {
        let pct = format!("{}%", slot.volume);
        let char_w = PROFONT_24_POINT.character_size.width as i32;
        let text_px_w = pct.len() as i32 * char_w;
        let pct_x = (ox + BAR_X_OFFSET + BAR_W) as i32 - text_px_w;
        let mut target = ImageBufferTarget {
            img: std::mem::take(img),
        };
        let style = MonoTextStyle::new(
            &PROFONT_24_POINT,
            embedded_graphics::pixelcolor::Rgb888::new(
                text_color.0[0],
                text_color.0[1],
                text_color.0[2],
            ),
        );
        let _ = Text::new(
            &pct,
            Point::new(pct_x, (oy + BAR_H + 28) as i32),
            style,
        )
        .draw(&mut target);
        *img = target.img;
    }
}

fn render_col(
    template: &ImageBuffer<Rgb<u8>, Vec<u8>>,
    slot: &SlotView,
    quality: u8,
) -> Vec<u8> {
    let mut img = template.clone();
    draw_col_content(&mut img, slot, 0, 0);
    encode_jpeg(&img, quality)
}

impl DisplayLayout for Column4Layout {
    fn render_full(&self, state: &DisplayState) -> Vec<u8> {
        let mut target = ImageBufferTarget::new_with_color(DISPLAY_W, DISPLAY_H, BG);

        // Header + tab bar
        render::render_header_onto(&mut target.img, state, MAX_TAB_W);

        // Render columns
        for i in 0..4u32 {
            if let Some(slot) = &state.visible_inputs[i as usize] {
                let cx = col_x(i);
                let cy = GRID_TOP;
                let tmpl_h = self.col_template.height();

                // Blit template
                for y in 0..tmpl_h {
                    for x in 0..COL_W.min(self.col_template.width()) {
                        let px = cx + x;
                        let py = cy + y;
                        if px < target.img.width() && py < target.img.height() {
                            target.img.put_pixel(px, py, *self.col_template.get_pixel(x, y));
                        }
                    }
                }
                draw_col_content(&mut target.img, slot, cx, cy);
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
                    let jpeg = render_col(&self.col_template, slot, PATCH_QUALITY);
                    patches.push(Patch {
                        jpeg,
                        x: cx,
                        y: GRID_TOP,
                    });
                } else {
                    let jpeg = encode_jpeg(&self.col_template, PATCH_QUALITY);
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
        let layout = Column4Layout::new();
        let state = test_display_state();
        let jpeg = layout.render_full(&state);
        assert!(!jpeg.is_empty());
        assert_eq!(jpeg[0], 0xFF);
        assert_eq!(jpeg[1], 0xD8);
    }

    #[test]
    fn render_diff_no_changes() {
        let layout = Column4Layout::new();
        let state = test_display_state();
        let patches = layout.render_diff(&state, &state);
        assert!(patches.is_empty());
    }

    #[test]
    fn render_diff_volume_change() {
        let layout = Column4Layout::new();
        let prev = test_display_state();
        let mut next = test_display_state();
        next.visible_inputs[1].as_mut().unwrap().volume = 30;
        let patches = layout.render_diff(&prev, &next);
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].x, col_x(1));
        assert_eq!(patches[0].y, GRID_TOP);
    }
}
