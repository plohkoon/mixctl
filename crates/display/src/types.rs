/// Full snapshot of the mixer state for display rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayState {
    /// Which output tab is currently selected (index into `outputs`)
    pub current_output_index: usize,
    /// All output tabs
    pub outputs: Vec<OutputTab>,
    /// The 4 input slots visible on the current page
    pub visible_inputs: [Option<SlotView>; 4],
    /// Current page number (0-based)
    pub page: u32,
    /// Total number of pages
    pub total_pages: u32,
}

/// An output tab shown in the header bar.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputTab {
    pub id: u32,
    pub name: String,
    pub color: (u8, u8, u8),
    pub is_current: bool,
}

/// A single input slot in the 2x2 grid.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotView {
    pub input_id: u32,
    pub name: String,
    pub color: (u8, u8, u8),
    pub volume: u8,
    pub route_muted: bool,
    pub global_muted: bool,
    /// Real-time audio level (0.0-1.0, mono peak). None when level monitoring is disabled.
    pub level: Option<f32>,
    /// App names of audio streams currently assigned to this input.
    pub streams: Vec<String>,
}

#[cfg(test)]
pub(crate) fn test_display_state() -> DisplayState {
    DisplayState {
        current_output_index: 0,
        outputs: vec![
            OutputTab {
                id: 1,
                name: "Personal".into(),
                color: (142, 68, 173),
                is_current: true,
            },
            OutputTab {
                id: 2,
                name: "Stream".into(),
                color: (52, 152, 219),
                is_current: false,
            },
        ],
        visible_inputs: [
            Some(SlotView {
                input_id: 1,
                name: "System".into(),
                color: (74, 144, 217),
                volume: 80,
                route_muted: false,
                global_muted: false,
                level: None,
                streams: vec!["Factorio".into(), "Discord".into()],
            }),
            Some(SlotView {
                input_id: 2,
                name: "Game".into(),
                color: (231, 76, 60),
                volume: 60,
                route_muted: false,
                global_muted: false,
                level: None,
                streams: vec![],
            }),
            Some(SlotView {
                input_id: 3,
                name: "Music".into(),
                color: (46, 204, 113),
                volume: 100,
                route_muted: true,
                global_muted: false,
                level: None,
                streams: vec![],
            }),
            Some(SlotView {
                input_id: 4,
                name: "Chat".into(),
                color: (243, 156, 18),
                volume: 50,
                route_muted: false,
                global_muted: true,
                level: None,
                streams: vec![],
            }),
        ],
        page: 0,
        total_pages: 2,
    }
}
