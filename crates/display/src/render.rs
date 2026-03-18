use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;
use image::{ImageBuffer, Rgb, codecs::jpeg::JpegEncoder};
use profont::{PROFONT_14_POINT, PROFONT_18_POINT, PROFONT_24_POINT};

use crate::types::{DisplayState, SlotView};

// ── Shared display constants ───────────────────────────────────────────────

pub(crate) const DISPLAY_W: u32 = 800;
pub(crate) const DISPLAY_H: u32 = 480;
pub(crate) const PAD: u32 = 20;
pub(crate) const HEADER_H: u32 = 48;
pub(crate) const TAB_BAR_H: u32 = 36;
pub(crate) const TAB_BAR_Y: u32 = PAD + HEADER_H + 4;

// Colors
pub(crate) const BG: Rgb<u8> = Rgb([15, 15, 20]);
pub(crate) const TRACK_COLOR: Rgb<u8> = Rgb([30, 30, 35]);
pub(crate) const BORDER: Rgb<u8> = Rgb([60, 60, 65]);
pub(crate) const TEXT_COLOR: Rgb<u8> = Rgb([200, 200, 200]);
pub(crate) const DIM_TEXT: Rgb<u8> = Rgb([100, 100, 100]);
pub(crate) const MUTED_COLOR: Rgb<u8> = Rgb([180, 50, 50]);

// JPEG quality
pub(crate) const FULL_QUALITY: u8 = 80;
pub(crate) const PATCH_QUALITY: u8 = 50;

// ── JPEG encoding ──────────────────────────────────────────────────────────

/// Encode an ImageBuffer as JPEG with the given quality (0-100).
pub fn encode_jpeg(img: &ImageBuffer<Rgb<u8>, Vec<u8>>, quality: u8) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut buf, quality);
    encoder
        .encode(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgb8,
        )
        .expect("jpeg encode failed");
    buf
}

// ── ImageBufferTarget (embedded-graphics DrawTarget) ───────────────────────

/// Wrapper around ImageBuffer that implements embedded_graphics::DrawTarget.
pub struct ImageBufferTarget {
    pub img: ImageBuffer<Rgb<u8>, Vec<u8>>,
}

impl ImageBufferTarget {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            img: ImageBuffer::new(width, height),
        }
    }

    pub fn new_with_color(width: u32, height: u32, color: Rgb<u8>) -> Self {
        let mut img = ImageBuffer::new(width, height);
        for pixel in img.pixels_mut() {
            *pixel = color;
        }
        Self { img }
    }
}

impl embedded_graphics::geometry::OriginDimensions for ImageBufferTarget {
    fn size(&self) -> embedded_graphics::geometry::Size {
        embedded_graphics::geometry::Size::new(self.img.width(), self.img.height())
    }
}

impl embedded_graphics::draw_target::DrawTarget for ImageBufferTarget {
    type Color = embedded_graphics::pixelcolor::Rgb888;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::Pixel<Self::Color>>,
    {
        use embedded_graphics::pixelcolor::RgbColor;

        for embedded_graphics::Pixel(coord, color) in pixels {
            let x = coord.x;
            let y = coord.y;
            if x >= 0 && y >= 0 && (x as u32) < self.img.width() && (y as u32) < self.img.height()
            {
                self.img
                    .put_pixel(x as u32, y as u32, Rgb([color.r(), color.g(), color.b()]));
            }
        }
        Ok(())
    }
}

// ── Shared render_header ───────────────────────────────────────────────────

/// Render the header (output name + tab bar) as a JPEG patch.
/// `max_tab_w` controls the maximum tab width (grid2x2 uses 160, others 200).
pub(crate) fn render_header(state: &DisplayState, max_tab_w: i32) -> (Vec<u8>, u32, u32) {
    let header_total_h = HEADER_H + 4 + TAB_BAR_H;
    let mut target = ImageBufferTarget::new_with_color(DISPLAY_W - 2 * PAD, header_total_h, BG);

    render_header_onto_target(&mut target, state, max_tab_w);

    let jpeg = encode_jpeg(&target.img, PATCH_QUALITY);
    (jpeg, PAD, PAD)
}

/// Render the header directly onto a full-frame image at position (PAD, PAD).
/// Avoids the JPEG encode/decode roundtrip used by `render_header`.
pub(crate) fn render_header_onto(
    img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>,
    state: &DisplayState,
    max_tab_w: i32,
) {
    let header_total_h = HEADER_H + 4 + TAB_BAR_H;
    let header_w = DISPLAY_W - 2 * PAD;
    let mut header_target = ImageBufferTarget::new_with_color(header_w, header_total_h, BG);

    render_header_onto_target(&mut header_target, state, max_tab_w);

    // Blit header onto main image at (PAD, PAD)
    for y in 0..header_total_h {
        for x in 0..header_w {
            let pixel = *header_target.img.get_pixel(x, y);
            img.put_pixel(x + PAD, y + PAD, pixel);
        }
    }
}

