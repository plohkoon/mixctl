use mixctl_core::EqBandInfo;
use slint::{Model, SharedPixelBuffer};

/// Render the EQ frequency response curve into a Slint Image.
///
/// The image shows:
/// - A dark background
/// - Horizontal grid lines at key dB values (0, +/-6, +/-12, +/-18)
/// - Vertical grid lines at key frequencies (50, 100, 200, 500, 1k, 2k, 5k, 10k Hz)
/// - The computed EQ response curve in cyan with a filled region toward 0dB
/// - Yellow dot markers at each band's (frequency, gain) position
pub fn render_eq_curve(bands: &[EqBandInfo], width: u32, height: u32) -> slint::Image {
    let mut buf = SharedPixelBuffer::<slint::Rgba8Pixel>::new(width, height);
    let pixels = buf.make_mut_bytes();

    // Fill background (dark)
    for chunk in pixels.chunks_exact_mut(4) {
        chunk[0] = 20;
        chunk[1] = 20;
        chunk[2] = 25;
        chunk[3] = 255;
    }

    let log_min = 20.0_f64.log10();
    let log_range = 20000.0_f64.log10() - log_min;
    let w = width as f64;
    let h = height as f64;

    // Draw horizontal grid lines at key dB values
    for &db in &[-18.0_f64, -12.0, -6.0, 0.0, 6.0, 12.0, 18.0] {
        let y = db_to_y(db, h);
        let (r, g, b) = if db == 0.0 {
            (70u8, 70, 75)
        } else {
            (35, 35, 40)
        };
        for x in 0..width as i32 {
            set_pixel(pixels, width, x, y, r, g, b);
        }
    }

    // Draw vertical grid lines at key frequencies
    for &freq in &[50.0_f64, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0] {
        let x = freq_to_x(freq, log_min, log_range, w);
        for y in 0..height as i32 {
            set_pixel(pixels, width, x, y, 35, 35, 40);
        }
    }

    // Compute the EQ curve
    let curve = mixctl_core::compute_eq_curve(bands);

    // Draw filled region between curve and 0dB center line first (behind the curve line)
    let y_center = (h / 2.0) as i32;
    for i in 1..curve.len() {
        let (f0, db0) = curve[i - 1];
        let (f1, db1) = curve[i];
        let x0 = freq_to_x(f0, log_min, log_range, w);
        let x1 = freq_to_x(f1, log_min, log_range, w);
        let y0 = db_to_y(db0, h);
        let y1 = db_to_y(db1, h);

        let dx = (x1 - x0).abs().max(1);
        for step in 0..=dx {
            let t = step as f64 / dx as f64;
            let x = (x0 as f64 + t * (x1 - x0) as f64) as i32;
            let y_curve = (y0 as f64 + t * (y1 - y0) as f64) as i32;
            let (y_start, y_end) = if y_curve < y_center {
                (y_curve, y_center)
            } else {
                (y_center, y_curve)
            };
            for y in y_start..y_end {
                blend_pixel(pixels, width, x, y, 0, 100, 140, 80);
            }
        }
    }

    // Draw the EQ curve line (3px thick, cyan)
    for i in 1..curve.len() {
        let (f0, db0) = curve[i - 1];
        let (f1, db1) = curve[i];
        let x0 = freq_to_x(f0, log_min, log_range, w);
        let x1 = freq_to_x(f1, log_min, log_range, w);
        let y0 = db_to_y(db0, h);
        let y1 = db_to_y(db1, h);

        let dx = (x1 - x0).abs().max(1);
        for step in 0..=dx {
            let t = step as f64 / dx as f64;
            let x = (x0 as f64 + t * (x1 - x0) as f64) as i32;
            let y = (y0 as f64 + t * (y1 - y0) as f64) as i32;
            // 3px thickness
            for dy in -1..=1 {
                set_pixel(pixels, width, x, y + dy, 0, 200, 255);
            }
        }
    }

    // Draw band marker dots (yellow circles, radius 4px)
    for band in bands {
        if band.band_type == "bypass" {
            continue;
        }
        let x = freq_to_x(band.frequency, log_min, log_range, w);
        let y = db_to_y(band.gain_db, h);
        for dy in -4..=4_i32 {
            for dx in -4..=4_i32 {
                if dx * dx + dy * dy <= 16 {
                    set_pixel(pixels, width, x + dx, y + dy, 255, 200, 50);
                }
            }
        }
    }

    slint::Image::from_rgba8_premultiplied(buf)
}

/// Map a frequency (Hz) to an x pixel coordinate using log scale.
fn freq_to_x(freq: f64, log_min: f64, log_range: f64, width: f64) -> i32 {
    ((freq.log10() - log_min) / log_range * width) as i32
}

/// Map a dB value to a y pixel coordinate.
/// 0dB is at center, +24dB at top, -24dB at bottom.
fn db_to_y(db: f64, height: f64) -> i32 {
    ((height / 2.0) - (db / 24.0 * height / 2.0)) as i32
}

/// Set a pixel at (x, y) to (r, g, b, 255) if within bounds.
fn set_pixel(pixels: &mut [u8], w: u32, x: i32, y: i32, r: u8, g: u8, b: u8) {
    if x >= 0 && y >= 0 && (x as u32) < w {
        let h = pixels.len() as u32 / 4 / w;
        if (y as u32) < h {
            let idx = (y as u32 * w + x as u32) as usize * 4;
            pixels[idx] = r;
            pixels[idx + 1] = g;
            pixels[idx + 2] = b;
            pixels[idx + 3] = 255;
        }
    }
}

/// Blend a pixel at (x, y) with the given color and alpha (0-255).
fn blend_pixel(pixels: &mut [u8], w: u32, x: i32, y: i32, r: u8, g: u8, b: u8, a: u8) {
    if x >= 0 && y >= 0 && (x as u32) < w {
        let h = pixels.len() as u32 / 4 / w;
        if (y as u32) < h {
            let idx = (y as u32 * w + x as u32) as usize * 4;
            let alpha = a as f32 / 255.0;
            let inv = 1.0 - alpha;
            // Premultiplied alpha blending
            pixels[idx] = (r as f32 * alpha + pixels[idx] as f32 * inv) as u8;
            pixels[idx + 1] = (g as f32 * alpha + pixels[idx + 1] as f32 * inv) as u8;
            pixels[idx + 2] = (b as f32 * alpha + pixels[idx + 2] as f32 * inv) as u8;
            pixels[idx + 3] = 255;
        }
    }
}

/// Helper to convert EqBandData (Slint struct) list to EqBandInfo vec for curve computation.
pub fn bands_from_dialog(dialog: &crate::DspDialog) -> Vec<EqBandInfo> {
    let eq_bands_model = dialog.get_eq_bands();
    let count = eq_bands_model.row_count();
    let mut bands = Vec::with_capacity(count);
    for i in 0..count {
        let bd = eq_bands_model.row_data(i).unwrap();
        let band_type = match bd.band_type_index {
            0 => "low_shelf",
            2 => "high_shelf",
            _ => "peaking",
        }
        .to_string();
        bands.push(EqBandInfo {
            band_type,
            frequency: bd.frequency as f64,
            gain_db: bd.gain_db as f64,
            q: bd.q as f64,
        });
    }
    bands
}

/// Re-render the EQ curve and set it on the dialog.
pub fn update_eq_curve_image(dialog: &crate::DspDialog) {
    let bands = bands_from_dialog(dialog);
    let image = render_eq_curve(&bands, 600, 200);
    dialog.set_eq_curve_image(image);
}
