/// Events sent from the PipeWire thread back to the tokio side.
pub enum PwEvent {
    // -- Connection lifecycle --
    Connected,
    Disconnected,
    /// PW thread created a new channel sender after reconnecting.
    ChannelReady {
        sender: pipewire::channel::Sender<super::commands::PwCommand>,
    },

    // -- Phase 1: Input sinks --
    InputSinkCreated {
        input_id: u32,
        pw_node_id: u32,
    },
    InputSinkDestroyed {
        input_id: u32,
    },

    // -- Phase 2: Output sources + routing --
    OutputSourceCreated {
        output_id: u32,
        pw_node_id: u32,
    },
    OutputSourceDestroyed {
        output_id: u32,
    },
    RouteLinkCreated {
        input_id: u32,
        output_id: u32,
    },

    // -- Phase 3: Stream assignment --
    StreamAppeared {
        pw_node_id: u32,
        app_name: String,
        media_name: String,
    },
    StreamRemoved {
        pw_node_id: u32,
    },

    // -- Phase 4: Capture devices --
    CaptureDeviceAppeared {
        pw_node_id: u32,
        name: String,
        device_name: String,
    },
    CaptureDeviceRemoved {
        pw_node_id: u32,
    },

    // -- Playback devices --
    PlaybackDeviceAppeared {
        pw_node_id: u32,
        name: String,
        device_name: String,
    },
    PlaybackDeviceRemoved {
        pw_node_id: u32,
    },

    // -- Original state (for shutdown restoration) --
    OriginalDefaultSink {
        value: Option<String>,
    },
    OriginalStreamTarget {
        stream_id: u32,
        value: String,
    },

    // -- Level monitoring --
    LevelUpdate {
        levels: Vec<(u32, f32)>,
    },

    // -- Errors --
    Error {
        message: String,
    },
}

// Manual Debug impl because pipewire::channel::Sender doesn't implement Debug
impl std::fmt::Debug for PwEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connected => write!(f, "Connected"),
            Self::Disconnected => write!(f, "Disconnected"),
            Self::ChannelReady { .. } => write!(f, "ChannelReady"),
            Self::InputSinkCreated { input_id, pw_node_id } => {
                f.debug_struct("InputSinkCreated")
                    .field("input_id", input_id)
                    .field("pw_node_id", pw_node_id)
                    .finish()
            }
            Self::InputSinkDestroyed { input_id } => {
                f.debug_struct("InputSinkDestroyed")
                    .field("input_id", input_id)
                    .finish()
            }
            Self::OutputSourceCreated { output_id, pw_node_id } => {
                f.debug_struct("OutputSourceCreated")
                    .field("output_id", output_id)
                    .field("pw_node_id", pw_node_id)
                    .finish()
            }
            Self::OutputSourceDestroyed { output_id } => {
                f.debug_struct("OutputSourceDestroyed")
                    .field("output_id", output_id)
                    .finish()
            }
            Self::RouteLinkCreated { input_id, output_id } => {
                f.debug_struct("RouteLinkCreated")
                    .field("input_id", input_id)
                    .field("output_id", output_id)
                    .finish()
            }
            Self::StreamAppeared { pw_node_id, app_name, media_name } => {
                f.debug_struct("StreamAppeared")
                    .field("pw_node_id", pw_node_id)
                    .field("app_name", app_name)
                    .field("media_name", media_name)
                    .finish()
            }
            Self::StreamRemoved { pw_node_id } => {
                f.debug_struct("StreamRemoved")
                    .field("pw_node_id", pw_node_id)
                    .finish()
            }
            Self::CaptureDeviceAppeared { pw_node_id, name, device_name } => {
                f.debug_struct("CaptureDeviceAppeared")
                    .field("pw_node_id", pw_node_id)
                    .field("name", name)
                    .field("device_name", device_name)
                    .finish()
            }
            Self::CaptureDeviceRemoved { pw_node_id } => {
                f.debug_struct("CaptureDeviceRemoved")
                    .field("pw_node_id", pw_node_id)
                    .finish()
            }
            Self::PlaybackDeviceAppeared { pw_node_id, name, device_name } => {
                f.debug_struct("PlaybackDeviceAppeared")
                    .field("pw_node_id", pw_node_id)
                    .field("name", name)
                    .field("device_name", device_name)
                    .finish()
            }
            Self::PlaybackDeviceRemoved { pw_node_id } => {
                f.debug_struct("PlaybackDeviceRemoved")
                    .field("pw_node_id", pw_node_id)
                    .finish()
            }
            Self::OriginalDefaultSink { value } => {
                f.debug_struct("OriginalDefaultSink")
                    .field("value", value)
                    .finish()
            }
            Self::OriginalStreamTarget { stream_id, value } => {
                f.debug_struct("OriginalStreamTarget")
                    .field("stream_id", stream_id)
                    .field("value", value)
                    .finish()
            }
            Self::LevelUpdate { levels } => {
                f.debug_struct("LevelUpdate")
                    .field("levels", levels)
                    .finish()
            }
            Self::Error { message } => {
                f.debug_struct("Error")
                    .field("message", message)
                    .finish()
            }
        }
    }
}
