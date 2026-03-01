#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Button {
    AudienceMix = 0,
    PageLeft = 1,
    PageRight = 2,
    Dial1 = 8,
    Dial2 = 9,
    Dial3 = 10,
    Dial4 = 11,
    Audience1 = 12,
    Audience2 = 13,
    Audience3 = 14,
    Audience4 = 15,
}

impl Button {
    pub const ALL: &[Button] = &[
        Button::AudienceMix,
        Button::PageLeft,
        Button::PageRight,
        Button::Dial1,
        Button::Dial2,
        Button::Dial3,
        Button::Dial4,
        Button::Audience1,
        Button::Audience2,
        Button::Audience3,
        Button::Audience4,
    ];

    pub fn is_pressed(self, mask: u16) -> bool {
        mask & (1 << self as u8) != 0
    }
}

impl TryFrom<u8> for Button {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Button::AudienceMix),
            1 => Ok(Button::PageLeft),
            2 => Ok(Button::PageRight),
            8 => Ok(Button::Dial1),
            9 => Ok(Button::Dial2),
            10 => Ok(Button::Dial3),
            11 => Ok(Button::Dial4),
            12 => Ok(Button::Audience1),
            13 => Ok(Button::Audience2),
            14 => Ok(Button::Audience3),
            15 => Ok(Button::Audience4),
            other => Err(other),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Dial {
    Dial1 = 0,
    Dial2 = 1,
    Dial3 = 2,
    Dial4 = 3,
}

impl TryFrom<u8> for Dial {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Dial::Dial1),
            1 => Ok(Dial::Dial2),
            2 => Ok(Dial::Dial3),
            3 => Ok(Dial::Dial4),
            other => Err(other),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ButtonLighting {
    Dial1 = 0,
    Dial2 = 1,
    Dial3 = 2,
    Dial4 = 3,
    Mix = 4,
    Left = 5,
    Right = 6,
}

impl TryFrom<u8> for ButtonLighting {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(ButtonLighting::Dial1),
            1 => Ok(ButtonLighting::Dial2),
            2 => Ok(ButtonLighting::Dial3),
            3 => Ok(ButtonLighting::Dial4),
            4 => Ok(ButtonLighting::Mix),
            5 => Ok(ButtonLighting::Left),
            6 => Ok(ButtonLighting::Right),
            other => Err(other),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_try_from_valid() {
        assert_eq!(Button::try_from(0), Ok(Button::AudienceMix));
        assert_eq!(Button::try_from(8), Ok(Button::Dial1));
        assert_eq!(Button::try_from(15), Ok(Button::Audience4));
    }

    #[test]
    fn button_try_from_invalid() {
        assert_eq!(Button::try_from(3), Err(3));
        assert_eq!(Button::try_from(7), Err(7));
        assert_eq!(Button::try_from(16), Err(16));
    }

    #[test]
    fn button_is_pressed() {
        let mask: u16 = (1 << 0) | (1 << 8); // AudienceMix + Dial1
        assert!(Button::AudienceMix.is_pressed(mask));
        assert!(Button::Dial1.is_pressed(mask));
        assert!(!Button::PageLeft.is_pressed(mask));
        assert!(!Button::Dial2.is_pressed(mask));
    }

    #[test]
    fn dial_try_from() {
        assert_eq!(Dial::try_from(0), Ok(Dial::Dial1));
        assert_eq!(Dial::try_from(3), Ok(Dial::Dial4));
        assert_eq!(Dial::try_from(4), Err(4));
    }

    #[test]
    fn button_lighting_try_from() {
        assert_eq!(ButtonLighting::try_from(4), Ok(ButtonLighting::Mix));
        assert_eq!(ButtonLighting::try_from(6), Ok(ButtonLighting::Right));
        assert_eq!(ButtonLighting::try_from(7), Err(7));
    }
}
