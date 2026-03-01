use crate::enums::Button;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputEvent {
    pub dials: [i8; 4],
    pub buttons_pressed: Vec<Button>,
    pub button_mask: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputParseError {
    TooShort { len: usize },
}

impl core::fmt::Display for InputParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            InputParseError::TooShort { len } => {
                write!(f, "input buffer too short: {} bytes (need at least 10)", len)
            }
        }
    }
}

pub fn parse_input(buf: &[u8]) -> Result<InputEvent, InputParseError> {
    if buf.len() < 10 {
        return Err(InputParseError::TooShort { len: buf.len() });
    }

    let dials = [
        buf[4] as i8,
        buf[5] as i8,
        buf[6] as i8,
        buf[7] as i8,
    ];

    let button_mask = u16::from_be_bytes([buf[8], buf[9]]);

    let buttons_pressed = Button::ALL
        .iter()
        .filter(|b| b.is_pressed(button_mask))
        .copied()
        .collect();

    Ok(InputEvent {
        dials,
        buttons_pressed,
        button_mask,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_too_short() {
        let buf = [0u8; 5];
        assert_eq!(
            parse_input(&buf),
            Err(InputParseError::TooShort { len: 5 })
        );
    }

    #[test]
    fn parse_no_input() {
        let buf = [0u8; 10];
        let event = parse_input(&buf).unwrap();
        assert_eq!(event.dials, [0, 0, 0, 0]);
        assert!(event.buttons_pressed.is_empty());
        assert_eq!(event.button_mask, 0);
    }

    #[test]
    fn parse_dial_deltas() {
        let mut buf = [0u8; 10];
        buf[4] = 1;            // Dial1 +1
        buf[5] = 0xFF;         // Dial2 -1 (as i8)
        buf[6] = 3;            // Dial3 +3
        buf[7] = 0;            // Dial4 no change
        let event = parse_input(&buf).unwrap();
        assert_eq!(event.dials, [1, -1, 3, 0]);
    }

    #[test]
    fn parse_buttons() {
        let mut buf = [0u8; 10];
        // Set bits for AudienceMix (0) and Dial1 (8)
        let mask: u16 = (1 << 0) | (1 << 8);
        let be = mask.to_be_bytes();
        buf[8] = be[0];
        buf[9] = be[1];
        let event = parse_input(&buf).unwrap();
        assert_eq!(event.button_mask, mask);
        assert!(event.buttons_pressed.contains(&Button::AudienceMix));
        assert!(event.buttons_pressed.contains(&Button::Dial1));
        assert_eq!(event.buttons_pressed.len(), 2);
    }

    #[test]
    fn parse_longer_buffer_ok() {
        let buf = [0u8; 64];
        let event = parse_input(&buf).unwrap();
        assert_eq!(event.dials, [0, 0, 0, 0]);
        assert!(event.buttons_pressed.is_empty());
    }
}