/// Internal: draw header content onto an ImageBufferTarget (header-sized).
fn render_header_onto_target(
    target: &mut ImageBufferTarget,
    state: &DisplayState,
    max_tab_w: i32,
) {
    // Output name
    if let Some(tab) = state.outputs.get(state.current_output_index) {
        let style = MonoTextStyle::new(
            &PROFONT_24_POINT,
            embedded_graphics::pixelcolor::Rgb888::new(tab.color.0, tab.color.1, tab.color.2),
        );
        let _ = Text::new(&tab.name, Point::new(4, 32), style).draw(target);
    }

    // Tab bar
    let tab_count = state.outputs.len();
    if tab_count > 0 {
        let tab_bar_width = (DISPLAY_W - 2 * PAD) as i32;
        let tab_w = (tab_bar_width / tab_count as i32).min(max_tab_w);
        let tab_y = (HEADER_H + 4) as i32;

        for (i, tab) in state.outputs.iter().enumerate() {
            let tx = i as i32 * tab_w;

            let (br, bg, bb) = if tab.is_current {
                (tab.color.0, tab.color.1, tab.color.2)
            } else {
                (40u8, 40u8, 45u8)
            };

            for y in tab_y..tab_y + TAB_BAR_H as i32 {
                for x in tx..tx + tab_w - 2 {
                    if x >= 0
                        && (x as u32) < target.img.width()
                        && y >= 0
                        && (y as u32) < target.img.height()
                    {
                        target
                            .img
                            .put_pixel(x as u32, y as u32, Rgb([br, bg, bb]));
                    }
                }
            }

            let text_color = if tab.is_current {
                embedded_graphics::pixelcolor::Rgb888::new(0, 0, 0)
            } else {
                embedded_graphics::pixelcolor::Rgb888::new(150, 150, 150)
            };
            let style = MonoTextStyle::new(&PROFONT_14_POINT, text_color);
            let max_chars = (tab_w / 10).max(1) as usize;
            let display_name: String = tab.name.chars().take(max_chars).collect();
            let _ = Text::new(&display_name, Point::new(tx + 6, tab_y + 24), style)
                .draw(target);
        }
    }
}

// ── Shared render_page_indicator ───────────────────────────────────────────

/// Render the page indicator ("1/3") as a JPEG patch.
pub(crate) fn render_page_indicator(state: &DisplayState) -> (Vec<u8>, u32, u32) {
    let w = 140u32;
    let h = 30u32;
    let mut target = ImageBufferTarget::new_with_color(w, h, BG);

    let text = format!("{}/{}", state.page + 1, state.total_pages);
    let style = MonoTextStyle::new(
        &PROFONT_18_POINT,
        embedded_graphics::pixelcolor::Rgb888::new(DIM_TEXT.0[0], DIM_TEXT.0[1], DIM_TEXT.0[2]),
    );
    let _ = Text::new(&text, Point::new(4, 22), style).draw(&mut target);

    let x = (DISPLAY_W - w) / 2;
    let y = DISPLAY_H - 30;
    let jpeg = encode_jpeg(&target.img, PATCH_QUALITY);
    (jpeg, x, y)
}

// ── Shared slot_fill_color ─────────────────────────────────────────────────

/// Compute the fill color for a slot at the given brightness (0.0..1.0).
/// Global muted -> red tint, route muted -> gray, else slot color.
pub(crate) fn slot_fill_color(slot: &SlotView, brightness: f32) -> Rgb<u8> {
    if slot.global_muted {
        Rgb([
            (120.0 * brightness) as u8,
            (30.0 * brightness) as u8,
            (30.0 * brightness) as u8,
        ])
    } else if slot.route_muted {
        let v = (80.0 * brightness) as u8;
        Rgb([v, v, v])
    } else {
        let (cr, cg, cb) = slot.color;
        Rgb([
            (cr as f32 * brightness) as u8,
            (cg as f32 * brightness) as u8,
            (cb as f32 * brightness) as u8,
        ])
    }
}

// ── Level indicator color ──────────────────────────────────────────────────

pub(crate) const LEVEL_COLOR: Rgb<u8> = Rgb([255, 255, 255]);

// ── Shared draw_mute_badge ─────────────────────────────────────────────────

/// Draw a 28x28 red mute badge with centered "X" (global) or "M" (route) text.
pub(crate) fn draw_mute_badge(
    img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>,
    x: u32,
    y: u32,
    is_global: bool,
) {
    let badge_text = if is_global { "X" } else { "M" };

    // Badge background
    for by in y..y + 28 {
        for bx in x..x + 28 {
            if bx < img.width() && by < img.height() {
                img.put_pixel(bx, by, MUTED_COLOR);
            }
        }
    }

    // Badge text
    let mut target = ImageBufferTarget {
        img: std::mem::take(img),
    };
    let style = MonoTextStyle::new(
        &PROFONT_24_POINT,
        embedded_graphics::pixelcolor::Rgb888::WHITE,
    );
    let _ = Text::new(
        badge_text,
        Point::new(
            (x + (28 - PROFONT_24_POINT.character_size.width) / 2) as i32,
            (y + (28 - PROFONT_24_POINT.baseline) / 2 + PROFONT_24_POINT.baseline) as i32,
        ),
        style,
    )
    .draw(&mut target);
    *img = target.img;
}
