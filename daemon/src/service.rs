use std::collections::HashMap;
use std::sync::Arc;

use mixctl_core::{InputInfo, OutputInfo, RouteInfo};
use tokio::sync::Mutex;
use zbus::object_server::SignalEmitter;

use tracing::{info, warn};

use crate::audio::{PwCommand, PwEvent};
use crate::audio::volume::combine_pw_volume;
use crate::config::{ChannelConfig, ConfigFile};
use crate::state::{CaptureDeviceState, OutputState, RouteState, StateFile, StreamState};

pub enum ServiceSignal {
    AudioStatusChanged,
    CaptureDevicesChanged,
    StreamsChanged,
}

impl ServiceSignal {
    pub async fn emit(&self, emitter: &SignalEmitter<'_>) {
        match self {
            Self::AudioStatusChanged => {
                Service::emit_audio_status_changed(emitter).await.ok();
            }
            Self::CaptureDevicesChanged => {
                Service::emit_capture_devices_changed(emitter).await.ok();
            }
            Self::StreamsChanged => {
                Service::emit_streams_changed(emitter).await.ok();
            }
        }
    }
}

#[derive(Clone)]
pub struct Service {
    pub(crate) inner: Arc<Mutex<Shared>>,
}

pub struct Shared {
    pub config: ConfigFile,
    pub state: StateFile,
    pub config_dirty: bool,
    pub state_dirty: bool,
    pub pw_commands: tokio::sync::mpsc::UnboundedSender<PwCommand>,
    pub signal_tx: tokio::sync::mpsc::UnboundedSender<ServiceSignal>,
    pub audio_connected: bool,
    pub active_streams: HashMap<u32, StreamState>,
    pub capture_devices: HashMap<u32, CaptureDeviceState>,
    pub original_default_sink: Option<String>,
    pub original_stream_targets: HashMap<u32, String>,
}

impl Shared {
    /// Persist current stream→input assignments as app rules for restart.
    pub fn persist_stream_assignments(&mut self) {
        for stream in self.active_streams.values() {
            if stream.input_id == 0 {
                continue;
            }
            if self.match_app_rule(&stream.app_name).is_some() {
                continue;
            }
            self.config.app_rules.push(crate::config::AppRule {
                app_name: stream.app_name.clone(),
                input_id: stream.input_id,
            });
            self.config_dirty = true;
        }
    }

    /// Match an app name against configured rules. Returns the input_id if matched.
    pub fn match_app_rule(&self, app_name: &str) -> Option<u32> {
        for rule in &self.config.app_rules {
            if rule.app_name == app_name {
                return Some(rule.input_id);
            }
            // Glob pattern match
            if glob_match::glob_match(&rule.app_name, app_name) {
                return Some(rule.input_id);
            }
        }
        None
    }

    /// Compute the combined PW volume for a route and send a SetRouteLink command.
    pub fn send_route_link(&self, input_id: u32, output_id: u32) {
        let rs = self
            .state
            .route_state(input_id, output_id)
            .cloned()
            .unwrap_or_default();
        let os = self
            .state
            .output_state(output_id)
            .cloned()
            .unwrap_or_default();
        let pw_vol = combine_pw_volume(rs.volume, rs.muted, os.volume, os.muted);
        Service::send_pw_cmd(
            self,
            PwCommand::SetRouteLink {
                input_id,
                output_id,
                volume: pw_vol,
            },
        );
    }
}

