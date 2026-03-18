use mixctl_beacn_display::DisplayState;

/// Commands sent from the daemon to the device thread.
pub enum DeviceCommand {
    /// Update the display with new mixer state.
    UpdateState(DisplayState),
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
    /// Rotate to next output tab.
    NextOutput,
    /// Move to previous page of inputs.
    PageLeft,
    /// Move to next page of inputs.
    PageRight,
}
