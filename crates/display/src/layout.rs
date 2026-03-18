use crate::types::DisplayState;

/// A patch to send to the display: JPEG bytes + position.
pub struct Patch {
    pub jpeg: Vec<u8>,
    pub x: u32,
    pub y: u32,
}

/// Trait for display layout implementations.
/// Each layout decides how to render the mixer state onto 800x480.
pub trait DisplayLayout: Send {
    /// Render a full 800x480 frame (used on init, page/output switch).
    fn render_full(&self, state: &DisplayState) -> Vec<u8>;

    /// Render incremental patches for changed state.
    /// Compares prev and next, returns only the patches needed.
    fn render_diff(&self, prev: &DisplayState, next: &DisplayState) -> Vec<Patch>;
}
