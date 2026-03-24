use std::collections::HashMap;
use std::sync::Arc;

use mixctl_core::{InputInfo, OutputInfo, RouteInfo};
use tokio::sync::Mutex;
use zbus::object_server::SignalEmitter;

use tracing::{info, warn};

use crate::audio::{PwCommand, PwEvent};
use crate::audio::volume::combine_pw_volume;
use crate::config::{ChannelConfig, ConfigFile};
use crate::state::{CaptureDeviceState, OutputState, PlaybackDeviceState, RouteState, StateFile, StreamState};

pub enum ServiceSignal {
    AudioStatusChanged,
    CaptureDevicesChanged,
    PlaybackDevicesChanged,
    StreamsChanged,
    InputLevelsChanged { levels: Vec<(u32, f64)> },
    BroadcastLevelsChanged { enabled: bool },
    ConfigSectionChanged { section: String },
    ComponentChanged,
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
            Self::PlaybackDevicesChanged => {
                Service::emit_playback_devices_changed(emitter).await.ok();
            }
            Self::StreamsChanged => {
                Service::emit_streams_changed(emitter).await.ok();
            }
            Self::InputLevelsChanged { levels } => {
                Service::emit_input_levels_changed(emitter, levels.clone()).await.ok();
            }
            Self::BroadcastLevelsChanged { enabled } => {
                Service::emit_broadcast_levels_changed(emitter, *enabled).await.ok();
            }
            Self::ConfigSectionChanged { section } => {
                Service::emit_config_section_changed(emitter, section.clone()).await.ok();
            }
            Self::ComponentChanged => {
                Service::emit_component_changed(emitter).await.ok();
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
    pub playback_devices: HashMap<u32, PlaybackDeviceState>,
    pub original_default_sink: Option<String>,
    pub original_default_source: Option<String>,
    pub original_stream_targets: HashMap<u32, String>,
    pub input_levels: HashMap<u32, f32>,
    /// Registered components: bus_name → component_type
    pub components: HashMap<String, String>,
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
                playback_devices: HashMap::new(),
                original_default_sink: None,
                original_default_source: None,
                original_stream_targets: HashMap::new(),
                input_levels: HashMap::new(),
                components: HashMap::new(),
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
            target_device: cfg.target_device.clone().unwrap_or_default(),
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
            PwEvent::PlaybackDeviceAppeared {
                pw_node_id,
                name,
                device_name,
            } => {
                info!("playback device appeared: node={pw_node_id} name={name} device={device_name}");
                let mut shared = self.inner.lock().await;
                shared.playback_devices.insert(
                    pw_node_id,
                    PlaybackDeviceState {
                        name,
                        device_name,
                    },
                );
                shared.signal_tx.send(ServiceSignal::PlaybackDevicesChanged).ok();
            }
            PwEvent::PlaybackDeviceRemoved { pw_node_id } => {
                info!("playback device removed: node={pw_node_id}");
                let mut shared = self.inner.lock().await;
                shared.playback_devices.remove(&pw_node_id);
                shared.signal_tx.send(ServiceSignal::PlaybackDevicesChanged).ok();
            }
            PwEvent::OriginalDefaultSink { value } => {
                let mut shared = self.inner.lock().await;
                if shared.original_default_sink.is_none() {
                    shared.original_default_sink = value;
                }
            }
            PwEvent::OriginalDefaultSource { value } => {
                let mut shared = self.inner.lock().await;
                if shared.original_default_source.is_none() {
                    shared.original_default_source = value;
                }
            }
            PwEvent::OriginalStreamTarget { stream_id, value } => {
                let mut shared = self.inner.lock().await;
                shared.original_stream_targets.entry(stream_id).or_insert(value);
            }
            PwEvent::LevelUpdate { levels } => {
                let mut shared = self.inner.lock().await;
                for &(input_id, level) in &levels {
                    shared.input_levels.insert(input_id, level);
                }
                let dbus_levels: Vec<(u32, f64)> = levels
                    .into_iter()
                    .map(|(id, lvl)| (id, lvl as f64))
                    .collect();
                shared
                    .signal_tx
                    .send(ServiceSignal::InputLevelsChanged { levels: dbus_levels })
                    .ok();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppRule, ChannelConfig, ConfigFile};
    use crate::state::StateFile;

    fn make_shared_for_rules(rules: Vec<AppRule>) -> Shared {
        let (pw_tx, _pw_rx) = tokio::sync::mpsc::unbounded_channel();
        let (sig_tx, _sig_rx) = tokio::sync::mpsc::unbounded_channel();
        let config = ConfigFile {
            version: 1,
            inputs: vec![ChannelConfig {
                id: Some(1),
                name: "Sys".into(),
                color: "#000000".into(),
                target_device: None,
                capture_device: None,
            }],
            outputs: vec![],
            default_input: None,
            default_output: None,
            app_rules: rules,
            broadcast_levels: None,
            beacn: Default::default(),
            ui: Default::default(),
            applet: Default::default(),
            cli: Default::default(),
            tui: Default::default(),
        };
        Shared {
            config,
            state: StateFile::default(),
            config_dirty: false,
            state_dirty: false,
            pw_commands: pw_tx,
            signal_tx: sig_tx,
            audio_connected: false,
            active_streams: HashMap::new(),
            capture_devices: HashMap::new(),
            playback_devices: HashMap::new(),
            original_default_sink: None,
            original_default_source: None,
            original_stream_targets: HashMap::new(),
            input_levels: HashMap::new(),
            components: HashMap::new(),
        }
    }

    #[test]
    fn match_app_rule_exact_match() {
        let shared = make_shared_for_rules(vec![AppRule {
            app_name: "spotify".into(),
            input_id: 1,
        }]);
        assert_eq!(shared.match_app_rule("spotify"), Some(1));
    }

    #[test]
    fn match_app_rule_no_match_returns_none() {
        let shared = make_shared_for_rules(vec![AppRule {
            app_name: "spotify".into(),
            input_id: 1,
        }]);
        assert_eq!(shared.match_app_rule("firefox"), None);
    }

    #[test]
    fn match_app_rule_glob_pattern() {
        let shared = make_shared_for_rules(vec![AppRule {
            app_name: "fire*".into(),
            input_id: 2,
        }]);
        assert_eq!(shared.match_app_rule("firefox"), Some(2));
        assert_eq!(shared.match_app_rule("firewall"), Some(2));
        assert_eq!(shared.match_app_rule("chrome"), None);
    }

    #[test]
    fn persist_stream_assignments_creates_rules() {
        let mut shared = make_shared_for_rules(vec![]);
        shared.active_streams.insert(100, StreamState {
            app_name: "spotify".into(),
            media_name: "Music".into(),
            input_id: 1,
        });
        shared.active_streams.insert(101, StreamState {
            app_name: "discord".into(),
            media_name: "Voice".into(),
            input_id: 1,
        });

        shared.persist_stream_assignments();

        assert_eq!(shared.config.app_rules.len(), 2);
        assert!(shared.config.app_rules.iter().any(|r| r.app_name == "spotify" && r.input_id == 1));
        assert!(shared.config.app_rules.iter().any(|r| r.app_name == "discord" && r.input_id == 1));
        assert!(shared.config_dirty);
    }

    #[test]
    fn persist_stream_assignments_no_duplicates() {
        let mut shared = make_shared_for_rules(vec![AppRule {
            app_name: "spotify".into(),
            input_id: 1,
        }]);
        shared.active_streams.insert(100, StreamState {
            app_name: "spotify".into(),
            media_name: "Music".into(),
            input_id: 1,
        });

        shared.persist_stream_assignments();

        // Should not add a duplicate rule
        let spotify_rules: Vec<_> = shared.config.app_rules.iter()
            .filter(|r| r.app_name == "spotify")
            .collect();
        assert_eq!(spotify_rules.len(), 1);
    }

    #[test]
    fn build_input_info_correct() {
        let cfg = ChannelConfig {
            id: Some(42),
            name: "TestInput".into(),
            color: "#AABBCC".into(),
            target_device: None,
            capture_device: None,
        };
        let info = Service::build_input_info(&cfg);
        assert_eq!(info.id, 42);
        assert_eq!(info.name, "TestInput");
        assert_eq!(info.color, "#AABBCC");
    }

    #[test]
    fn build_output_info_correct() {
        let cfg = ChannelConfig {
            id: Some(7),
            name: "MainMix".into(),
            color: "#112233".into(),
            target_device: Some("alsa_output.usb".into()),
            capture_device: None,
        };
        let state = crate::state::OutputState {
            volume: 75,
            muted: true,
        };
        let info = Service::build_output_info(&cfg, &state);
        assert_eq!(info.id, 7);
        assert_eq!(info.name, "MainMix");
        assert_eq!(info.color, "#112233");
        assert_eq!(info.volume, 75);
        assert!(info.muted);
        assert_eq!(info.target_device, "alsa_output.usb");
    }
}
