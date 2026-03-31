use serde::{Deserialize, Serialize};

// ── Capability model ─────────────────────────────────────────────────

/// Hardware capabilities a device can declare.
/// Used for capability advertisement so the mixer daemon (and eventually the UI)
/// knows what a connected device can do.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Capability {
    Fader {
        count: u8,
        range: (f32, f32),
    },
    Button {
        count: u8,
        kind: ButtonKind,
    },
    Screen {
        width: u16,
        height: u16,
        format: ScreenFormat,
    },
    Led {
        count: u8,
        color_mode: ColorMode,
    },
    Meter {
        count: u8,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ButtonKind {
    Momentary,
    Toggle,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ScreenFormat {
    Jpeg,
    Raw,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ColorMode {
    Rgb,
    SingleColor,
}

// ── Mixer events (from D-Bus signals) ────────────────────────────────
//
// Each variant maps to a specific D-Bus signal from the mixer daemon.
// Adapters match on variants they care about and ignore the rest.
// This avoids the "one coarse callback" problem where 20Hz level updates
// trigger the same handler as rare config changes.

#[derive(Debug, Clone)]
pub enum MixerEvent {
    /// Inputs configuration changed (added/removed/renamed)
    InputsChanged,
    /// Outputs configuration changed
    OutputsChanged,
    /// A specific output's volume or mute state changed
    OutputStateChanged { id: u32 },
    /// A specific route's volume or mute changed
    RouteChanged { input_id: u32, output_id: u32 },
    /// Audio streams changed (apps assigned/unassigned to inputs)
    StreamsChanged,
    /// Audio levels updated (~20Hz when broadcast_levels is enabled)
    LevelsChanged { levels: Vec<(u32, f64)> },
    /// Broadcast levels monitoring toggled
    BroadcastLevelsChanged { enabled: bool },
    /// A config section was updated (e.g. "beacn", "ui")
    ConfigSectionChanged { section: String },
    /// A custom input value changed
    CustomInputChanged { id: u32 },
    /// Audio engine status changed
    AudioStatusChanged,
    /// Component registry changed
    ComponentChanged,
    /// DSP settings changed on an input (EQ, gate, de-esser)
    InputDspChanged { input_id: u32 },
    /// DSP settings changed on an output (compressor, limiter)
    OutputDspChanged { output_id: u32 },
    /// A profile was loaded
    ProfileChanged { name: String },
    /// Capture devices changed
    CaptureDevicesChanged,
    /// Playback devices changed
    PlaybackDevicesChanged,
}

// ── Raw hardware input ───────────────────────────────────────────────
//
// Thin representation of raw hardware events. Adapters that use
// ChannelAdapter to bridge a device thread can define their own
// richer event types internally — DeviceInput is provided as a
// convenience for simple adapters (e.g. MIDI fader banks).

#[derive(Debug, Clone)]
pub enum DeviceInput {
    /// Rotary dial or linear fader moved by a relative delta
    FaderMoved { index: u8, delta: i16 },
    /// Button was pressed
    ButtonPressed { index: u8 },
    /// Button was released
    ButtonReleased { index: u8 },
    /// Device connected to host USB/MIDI/HID
    Connected,
    /// Device disconnected from host
    Disconnected,
}