impl Service {
    pub fn new(
        config: ConfigFile,
        state: StateFile,
        pw_commands: tokio::sync::mpsc::UnboundedSender<PwCommand>,
        signal_tx: tokio::sync::mpsc::UnboundedSender<ServiceSignal>,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Shared {
                config,
                state,
                config_dirty: false,
                state_dirty: false,
                pw_commands,
                signal_tx,
                audio_connected: false,
                active_streams: HashMap::new(),
                capture_devices: HashMap::new(),
                original_default_sink: None,
                original_stream_targets: HashMap::new(),
            })),
        }
    }

    pub fn build_input_info(cfg: &ChannelConfig) -> InputInfo {
        InputInfo {
            id: cfg.id(),
            name: cfg.name.clone(),
            color: cfg.color.clone(),
        }
    }

    pub fn build_output_info(cfg: &ChannelConfig, state: &OutputState) -> OutputInfo {
        OutputInfo {
            id: cfg.id(),
            name: cfg.name.clone(),
            color: cfg.color.clone(),
            volume: state.volume,
            muted: state.muted,
        }
    }

    pub fn build_route_info(input_id: u32, output_id: u32, state: &RouteState) -> RouteInfo {
        RouteInfo {
            input_id,
            output_id,
            volume: state.volume,
            muted: state.muted,
        }
    }

    pub async fn handle_pw_event(&self, event: PwEvent) {
        match event {
            PwEvent::Connected => {
                info!("PipeWire connected");
                let mut shared = self.inner.lock().await;
                shared.audio_connected = true;
                shared.signal_tx.send(ServiceSignal::AudioStatusChanged).ok();
            }
            PwEvent::Disconnected => {
                warn!("PipeWire disconnected, will retry");
                let mut shared = self.inner.lock().await;
                shared.audio_connected = false;
                shared.signal_tx.send(ServiceSignal::AudioStatusChanged).ok();
            }
            PwEvent::InputSinkCreated {
                input_id,
                pw_node_id,
            } => {
                info!("input sink created: input={input_id} pw_node={pw_node_id}");
            }
            PwEvent::InputSinkDestroyed { input_id } => {
                info!("input sink destroyed: input={input_id}");
            }
            PwEvent::OutputSourceCreated {
                output_id,
                pw_node_id,
            } => {
                info!("output source created: output={output_id} pw_node={pw_node_id}");
            }
            PwEvent::OutputSourceDestroyed { output_id } => {
                info!("output source destroyed: output={output_id}");
            }
            PwEvent::RouteLinkCreated {
                input_id,
                output_id,
            } => {
                info!("route link created: {input_id} → {output_id}");
            }
            PwEvent::StreamAppeared {
                pw_node_id,
                app_name,
                media_name,
            } => {
                info!("stream appeared: node={pw_node_id} app={app_name} media={media_name}");
                let mut shared = self.inner.lock().await;
                let target_input = shared.match_app_rule(&app_name);
                let input_id = target_input
                    .or(shared.config.default_input)
                    .or_else(|| shared.config.inputs.first().map(|i| i.id()))
                    .unwrap_or(0);

                shared.active_streams.insert(
                    pw_node_id,
                    StreamState {
                        app_name: app_name.clone(),
                        media_name,
                        input_id,
                    },
                );

                if input_id > 0 {
                    shared
                        .pw_commands
                        .send(PwCommand::MoveStream {
                            pw_node_id,
                            input_id,
                        })
                        .ok();
                }
                shared.signal_tx.send(ServiceSignal::StreamsChanged).ok();
            }
            PwEvent::StreamRemoved { pw_node_id } => {
                info!("stream removed: node={pw_node_id}");
                let mut shared = self.inner.lock().await;
                shared.active_streams.remove(&pw_node_id);
                shared.signal_tx.send(ServiceSignal::StreamsChanged).ok();
            }
            PwEvent::CaptureDeviceAppeared {
                pw_node_id,
                name,
                device_name,
            } => {
                info!("capture device appeared: node={pw_node_id} name={name} device={device_name}");
                let mut shared = self.inner.lock().await;
                shared.capture_devices.insert(
                    pw_node_id,
                    CaptureDeviceState {
                        name,
                        device_name,
                    },
                );
                shared.signal_tx.send(ServiceSignal::CaptureDevicesChanged).ok();
            }
            PwEvent::CaptureDeviceRemoved { pw_node_id } => {
                info!("capture device removed: node={pw_node_id}");
                let mut shared = self.inner.lock().await;
                shared.capture_devices.remove(&pw_node_id);
                shared.signal_tx.send(ServiceSignal::CaptureDevicesChanged).ok();
            }
            PwEvent::OriginalDefaultSink { value } => {
                let mut shared = self.inner.lock().await;
                if shared.original_default_sink.is_none() {
                    shared.original_default_sink = value;
                }
            }
            PwEvent::OriginalStreamTarget { stream_id, value } => {
                let mut shared = self.inner.lock().await;
                shared.original_stream_targets.entry(stream_id).or_insert(value);
            }
            PwEvent::ChannelReady { .. } => {
                // Handled directly in main.rs event loop, not forwarded here
            }
            PwEvent::Error { message } => {
                warn!("PipeWire error: {message}");
            }
        }
    }

    /// Send a command to the PipeWire engine.
    pub fn send_pw_cmd(shared: &Shared, cmd: PwCommand) {
        if let Err(e) = shared.pw_commands.send(cmd) {
            tracing::warn!("failed to send PW command: {e}");
        }
    }
}
