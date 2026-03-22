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
