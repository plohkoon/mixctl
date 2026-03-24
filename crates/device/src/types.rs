use std::time::Duration;

use mixctl_beacn_display::{DisplayLayout, DisplayState};
use mixctl_core::config_sections::ButtonMappings;

/// Commands sent from the daemon to the device thread.
pub enum DeviceCommand {
    /// Update the display with new mixer state.
    UpdateState(DisplayState),
    /// Switch to a different display layout at runtime.
    ChangeLayout(Box<dyn DisplayLayout>),
    /// Update button mappings and hold threshold from config.
    SetButtonConfig {
        mappings: ButtonMappings,
        hold_threshold: Duration,
    },
    /// Set display and LED brightness.
    SetBrightness { display: u8, led: u8 },
    /// Show a "waiting for daemon" screen on the device.
    ShowWaiting,
    /// Shut down the device thread.
    Shutdown,
}

/// Events sent from the device thread to the daemon.
#[derive(Debug)]
pub enum DeviceEvent {
    /// Device was found and initialized.
    Connected,
    /// Device was disconnected or errored.
    Disconnected,
    /// Adjust route volume for input on current output.
    AdjustRouteVolume {
        input_id: u32,
        output_id: u32,
        delta: i8,
    },
    /// Toggle route mute for input on current output.
    ToggleRouteMute {
        input_id: u32,
        output_id: u32,
    },
    /// Toggle global mute for input on all outputs.
    ToggleGlobalMute {
        input_id: u32,
    },
    /// Toggle mute on a specific output.
    ToggleOutputMute {
        output_id: u32,
    },
    /// Toggle mute on all outputs.
    ToggleAllOutputsMute,
    /// Toggle EQ for an input.
    ToggleEq {
        input_id: u32,
    },
    /// Toggle noise gate for an input.
    ToggleGate {
        input_id: u32,
    },
    /// Toggle de-esser for an input.
    ToggleDeesser {
        input_id: u32,
    },
    /// Toggle compressor for an output.
    ToggleCompressor {
        output_id: u32,
    },
    /// Toggle limiter for an output.
    ToggleLimiter {
        output_id: u32,
    },
    /// Load a named profile.
    LoadProfile {
        name: String,
    },
    /// Explicitly set global mute state (for push-to-talk/mute release).
    SetGlobalMute {
        input_id: u32,
        muted: bool,
    },
    /// Rotate to next output tab.
    NextOutput,
    /// Rotate to previous output tab.
    PrevOutput,
    /// Move to previous page of inputs.
    PageLeft,
    /// Move to next page of inputs.
    PageRight,
}
