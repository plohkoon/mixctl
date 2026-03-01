use crate::enums::{ButtonLighting, Color};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    DisplayBrightness(u8),
    DisplayPower(bool),
    ButtonLedBrightness(u8),
    ButtonLedColor { zone: ButtonLighting, color: Color },
    Wake,
    Poll,
}

impl Command {
    pub fn to_bytes(self) -> [u8; 8] {
        match self {
            Command::DisplayBrightness(v) => [0x00, 0x00, 0x00, 0x04, v, 0x00, 0x00, 0x00],
            Command::DisplayPower(on) => {
                [0x00, 0x01, 0x00, 0x04, if on { 0 } else { 1 }, 0x00, 0x00, 0x00]
            }
            Command::ButtonLedBrightness(v) => [0x01, 0x07, 0x00, 0x04, v, 0x00, 0x00, 0x00],
            Command::ButtonLedColor { zone, color } => {
                [0x01, zone as u8, 0x00, 0x04, color.b, color.g, color.r, color.a]
            }
            Command::Wake => [0x00, 0x00, 0x00, 0xF1, 0x00, 0x00, 0x00, 0x00],
            Command::Poll => [0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_brightness() {
        assert_eq!(
            Command::DisplayBrightness(128).to_bytes(),
            [0x00, 0x00, 0x00, 0x04, 128, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn display_power_on() {
        assert_eq!(
            Command::DisplayPower(true).to_bytes(),
            [0x00, 0x01, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn display_power_off() {
        assert_eq!(
            Command::DisplayPower(false).to_bytes(),
            [0x00, 0x01, 0x00, 0x04, 0x01, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn button_led_brightness() {
        assert_eq!(
            Command::ButtonLedBrightness(255).to_bytes(),
            [0x01, 0x07, 0x00, 0x04, 255, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn button_led_color() {
        let cmd = Command::ButtonLedColor {
            zone: ButtonLighting::Mix,
            color: Color {
                r: 0xFF,
                g: 0x80,
                b: 0x00,
                a: 0xFF,
            },
        };
        // Wire order is B, G, R, A
        assert_eq!(
            cmd.to_bytes(),
            [0x01, 0x04, 0x00, 0x04, 0x00, 0x80, 0xFF, 0xFF]
        );
    }

    #[test]
    fn wake() {
        assert_eq!(
            Command::Wake.to_bytes(),
            [0x00, 0x00, 0x00, 0xF1, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn poll() {
        assert_eq!(
            Command::Poll.to_bytes(),
            [0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00]
        );
    }
}
