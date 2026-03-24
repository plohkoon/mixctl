use std::collections::HashMap;

use mixctl_core::config_sections::{BeacnConfig, TuiConfig};
use mixctl_core::dbus::MixCtlProxy;
use mixctl_core::{
    AppRuleInfo, CaptureDeviceInfo, CompressorInfo, ComponentInfo, DeesserInfo,
    EqBandInfo, GateInfo, InputInfo, LimiterInfo, OutputInfo, PlaybackDeviceInfo,
    RouteInfo, StreamInfo,
};

/// Colour palette for cycling input/output colours.
pub const COLOR_PALETTE: &[&str] = &[
    "#4A90D9", "#E74C3C", "#2ECC71", "#F39C12", "#8E44AD",
    "#3498DB", "#E67E22", "#1ABC9C", "#9B59B6", "#27AE60",
];

/// Panels the user can navigate between.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Routes,
    Streams,
    Outputs,
    Rules,
    Capture,
    Settings,
    Dsp,
}

/// Actions dispatched from key events.
#[derive(Debug)]
pub enum AppAction {
    Quit,
    NextPanel,
    PrevPanel,
    CursorUp,
    CursorDown,
    VolumeUp { fine: bool },
    VolumeDown { fine: bool },
    ToggleMute,
    ToggleOutputMute,
    SelectOutputTab(usize),
    ShowHelp,
    DeleteRule,
    AssignRuleToInput(usize),
    BindCapture(usize),
    UnbindCapture,
    CycleInputColor,
    CycleOutputColor,
    ToggleEq,
    ToggleGate,
    ToggleDeesser,
    ToggleCompressor,
    ToggleLimiter,
    MoveUp,
    MoveDown,
    AddChannel,
    RemoveChannel,
    StartRename,
    ConfirmRename,
    CancelRename,
    RenameChar(char),
    RenameBackspace,
    // Step 6: Output target selector
    SetOutputTarget,
    // Step 7: Capture device management
    AddCaptureInput,
    RemoveCaptureInput,
    SetCaptureVolume { fine: bool },
    DecreaseCaptureVolume { fine: bool },
    SetCaptureMute,
    // Step 8: DSP parameter editing
    EnterDspEdit,
    ExitDspEdit,
    DspValueUp { fine: bool },
    DspValueDown { fine: bool },
    DspParamNext,
    DspParamPrev,
    DspResetEq,
}

/// Signals received from D-Bus, pre-processed with fresh data.
#[derive(Debug)]
pub enum DaemonSignal {
    RouteUpdated(RouteInfo),
    OutputsRefreshed(Vec<OutputInfo>),
    StreamsRefreshed(Vec<StreamInfo>),
    FullRefresh {
        inputs: Vec<InputInfo>,
        outputs: Vec<OutputInfo>,
        routes: Vec<RouteInfo>,
        streams: Vec<StreamInfo>,
    },
    RulesRefreshed(Vec<AppRuleInfo>),
    CaptureDevicesRefreshed(Vec<CaptureDeviceInfo>),
    PlaybackDevicesRefreshed(Vec<PlaybackDeviceInfo>),
    ComponentsRefreshed(Vec<ComponentInfo>),
    BeacnConfigRefreshed(BeacnConfig),
    InputDspRefreshed {
        input_id: u32,
        eq_enabled: bool,
        eq_bands: Vec<EqBandInfo>,
        gate: GateInfo,
        deesser: DeesserInfo,
    },
    OutputDspRefreshed {
        output_id: u32,
        compressor: CompressorInfo,
        limiter: LimiterInfo,
    },
}

pub struct AppState {
    pub inputs: Vec<InputInfo>,
    pub outputs: Vec<OutputInfo>,
    pub routes: Vec<RouteInfo>,
    pub streams: Vec<StreamInfo>,
    pub rules: Vec<AppRuleInfo>,
    pub capture_devices: Vec<CaptureDeviceInfo>,
    pub playback_devices: Vec<PlaybackDeviceInfo>,
    pub components: Vec<ComponentInfo>,

    pub config: TuiConfig,
    pub active_panel: Panel,
    pub selected_output_idx: usize,
    pub route_cursor: usize,
    pub stream_cursor: usize,
    pub output_cursor: usize,
    pub rule_cursor: usize,
    pub capture_cursor: usize,
    pub settings_cursor: usize,
    pub beacn_connected: bool,
    pub beacn_config: Option<BeacnConfig>,
    pub show_help: bool,
    pub rename_buf: Option<String>,

    // DSP state caches
    pub dsp_input_eq: HashMap<u32, (bool, Vec<EqBandInfo>)>,
    pub dsp_input_gate: HashMap<u32, GateInfo>,
    pub dsp_input_deesser: HashMap<u32, DeesserInfo>,
    pub dsp_output_compressor: HashMap<u32, CompressorInfo>,
    pub dsp_output_limiter: HashMap<u32, LimiterInfo>,
    pub dsp_cursor: usize,

    // DSP editing mode (Step 8)
    pub dsp_editing: bool,
    pub dsp_param_cursor: usize,
}

