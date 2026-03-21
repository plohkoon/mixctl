/// Commands sent from the tokio side to the PipeWire thread.
#[derive(Debug)]
#[allow(dead_code)]
pub enum PwCommand {
    // -- Phase 1: Input sinks --
    CreateInputSink {
        input_id: u32,
        description: String,
    },
    DestroyInputSink {
        input_id: u32,
    },
    SetDefaultInput {
        input_id: u32,
    },
    RenameInputSink {
        input_id: u32,
        description: String,
    },

    // -- Phase 2: Output sources + routing --
    CreateOutputSource {
        output_id: u32,
        description: String,
    },
    DestroyOutputSource {
        output_id: u32,
    },
    RenameOutputSource {
        output_id: u32,
        description: String,
    },
    /// Update the volume matrix entry for a route. No PW objects created/destroyed.
    SetRouteLink {
        input_id: u32,
        output_id: u32,
        volume: f32,
    },
    /// Zero the volume matrix entry for a route. No PW objects created/destroyed.
    DestroyRouteLink {
        input_id: u32,
        output_id: u32,
    },
    SetOutputTarget {
        output_id: u32,
        device_name: Option<String>,
    },

    // -- Phase 3: Stream assignment --
    MoveStream {
        pw_node_id: u32,
        input_id: u32,
    },

    // -- Phase 4: Capture devices --
    CreateCaptureInput {
        input_id: u32,
        description: String,
        capture_device_name: String,
    },
    /// Bind a capture device to an existing input (creates direct links).
    BindCaptureToInput {
        input_id: u32,
        capture_device_name: String,
    },
    DestroyCaptureLoopback {
        input_id: u32,
    },
    SetCaptureVolume {
        input_id: u32,
        pw_volume: f32,
    },

    // -- Level monitoring --
    EnableLevelMonitoring,
    DisableLevelMonitoring,

    /// Graceful shutdown of the PipeWire thread.
    Shutdown {
        original_default_sink: Option<String>,
        original_stream_targets: std::collections::HashMap<u32, String>,
    },
}
