use std::collections::HashMap;

use mixctl_core::config_sections::BeacnConfig;
use mixctl_core::dbus::MixCtlProxy;
use mixctl_core::{
    AppRuleInfo, CaptureDeviceInfo, CompressorInfo, ComponentInfo, DeesserInfo,
    EqBandInfo, GateInfo, InputInfo, LimiterInfo, OutputInfo, RouteInfo, StreamInfo,
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
    pub components: Vec<ComponentInfo>,

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

    // DSP state caches
    pub dsp_input_eq: HashMap<u32, (bool, Vec<EqBandInfo>)>,
    pub dsp_input_gate: HashMap<u32, GateInfo>,
    pub dsp_input_deesser: HashMap<u32, DeesserInfo>,
    pub dsp_output_compressor: HashMap<u32, CompressorInfo>,
    pub dsp_output_limiter: HashMap<u32, LimiterInfo>,
    pub dsp_cursor: usize,
}

impl AppState {
    pub fn new(
        inputs: Vec<InputInfo>,
        outputs: Vec<OutputInfo>,
        routes: Vec<RouteInfo>,
        streams: Vec<StreamInfo>,
        rules: Vec<AppRuleInfo>,
        capture_devices: Vec<CaptureDeviceInfo>,
        components: Vec<ComponentInfo>,
        beacn_config: Option<BeacnConfig>,
    ) -> Self {
        let beacn_connected = components.iter().any(|c| c.component_type == "beacn");
        Self {
            inputs,
            outputs,
            routes,
            streams,
            rules,
            capture_devices,
            components,
            active_panel: Panel::Routes,
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

            dsp_input_eq: HashMap::new(),
            dsp_input_gate: HashMap::new(),
            dsp_input_deesser: HashMap::new(),
            dsp_output_compressor: HashMap::new(),
            dsp_output_limiter: HashMap::new(),
            dsp_cursor: 0,
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
                let step: i16 = if fine { 1 } else { 5 };
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
                let step: i16 = if fine { 1 } else { 5 };
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
            None,
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