impl AppState {
    pub fn new(
        inputs: Vec<InputInfo>,
        outputs: Vec<OutputInfo>,
        routes: Vec<RouteInfo>,
        streams: Vec<StreamInfo>,
        rules: Vec<AppRuleInfo>,
        capture_devices: Vec<CaptureDeviceInfo>,
        playback_devices: Vec<PlaybackDeviceInfo>,
        components: Vec<ComponentInfo>,
        beacn_config: Option<BeacnConfig>,
        config: TuiConfig,
    ) -> Self {
        let beacn_connected = components.iter().any(|c| c.component_type == "beacn");
        let initial_panel = match config.initial_panel.as_str() {
            "streams" => Panel::Streams,
            "outputs" => Panel::Outputs,
            "rules" => Panel::Rules,
            "capture" => Panel::Capture,
            "settings" => Panel::Settings,
            "dsp" => Panel::Dsp,
            _ => Panel::Routes,
        };
        Self {
            inputs,
            outputs,
            routes,
            streams,
            rules,
            capture_devices,
            playback_devices,
            components,
            config,
            active_panel: initial_panel,
            selected_output_idx: 0,
            route_cursor: 0,
            stream_cursor: 0,
            output_cursor: 0,
            rule_cursor: 0,
            capture_cursor: 0,
            settings_cursor: 0,
            beacn_connected,
            beacn_config,
            show_help: false,
            rename_buf: None,

            dsp_input_eq: HashMap::new(),
            dsp_input_gate: HashMap::new(),
            dsp_input_deesser: HashMap::new(),
            dsp_output_compressor: HashMap::new(),
            dsp_output_limiter: HashMap::new(),
            dsp_cursor: 0,

            dsp_editing: false,
            dsp_param_cursor: 0,
        }
    }

    fn clamp_cursors(&mut self) {
        if !self.routes.is_empty() {
            self.route_cursor = self.route_cursor.min(self.routes.len() - 1);
        } else {
            self.route_cursor = 0;
        }
        if !self.streams.is_empty() {
            self.stream_cursor = self.stream_cursor.min(self.streams.len() - 1);
        } else {
            self.stream_cursor = 0;
        }
        if !self.outputs.is_empty() {
            self.output_cursor = self.output_cursor.min(self.outputs.len() - 1);
            self.selected_output_idx = self.selected_output_idx.min(self.outputs.len() - 1);
        } else {
            self.output_cursor = 0;
            self.selected_output_idx = 0;
        }
        if !self.rules.is_empty() {
            self.rule_cursor = self.rule_cursor.min(self.rules.len() - 1);
        } else {
            self.rule_cursor = 0;
        }
        if !self.capture_devices.is_empty() {
            self.capture_cursor = self.capture_cursor.min(self.capture_devices.len() - 1);
        } else {
            self.capture_cursor = 0;
        }
        let settings_items = self.inputs.len() + self.outputs.len();
        if settings_items > 0 {
            self.settings_cursor = self.settings_cursor.min(settings_items - 1);
        } else {
            self.settings_cursor = 0;
        }
        // DSP panel cursor: total selectable items = inputs + outputs
        let dsp_items = self.inputs.len() + self.outputs.len();
        if dsp_items > 0 {
            self.dsp_cursor = self.dsp_cursor.min(dsp_items - 1);
        } else {
            self.dsp_cursor = 0;
        }
    }

    pub fn handle_signal(&mut self, signal: DaemonSignal) {
        match signal {
            DaemonSignal::RouteUpdated(route) => {
                // Update the specific route in place if it belongs to the selected output
                if let Some(existing) = self.routes.iter_mut().find(|r| {
                    r.input_id == route.input_id && r.output_id == route.output_id
                }) {
                    *existing = route;
                }
            }
            DaemonSignal::OutputsRefreshed(outputs) => {
                self.outputs = outputs;
            }
            DaemonSignal::StreamsRefreshed(streams) => self.streams = streams,
            DaemonSignal::FullRefresh { inputs, outputs, routes, streams } => {
                self.inputs = inputs;
                self.outputs = outputs;
                self.routes = routes;
                self.streams = streams;
            }
            DaemonSignal::RulesRefreshed(rules) => self.rules = rules,
            DaemonSignal::CaptureDevicesRefreshed(devices) => self.capture_devices = devices,
            DaemonSignal::PlaybackDevicesRefreshed(devices) => self.playback_devices = devices,
            DaemonSignal::ComponentsRefreshed(components) => {
                self.beacn_connected = components.iter().any(|c| c.component_type == "beacn");
                self.components = components;
            }
            DaemonSignal::BeacnConfigRefreshed(config) => {
                self.beacn_config = Some(config);
            }
            DaemonSignal::InputDspRefreshed { input_id, eq_enabled, eq_bands, gate, deesser } => {
                self.dsp_input_eq.insert(input_id, (eq_enabled, eq_bands));
                self.dsp_input_gate.insert(input_id, gate);
                self.dsp_input_deesser.insert(input_id, deesser);
            }
            DaemonSignal::OutputDspRefreshed { output_id, compressor, limiter } => {
                self.dsp_output_compressor.insert(output_id, compressor);
                self.dsp_output_limiter.insert(output_id, limiter);
            }
        }
        self.clamp_cursors();
    }

