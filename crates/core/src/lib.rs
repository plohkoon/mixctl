use serde::{Deserialize, Serialize};
use zvariant::Type;
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
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RouteInfo {
    pub input_id: u32,
    pub output_id: u32,
    pub volume: u8,
    pub muted: bool,
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
