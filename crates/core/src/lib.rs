use serde::{Deserialize, Serialize};
use zvariant::Type;
pub mod config_sections;
pub mod dbus;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct InputInfo {
    pub id: u32,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct OutputInfo {
    pub id: u32,
    pub name: String,
    pub color: String,
    pub volume: u8,
    pub muted: bool,
    pub target_device: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RouteInfo {
    pub input_id: u32,
    pub output_id: u32,
    pub volume: u8,
    pub muted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct StreamInfo {
    pub pw_node_id: u32,
    pub app_name: String,
    pub media_name: String,
    pub input_id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AppRuleInfo {
    pub app_name: String,
    pub input_id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CaptureDeviceInfo {
    pub pw_node_id: u32,
    pub name: String,
    pub device_name: String,
    pub is_added: bool,
    pub input_id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct PlaybackDeviceInfo {
    pub pw_node_id: u32,
    pub name: String,
    pub device_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ComponentInfo {
    pub bus_name: String,
    pub component_type: String,
}

/// A registered device adapter with its capabilities.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DeviceInfo {
    pub bus_name: String,
    pub device_name: String,
    /// JSON-serialized capabilities array
    pub capabilities_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CustomInputInfo {
    pub id: u32,
    pub name: String,
    pub color: String,
    pub custom_type: String,
    pub value: u8,
}

// ---------------------------------------------------------------------------
// DSP types (D-Bus serializable)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct EqBandInfo {
    pub band_type: String,
    pub frequency: f64,
    pub gain_db: f64,
    pub q: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct GateInfo {
    pub enabled: bool,
    pub threshold_db: f64,
    pub attack_ms: f64,
    pub release_ms: f64,
    pub hold_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DeesserInfo {
    pub enabled: bool,
    pub frequency: f64,
    pub threshold_db: f64,
    pub ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CompressorInfo {
    pub enabled: bool,
    pub threshold_db: f64,
    pub ratio: f64,
    pub attack_ms: f64,
    pub release_ms: f64,
    pub makeup_gain_db: f64,
    pub knee_db: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct LimiterInfo {
    pub enabled: bool,
    pub ceiling_db: f64,
    pub release_ms: f64,
}

// ---------------------------------------------------------------------------
// EQ frequency response computation (shared between UIs)
// ---------------------------------------------------------------------------

/// Number of points for EQ frequency response curves.
pub const EQ_CURVE_POINTS: usize = 150;

/// Compute the combined EQ frequency response from band info.
/// Returns (frequency_hz, magnitude_db) pairs on a log scale, 20Hz–20kHz.
pub fn compute_eq_curve(bands: &[EqBandInfo]) -> Vec<(f64, f64)> {
    let sample_rate = 48000.0_f64;
    let log_min = 20.0_f64.ln();
    let log_max = 20000.0_f64.ln();

    (0..EQ_CURVE_POINTS)
        .map(|i| {
            let t = i as f64 / (EQ_CURVE_POINTS - 1) as f64;
            let freq = (log_min + t * (log_max - log_min)).exp();
            let w = 2.0 * std::f64::consts::PI * freq / sample_rate;
            let (sin_w, cos_w) = w.sin_cos();
            let (sin_2w, cos_2w) = (2.0 * w).sin_cos();

            let mut total_db = 0.0_f64;
            for band in bands {
                let coeffs = eq_band_coeffs(
                    &band.band_type,
                    band.frequency as f32,
                    band.gain_db as f32,
                    band.q as f32,
                    sample_rate as f32,
                );
                // H(z) magnitude at z = e^(jw)
                let num_re = coeffs.0 + coeffs.1 * cos_w as f32 + coeffs.2 * cos_2w as f32;
                let num_im = -(coeffs.1 * sin_w as f32 + coeffs.2 * sin_2w as f32);
                let den_re = 1.0 + coeffs.3 * cos_w as f32 + coeffs.4 * cos_2w as f32;
                let den_im = -(coeffs.3 * sin_w as f32 + coeffs.4 * sin_2w as f32);
                let num_sq = num_re * num_re + num_im * num_im;
                let den_sq = den_re * den_re + den_im * den_im;
                if den_sq > 1e-20 {
                    total_db += 10.0 * (num_sq / den_sq).log10() as f64;
                }
            }
            (freq, total_db.clamp(-30.0, 30.0))
        })
        .collect()
}

/// Compute biquad coefficients (b0, b1, b2, a1, a2) for an EQ band.
fn eq_band_coeffs(band_type: &str, freq: f32, gain_db: f32, q: f32, sr: f32) -> (f32, f32, f32, f32, f32) {
    let a = 10.0_f32.powf(gain_db / 40.0);
    let w0 = 2.0 * std::f32::consts::PI * freq / sr;
    let (sin_w0, cos_w0) = w0.sin_cos();
    let alpha = sin_w0 / (2.0 * q);

    match band_type {
        "low_shelf" => {
            let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
            let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
            (
                (a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha)) / a0,
                (2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0)) / a0,
                (a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha)) / a0,
                (-2.0 * ((a - 1.0) + (a + 1.0) * cos_w0)) / a0,
                ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha) / a0,
            )
        }
        "high_shelf" => {
            let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
            let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
            (
                (a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha)) / a0,
                (-2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0)) / a0,
                (a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha)) / a0,
                (2.0 * ((a - 1.0) - (a + 1.0) * cos_w0)) / a0,
                ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha) / a0,
            )
        }
        _ => {
            // Peaking (default)
            let a0 = 1.0 + alpha / a;
            (
                (1.0 + alpha * a) / a0,
                (-2.0 * cos_w0) / a0,
                (1.0 - alpha * a) / a0,
                (-2.0 * cos_w0) / a0,
                (1.0 - alpha / a) / a0,
            )
        }
    }
}

/// Parse a "#RRGGBB" hex color string into (R, G, B) components.
pub fn parse_hex_color(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some((r, g, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_colors() {
        assert_eq!(parse_hex_color("#000000"), Some((0, 0, 0)));
        assert_eq!(parse_hex_color("#FFFFFF"), Some((255, 255, 255)));
        assert_eq!(parse_hex_color("#4A90D9"), Some((74, 144, 217)));
        assert_eq!(parse_hex_color("#ff00ff"), Some((255, 0, 255)));
    }

    #[test]
    fn parse_invalid_colors() {
        assert_eq!(parse_hex_color(""), None);
        assert_eq!(parse_hex_color("#"), None);
        assert_eq!(parse_hex_color("#FFF"), None);
        assert_eq!(parse_hex_color("#GGGGGG"), None);
        assert_eq!(parse_hex_color("4A90D9"), None);
        assert_eq!(parse_hex_color("#4A90D9FF"), None);
    }
}