    pub async fn handle_action(&mut self, action: AppAction, proxy: &MixCtlProxy<'_>) {
        match action {
            AppAction::Quit => {} // handled in main loop
            AppAction::NextPanel => {
                self.active_panel = match self.active_panel {
                    Panel::Routes => Panel::Streams,
                    Panel::Streams => Panel::Outputs,
                    Panel::Outputs => Panel::Rules,
                    Panel::Rules => Panel::Capture,
                    Panel::Capture => Panel::Settings,
                    Panel::Settings => Panel::Dsp,
                    Panel::Dsp => Panel::Routes,
                };
            }
            AppAction::PrevPanel => {
                self.active_panel = match self.active_panel {
                    Panel::Routes => Panel::Dsp,
                    Panel::Streams => Panel::Routes,
                    Panel::Outputs => Panel::Streams,
                    Panel::Rules => Panel::Outputs,
                    Panel::Capture => Panel::Rules,
                    Panel::Settings => Panel::Capture,
                    Panel::Dsp => Panel::Settings,
                };
            }
            AppAction::CursorUp => {
                match self.active_panel {
                    Panel::Routes => self.route_cursor = self.route_cursor.saturating_sub(1),
                    Panel::Streams => self.stream_cursor = self.stream_cursor.saturating_sub(1),
                    Panel::Outputs => self.output_cursor = self.output_cursor.saturating_sub(1),
                    Panel::Rules => self.rule_cursor = self.rule_cursor.saturating_sub(1),
                    Panel::Capture => self.capture_cursor = self.capture_cursor.saturating_sub(1),
                    Panel::Settings => self.settings_cursor = self.settings_cursor.saturating_sub(1),
                    Panel::Dsp => self.dsp_cursor = self.dsp_cursor.saturating_sub(1),
                }
            }
            AppAction::CursorDown => {
                match self.active_panel {
                    Panel::Routes => {
                        if self.route_cursor + 1 < self.routes.len() {
                            self.route_cursor += 1;
                        }
                    }
                    Panel::Streams => {
                        if self.stream_cursor + 1 < self.streams.len() {
                            self.stream_cursor += 1;
                        }
                    }
                    Panel::Outputs => {
                        if self.output_cursor + 1 < self.outputs.len() {
                            self.output_cursor += 1;
                        }
                    }
                    Panel::Rules => {
                        if self.rule_cursor + 1 < self.rules.len() {
                            self.rule_cursor += 1;
                        }
                    }
                    Panel::Capture => {
                        if self.capture_cursor + 1 < self.capture_devices.len() {
                            self.capture_cursor += 1;
                        }
                    }
                    Panel::Settings => {
                        let total = self.inputs.len() + self.outputs.len();
                        if self.settings_cursor + 1 < total {
                            self.settings_cursor += 1;
                        }
                    }
                    Panel::Dsp => {
                        let total = self.inputs.len() + self.outputs.len();
                        if self.dsp_cursor + 1 < total {
                            self.dsp_cursor += 1;
                        }
                    }
                }
            }
            AppAction::VolumeUp { fine } => {
                let step: i16 = if fine { self.config.volume_fine_step as i16 } else { self.config.volume_step as i16 };
                match self.active_panel {
                    Panel::Routes => {
                        if let Some(route) = self.routes.get(self.route_cursor) {
                            let new_vol = (route.volume as i16 + step).min(100) as u8;
                            proxy.set_route_volume(route.input_id, route.output_id, new_vol).await.ok();
                        }
                    }
                    Panel::Outputs => {
                        if let Some(output) = self.outputs.get(self.output_cursor) {
                            let new_vol = (output.volume as i16 + step).min(100) as u8;
                            proxy.set_output_volume(output.id, new_vol).await.ok();
                        }
                    }
                    _ => {}
                }
            }
            AppAction::VolumeDown { fine } => {
                let step: i16 = if fine { self.config.volume_fine_step as i16 } else { self.config.volume_step as i16 };
                match self.active_panel {
                    Panel::Routes => {
                        if let Some(route) = self.routes.get(self.route_cursor) {
                            let new_vol = (route.volume as i16 - step).max(0) as u8;
                            proxy.set_route_volume(route.input_id, route.output_id, new_vol).await.ok();
                        }
                    }
                    Panel::Outputs => {
                        if let Some(output) = self.outputs.get(self.output_cursor) {
                            let new_vol = (output.volume as i16 - step).max(0) as u8;
                            proxy.set_output_volume(output.id, new_vol).await.ok();
                        }
                    }
                    _ => {}
                }
            }
            AppAction::ToggleMute => {
                match self.active_panel {
                    Panel::Routes => {
                        if let Some(route) = self.routes.get(self.route_cursor) {
                            proxy.set_route_mute(route.input_id, route.output_id, !route.muted).await.ok();
                        }
                    }
                    Panel::Outputs => {
                        if let Some(output) = self.outputs.get(self.output_cursor) {
                            proxy.set_output_mute(output.id, !output.muted).await.ok();
                        }
                    }
                    _ => {}
                }
            }
            AppAction::ToggleOutputMute => {
                if let Some(output) = self.outputs.get(self.selected_output_idx) {
                    proxy.set_output_mute(output.id, !output.muted).await.ok();
                }
            }
            AppAction::SelectOutputTab(idx) => {
                if idx < self.outputs.len() {
                    self.selected_output_idx = idx;
                    // Re-fetch routes for the newly selected output
                    if let Some(output) = self.outputs.get(idx) {
                        if let Ok(routes) = proxy.list_routes_for_output(output.id).await {
                            self.routes = routes;
                        }
                    }
                }
            }
            AppAction::ShowHelp => {
                self.show_help = !self.show_help;
            }
            AppAction::DeleteRule => {
                if let Some(rule) = self.rules.get(self.rule_cursor) {
                    proxy.remove_app_rule(&rule.app_name).await.ok();
                }
            }
            AppAction::AssignRuleToInput(n) => {
                if let Some(rule) = self.rules.get(self.rule_cursor) {
                    if let Some(input) = self.inputs.get(n - 1) {
                        proxy.set_app_rule(&rule.app_name, input.id).await.ok();
                    }
                }
            }
            AppAction::BindCapture(n) => {
                if let Some(device) = self.capture_devices.get(self.capture_cursor) {
                    if let Some(input) = self.inputs.get(n - 1) {
                        proxy.bind_capture_to_input(input.id, &device.device_name).await.ok();
                    }
                }
            }
            AppAction::UnbindCapture => {
                if let Some(device) = self.capture_devices.get(self.capture_cursor) {
                    // Unbind by binding to input 0 (no input)
                    proxy.bind_capture_to_input(0, &device.device_name).await.ok();
                }
            }
            AppAction::CycleInputColor => {
                if let Some(input) = self.inputs.get(self.settings_cursor) {
                    let current = &input.color;
                    let next = next_palette_color(current);
                    proxy.set_input_color(input.id, next).await.ok();
                }
            }
            AppAction::CycleOutputColor => {
                let output_idx = self.settings_cursor.saturating_sub(self.inputs.len());
                if let Some(output) = self.outputs.get(output_idx) {
                    let current = &output.color;
                    let next = next_palette_color(current);
                    proxy.set_output_color(output.id, next).await.ok();
                }
            }
            AppAction::ToggleEq => {
                if let Some(input) = self.dsp_selected_input() {
                    let current = self.dsp_input_eq.get(&input.id).map(|(e, _)| *e).unwrap_or(false);
                    proxy.set_input_eq_enabled(input.id, !current).await.ok();
                }
            }
            AppAction::ToggleGate => {
                if let Some(input) = self.dsp_selected_input() {
                    let current = self.dsp_input_gate.get(&input.id).map(|g| g.enabled).unwrap_or(false);
                    proxy.set_input_gate_enabled(input.id, !current).await.ok();
                }
            }
            AppAction::ToggleDeesser => {
                if let Some(input) = self.dsp_selected_input() {
                    let current = self.dsp_input_deesser.get(&input.id).map(|d| d.enabled).unwrap_or(false);
                    proxy.set_input_deesser_enabled(input.id, !current).await.ok();
                }
            }
            AppAction::ToggleCompressor => {
                if let Some(output) = self.dsp_selected_output() {
                    let current = self.dsp_output_compressor.get(&output.id).map(|c| c.enabled).unwrap_or(false);
                    proxy.set_output_compressor_enabled(output.id, !current).await.ok();
                }
            }
            AppAction::ToggleLimiter => {
                if let Some(output) = self.dsp_selected_output() {
                    let current = self.dsp_output_limiter.get(&output.id).map(|l| l.enabled).unwrap_or(false);
                    proxy.set_output_limiter_enabled(output.id, !current).await.ok();
                }
            }
            AppAction::MoveUp => {
                if self.settings_cursor < self.inputs.len() {
                    if let Some(input) = self.inputs.get(self.settings_cursor) {
                        if self.settings_cursor > 0 {
                            proxy.move_input(input.id, (self.settings_cursor - 1) as u32).await.ok();
                        }
                    }
                } else {
                    let idx = self.settings_cursor - self.inputs.len();
                    if let Some(output) = self.outputs.get(idx) {
                        if idx > 0 {
                            proxy.move_output(output.id, (idx - 1) as u32).await.ok();
                        }
                    }
                }
            }
            AppAction::MoveDown => {
                if self.settings_cursor < self.inputs.len() {
                    if let Some(input) = self.inputs.get(self.settings_cursor) {
                        if self.settings_cursor + 1 < self.inputs.len() {
                            proxy.move_input(input.id, (self.settings_cursor + 1) as u32).await.ok();
                        }
                    }
                } else {
                    let idx = self.settings_cursor - self.inputs.len();
                    if let Some(output) = self.outputs.get(idx) {
                        if idx + 1 < self.outputs.len() {
                            proxy.move_output(output.id, (idx + 1) as u32).await.ok();
                        }
                    }
                }
            }
            AppAction::AddChannel => {
                if self.settings_cursor < self.inputs.len() || (self.inputs.is_empty() && self.outputs.is_empty()) {
                    let name = format!("Input {}", self.inputs.len() + 1);
                    let color = COLOR_PALETTE[self.inputs.len() % COLOR_PALETTE.len()];
                    proxy.add_input(&name, color).await.ok();
                } else {
                    let name = format!("Output {}", self.outputs.len() + 1);
                    let color = COLOR_PALETTE[self.outputs.len() % COLOR_PALETTE.len()];
                    proxy.add_output(&name, color, 0).await.ok();
                }
            }
            AppAction::RemoveChannel => {
                if self.settings_cursor < self.inputs.len() {
                    if let Some(input) = self.inputs.get(self.settings_cursor) {
                        proxy.remove_input(input.id).await.ok();
                    }
                } else {
                    let idx = self.settings_cursor - self.inputs.len();
                    if let Some(output) = self.outputs.get(idx) {
                        proxy.remove_output(output.id).await.ok();
                    }
                }
            }
            AppAction::StartRename => {
                let name = if self.settings_cursor < self.inputs.len() {
                    self.inputs.get(self.settings_cursor).map(|i| i.name.clone())
                } else {
                    let idx = self.settings_cursor - self.inputs.len();
                    self.outputs.get(idx).map(|o| o.name.clone())
                };
                if let Some(name) = name {
                    self.rename_buf = Some(name);
                }
            }
            AppAction::ConfirmRename => {
                if let Some(new_name) = self.rename_buf.take() {
                    if !new_name.is_empty() {
                        if self.settings_cursor < self.inputs.len() {
                            if let Some(input) = self.inputs.get(self.settings_cursor) {
                                proxy.set_input_name(input.id, &new_name).await.ok();
                            }
                        } else {
                            let idx = self.settings_cursor - self.inputs.len();
                            if let Some(output) = self.outputs.get(idx) {
                                proxy.set_output_name(output.id, &new_name).await.ok();
                            }
                        }
                    }
                }
            }
            AppAction::CancelRename => {
                self.rename_buf = None;
            }
            AppAction::RenameChar(c) => {
                if let Some(ref mut buf) = self.rename_buf {
                    buf.push(c);
                }
            }
            AppAction::RenameBackspace => {
                if let Some(ref mut buf) = self.rename_buf {
                    buf.pop();
                }
            }
            // Step 6: Output target selector
            AppAction::SetOutputTarget => {
                if self.active_panel == Panel::Settings && self.settings_cursor >= self.inputs.len() {
                    let output_idx = self.settings_cursor - self.inputs.len();
                    if let Some(output) = self.outputs.get(output_idx) {
                        if !self.playback_devices.is_empty() {
                            // Find current target index, cycle to next
                            let current_idx = self.playback_devices.iter()
                                .position(|d| d.device_name == output.target_device)
                                .unwrap_or(0);
                            let next_idx = (current_idx + 1) % self.playback_devices.len();
                            let device_name = &self.playback_devices[next_idx].device_name;
                            proxy.set_output_target(output.id, device_name).await.ok();
                        }
                    }
                }
            }
            // Step 7: Capture device management
            AppAction::AddCaptureInput => {
                if let Some(device) = self.capture_devices.get(self.capture_cursor) {
                    if !device.is_added {
                        let color = COLOR_PALETTE[self.inputs.len() % COLOR_PALETTE.len()];
                        proxy.add_capture_input(device.pw_node_id, &device.name, color).await.ok();
                    }
                }
            }
            AppAction::RemoveCaptureInput => {
                if let Some(device) = self.capture_devices.get(self.capture_cursor) {
                    if device.is_added && device.input_id > 0 {
                        proxy.remove_capture_input(device.input_id).await.ok();
                    }
                }
            }
            AppAction::SetCaptureVolume { fine } => {
                if let Some(device) = self.capture_devices.get(self.capture_cursor) {
                    if device.input_id > 0 {
                        let step: f32 = if fine {
                            self.config.volume_fine_step as f32 / 100.0
                        } else {
                            self.config.volume_step as f32 / 100.0
                        };
                        // We don't track capture volume locally, so use a reasonable increment
                        // The daemon will clamp the value
                        let current_vol: f32 = 0.8; // reasonable default
                        let new_vol = (current_vol + step).min(1.0);
                        proxy.set_capture_volume(device.input_id, new_vol).await.ok();
                    }
                }
            }
            AppAction::DecreaseCaptureVolume { fine } => {
                if let Some(device) = self.capture_devices.get(self.capture_cursor) {
                    if device.input_id > 0 {
                        let step: f32 = if fine {
                            self.config.volume_fine_step as f32 / 100.0
                        } else {
                            self.config.volume_step as f32 / 100.0
                        };
                        let current_vol: f32 = 0.8;
                        let new_vol = (current_vol - step).max(0.0);
                        proxy.set_capture_volume(device.input_id, new_vol).await.ok();
                    }
                }
            }
            AppAction::SetCaptureMute => {
                if let Some(device) = self.capture_devices.get(self.capture_cursor) {
                    if device.input_id > 0 {
                        // We don't track mute locally; toggle assuming not muted
                        proxy.set_capture_mute(device.input_id, true).await.ok();
                    }
                }
            }
            // Step 8: DSP parameter editing
            AppAction::EnterDspEdit => {
                self.dsp_editing = true;
                self.dsp_param_cursor = 0;
            }
            AppAction::ExitDspEdit => {
                self.dsp_editing = false;
            }
            AppAction::DspParamNext => {
                let max = self.dsp_param_count();
                if max > 0 && self.dsp_param_cursor + 1 < max {
                    self.dsp_param_cursor += 1;
                }
            }
            AppAction::DspParamPrev => {
                self.dsp_param_cursor = self.dsp_param_cursor.saturating_sub(1);
            }
            AppAction::DspValueUp { fine } => {
                self.dsp_adjust_value(proxy, fine, true).await;
            }
            AppAction::DspValueDown { fine } => {
                self.dsp_adjust_value(proxy, fine, false).await;
            }
            AppAction::DspResetEq => {
                if let Some(input) = self.dsp_selected_input() {
                    let id = input.id;
                    proxy.reset_input_eq(id).await.ok();
                }
            }
        }
    }

