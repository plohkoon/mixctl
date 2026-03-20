pub mod column4;
pub mod dial4;
pub mod grid2x2;
pub mod layout;
pub mod render;
pub mod types;

pub use column4::Column4Layout;
pub use dial4::Dial4Layout;
pub use grid2x2::Grid2x2Layout;
pub use layout::{DisplayLayout, Patch};
pub use types::{DisplayState, OutputTab, SlotView};

/// Layout variant for the hardware device display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeviceLayoutKind {
    #[default]
    Column,
    Grid,
    Dial,
}

impl DeviceLayoutKind {
    pub fn from_str_loose(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "grid" | "grid2x2" | "2x2" => Self::Grid,
            "dial" | "dial4" | "dials" => Self::Dial,
            _ => Self::Column,
        }
    }

    pub fn create_layout(self) -> Box<dyn DisplayLayout> {
        match self {
            Self::Column => Box::new(Column4Layout::new()),
            Self::Grid => Box::new(Grid2x2Layout::new()),
            Self::Dial => Box::new(Dial4Layout::new()),
        }
    }
}
