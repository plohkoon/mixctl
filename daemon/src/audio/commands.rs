/// Commands sent from the tokio side to the PipeWire thread.
#[derive(Debug)]
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
    /// Set or recreate a route loopback with combined volume (route_vol * output_vol).
    /// Muting is encoded as pw_volume = 0.0.
    SetRouteLink {
        input_id: u32,
        output_id: u32,
        volume: f32,
    },
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
    DestroyCaptureLoopback {
        input_id: u32,
    },
    SetCaptureVolume {
        input_id: u32,
        pw_volume: f32,
    },

    /// Graceful shutdown of the PipeWire thread.
    Shutdown,
}