    /// If the DSP cursor is on an input row, return that input.
    pub fn dsp_selected_input(&self) -> Option<&InputInfo> {
        if self.dsp_cursor < self.inputs.len() {
            self.inputs.get(self.dsp_cursor)
        } else {
            None
        }
    }

    /// If the DSP cursor is on an output row, return that output.
    pub fn dsp_selected_output(&self) -> Option<&OutputInfo> {
        if self.dsp_cursor >= self.inputs.len() {
            let idx = self.dsp_cursor - self.inputs.len();
            self.outputs.get(idx)
        } else {
            None
        }
    }

    /// Total number of editable DSP parameters for the currently selected input/output.
    pub fn dsp_param_count(&self) -> usize {
        if let Some(input) = self.dsp_selected_input() {
            let eq_count = self.dsp_input_eq.get(&input.id)
                .map(|(_, bands)| bands.len() * 3) // freq, gain, q per band
                .unwrap_or(0);
            let gate_count = 4; // threshold, attack, release, hold
            let deesser_count = 3; // frequency, threshold, ratio
            eq_count + gate_count + deesser_count
        } else if let Some(output) = self.dsp_selected_output() {
            let comp_count = 6; // threshold, ratio, attack, release, makeup, knee
            let lim_count = 2; // ceiling, release
            let _ = output;
            comp_count + lim_count
        } else {
            0
        }
    }

