use mixctl_core::dbus::MixCtlProxy;
use mixctl_core::{AppRuleInfo, CaptureDeviceInfo, ComponentInfo, OutputInfo, RouteInfo, StreamInfo, InputInfo};

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
    pub show_help: bool,
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
            show_help: false,
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
                    Panel::Settings => Panel::Routes,
                };
            }
            AppAction::PrevPanel => {
                self.active_panel = match self.active_panel {
                    Panel::Routes => Panel::Settings,
                    Panel::Streams => Panel::Routes,
                    Panel::Outputs => Panel::Streams,
                    Panel::Rules => Panel::Outputs,
                    Panel::Capture => Panel::Rules,
                    Panel::Settings => Panel::Capture,
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