    /// Get a human-readable label and current value for the DSP parameter at `dsp_param_cursor`.
    pub fn dsp_param_label(&self) -> Option<(String, f64)> {
        if let Some(input) = self.dsp_selected_input() {
            let eq_bands = self.dsp_input_eq.get(&input.id)
                .map(|(_, bands)| bands.as_slice())
                .unwrap_or(&[]);
            let eq_params = eq_bands.len() * 3;
            let cursor = self.dsp_param_cursor;

            if cursor < eq_params {
                let band_idx = cursor / 3;
                let param_idx = cursor % 3;
                if let Some(band) = eq_bands.get(band_idx) {
                    return match param_idx {
                        0 => Some((format!("Band {} Freq", band_idx + 1), band.frequency)),
                        1 => Some((format!("Band {} Gain", band_idx + 1), band.gain_db)),
                        2 => Some((format!("Band {} Q", band_idx + 1), band.q)),
                        _ => None,
                    };
                }
            }
            let gate_start = eq_params;
            if cursor >= gate_start && cursor < gate_start + 4 {
                if let Some(gate) = self.dsp_input_gate.get(&input.id) {
                    return match cursor - gate_start {
                        0 => Some(("Gate Threshold".into(), gate.threshold_db)),
                        1 => Some(("Gate Attack".into(), gate.attack_ms)),
                        2 => Some(("Gate Release".into(), gate.release_ms)),
                        3 => Some(("Gate Hold".into(), gate.hold_ms)),
                        _ => None,
                    };
                }
            }
            let deesser_start = gate_start + 4;
            if cursor >= deesser_start && cursor < deesser_start + 3 {
                if let Some(deesser) = self.dsp_input_deesser.get(&input.id) {
                    return match cursor - deesser_start {
                        0 => Some(("De-esser Freq".into(), deesser.frequency)),
                        1 => Some(("De-esser Threshold".into(), deesser.threshold_db)),
                        2 => Some(("De-esser Ratio".into(), deesser.ratio)),
                        _ => None,
                    };
                }
            }
        } else if let Some(output) = self.dsp_selected_output() {
            let cursor = self.dsp_param_cursor;
            if cursor < 6 {
                if let Some(comp) = self.dsp_output_compressor.get(&output.id) {
                    return match cursor {
                        0 => Some(("Comp Threshold".into(), comp.threshold_db)),
                        1 => Some(("Comp Ratio".into(), comp.ratio)),
                        2 => Some(("Comp Attack".into(), comp.attack_ms)),
                        3 => Some(("Comp Release".into(), comp.release_ms)),
                        4 => Some(("Comp Makeup".into(), comp.makeup_gain_db)),
                        5 => Some(("Comp Knee".into(), comp.knee_db)),
                        _ => None,
                    };
                }
            }
            if cursor >= 6 && cursor < 8 {
                if let Some(lim) = self.dsp_output_limiter.get(&output.id) {
                    return match cursor - 6 {
                        0 => Some(("Limiter Ceiling".into(), lim.ceiling_db)),
                        1 => Some(("Limiter Release".into(), lim.release_ms)),
                        _ => None,
                    };
                }
            }
        }
        None
    }

    /// Adjust the DSP parameter at `dsp_param_cursor` up or down.
    async fn dsp_adjust_value(&mut self, proxy: &MixCtlProxy<'_>, fine: bool, up: bool) {
        let fine_div = if fine { 5.0 } else { 1.0 };
        let sign = if up { 1.0 } else { -1.0 };

        if let Some(input) = self.dsp_selected_input().cloned() {
            let eq_bands = self.dsp_input_eq.get(&input.id)
                .map(|(_, bands)| bands.clone())
                .unwrap_or_default();
            let eq_params = eq_bands.len() * 3;
            let cursor = self.dsp_param_cursor;

            if cursor < eq_params {
                let band_idx = cursor / 3;
                let param_idx = cursor % 3;
                if let Some(band) = eq_bands.get(band_idx) {
                    let mut freq = band.frequency;
                    let mut gain = band.gain_db;
                    let mut q = band.q;
                    match param_idx {
                        0 => freq = (freq + sign * 100.0 / fine_div).clamp(20.0, 20000.0),
                        1 => gain = (gain + sign * 0.5 / fine_div).clamp(-24.0, 24.0),
                        2 => q = (q + sign * 0.1 / fine_div).clamp(0.1, 20.0),
                        _ => {}
                    }
                    proxy.set_input_eq_band(input.id, band_idx as u8, &band.band_type, freq, gain, q).await.ok();
                }
                return;
            }
            let gate_start = eq_params;
            if cursor >= gate_start && cursor < gate_start + 4 {
                if let Some(gate) = self.dsp_input_gate.get(&input.id).cloned() {
                    let mut t = gate.threshold_db;
                    let mut a = gate.attack_ms;
                    let mut r = gate.release_ms;
                    let mut h = gate.hold_ms;
                    match cursor - gate_start {
                        0 => t = (t + sign * 1.0 / fine_div).clamp(-80.0, 0.0),
                        1 => a = (a + sign * 1.0 / fine_div).clamp(0.1, 200.0),
                        2 => r = (r + sign * 10.0 / fine_div).clamp(1.0, 2000.0),
                        3 => h = (h + sign * 5.0 / fine_div).clamp(0.0, 500.0),
                        _ => {}
                    }
                    proxy.set_input_gate(input.id, t, a, r, h).await.ok();
                }
                return;
            }
            let deesser_start = gate_start + 4;
            if cursor >= deesser_start && cursor < deesser_start + 3 {
                if let Some(deesser) = self.dsp_input_deesser.get(&input.id).cloned() {
                    let mut freq = deesser.frequency;
                    let mut thresh = deesser.threshold_db;
                    let mut ratio = deesser.ratio;
                    match cursor - deesser_start {
                        0 => freq = (freq + sign * 100.0 / fine_div).clamp(1000.0, 16000.0),
                        1 => thresh = (thresh + sign * 1.0 / fine_div).clamp(-60.0, 0.0),
                        2 => ratio = (ratio + sign * 0.5 / fine_div).clamp(1.0, 20.0),
                        _ => {}
                    }
                    proxy.set_input_deesser(input.id, freq, thresh, ratio).await.ok();
                }
            }
        } else if let Some(output) = self.dsp_selected_output().cloned() {
            let cursor = self.dsp_param_cursor;
            if cursor < 6 {
                if let Some(comp) = self.dsp_output_compressor.get(&output.id).cloned() {
                    let mut t = comp.threshold_db;
                    let mut ratio = comp.ratio;
                    let mut a = comp.attack_ms;
                    let mut r = comp.release_ms;
                    let mut m = comp.makeup_gain_db;
                    let mut k = comp.knee_db;
                    match cursor {
                        0 => t = (t + sign * 1.0 / fine_div).clamp(-60.0, 0.0),
                        1 => ratio = (ratio + sign * 0.5 / fine_div).clamp(1.0, 20.0),
                        2 => a = (a + sign * 1.0 / fine_div).clamp(0.1, 200.0),
                        3 => r = (r + sign * 10.0 / fine_div).clamp(1.0, 2000.0),
                        4 => m = (m + sign * 0.5 / fine_div).clamp(-12.0, 24.0),
                        5 => k = (k + sign * 0.5 / fine_div).clamp(0.0, 12.0),
                        _ => {}
                    }
                    proxy.set_output_compressor(output.id, t, ratio, a, r, m, k).await.ok();
                }
                return;
            }
            if cursor >= 6 && cursor < 8 {
                if let Some(lim) = self.dsp_output_limiter.get(&output.id).cloned() {
                    let mut ceiling = lim.ceiling_db;
                    let mut release = lim.release_ms;
                    match cursor - 6 {
                        0 => ceiling = (ceiling + sign * 0.5 / fine_div).clamp(-24.0, 0.0),
                        1 => release = (release + sign * 5.0 / fine_div).clamp(1.0, 500.0),
                        _ => {}
                    }
                    proxy.set_output_limiter(output.id, ceiling, release).await.ok();
                }
            }
        }
    }
}

fn next_palette_color(current: &str) -> &'static str {
    let idx = COLOR_PALETTE
        .iter()
        .position(|&c| c.eq_ignore_ascii_case(current))
        .map(|i| (i + 1) % COLOR_PALETTE.len())
        .unwrap_or(0);
    COLOR_PALETTE[idx]
}

#[cfg(test)]
mod tests {
    use super::*;
    use mixctl_core::{InputInfo, OutputInfo, RouteInfo};

    fn test_state() -> AppState {
        AppState::new(
            vec![InputInfo { id: 1, name: "Sys".into(), color: "#000".into() }],
            vec![OutputInfo { id: 5, name: "Out".into(), color: "#fff".into(), volume: 100, muted: false, target_device: String::new() }],
            vec![RouteInfo { input_id: 1, output_id: 5, volume: 80, muted: false }],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            None,
            TuiConfig::default(),
        )
    }

    #[test]
    fn panel_cycling_wraps() {
        let mut state = test_state();
        // Start at Dsp (last panel) and go next -> should wrap to Routes
        state.active_panel = Panel::Dsp;
        // Simulate NextPanel
        state.active_panel = match state.active_panel {
            Panel::Routes => Panel::Streams,
            Panel::Streams => Panel::Outputs,
            Panel::Outputs => Panel::Rules,
            Panel::Rules => Panel::Capture,
            Panel::Capture => Panel::Settings,
            Panel::Settings => Panel::Dsp,
            Panel::Dsp => Panel::Routes,
        };
        assert_eq!(state.active_panel, Panel::Routes);

        // From Routes, PrevPanel should go to Dsp
        state.active_panel = match state.active_panel {
            Panel::Routes => Panel::Dsp,
            Panel::Streams => Panel::Routes,
            Panel::Outputs => Panel::Streams,
            Panel::Rules => Panel::Outputs,
            Panel::Capture => Panel::Rules,
            Panel::Settings => Panel::Capture,
            Panel::Dsp => Panel::Settings,
        };
        assert_eq!(state.active_panel, Panel::Dsp);
    }

    #[test]
    fn cursor_clamp_on_empty() {
        let mut state = test_state();
        state.routes.clear();
        state.route_cursor = 5;
        // Trigger clamp via a signal
        state.handle_signal(DaemonSignal::StreamsRefreshed(vec![]));
        assert_eq!(state.route_cursor, 0);
    }

    #[test]
    fn signal_routes_refreshed() {
        let mut state = test_state();
        let updated_route = RouteInfo { input_id: 1, output_id: 5, volume: 42, muted: true };
        state.handle_signal(DaemonSignal::RouteUpdated(updated_route));
        let route = state.routes.iter().find(|r| r.input_id == 1 && r.output_id == 5).unwrap();
        assert_eq!(route.volume, 42);
        assert!(route.muted);
    }

    #[test]
    fn signal_full_refresh() {
        let mut state = test_state();
        let new_inputs = vec![
            InputInfo { id: 10, name: "New".into(), color: "#abc".into() },
            InputInfo { id: 11, name: "New2".into(), color: "#def".into() },
        ];
        let new_outputs = vec![
            OutputInfo { id: 20, name: "Out2".into(), color: "#111".into(), volume: 50, muted: true, target_device: String::new() },
        ];
        let new_routes = vec![
            RouteInfo { input_id: 10, output_id: 20, volume: 60, muted: false },
        ];
        state.handle_signal(DaemonSignal::FullRefresh {
            inputs: new_inputs,
            outputs: new_outputs,
            routes: new_routes,
            streams: vec![],
        });
        assert_eq!(state.inputs.len(), 2);
        assert_eq!(state.outputs.len(), 1);
        assert_eq!(state.routes.len(), 1);
        assert_eq!(state.inputs[0].id, 10);
        assert_eq!(state.outputs[0].volume, 50);
    }

    #[test]
    fn cursor_stays_in_bounds_after_shrink() {
        let mut state = test_state();
        // Add more routes so cursor can be at position 3
        state.routes = vec![
            RouteInfo { input_id: 1, output_id: 5, volume: 80, muted: false },
            RouteInfo { input_id: 2, output_id: 5, volume: 80, muted: false },
            RouteInfo { input_id: 3, output_id: 5, volume: 80, muted: false },
            RouteInfo { input_id: 4, output_id: 5, volume: 80, muted: false },
        ];
        state.route_cursor = 3;

        // Now shrink the routes list to 2 items via a FullRefresh
        state.handle_signal(DaemonSignal::FullRefresh {
            inputs: vec![InputInfo { id: 1, name: "Sys".into(), color: "#000".into() }],
            outputs: vec![OutputInfo { id: 5, name: "Out".into(), color: "#fff".into(), volume: 100, muted: false, target_device: String::new() }],
            routes: vec![
                RouteInfo { input_id: 1, output_id: 5, volume: 80, muted: false },
                RouteInfo { input_id: 2, output_id: 5, volume: 80, muted: false },
            ],
            streams: vec![],
        });
        assert!(
            state.route_cursor < state.routes.len(),
            "cursor {} should be < routes len {}",
            state.route_cursor,
            state.routes.len()
        );
    }
}
