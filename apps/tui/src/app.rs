use std::collections::HashMap;

use mixctl_core::config_sections::{BeacnConfig, ButtonAction, ButtonMapping, ButtonMappings, TuiConfig};
use mixctl_core::dbus::MixCtlProxy;
use mixctl_core::{
    AppRuleInfo, CaptureDeviceInfo, CompressorInfo, ComponentInfo, CustomInputInfo,
    DeesserInfo, EqBandInfo, GateInfo, InputInfo, LimiterInfo, OutputInfo,
    PlaybackDeviceInfo, RouteInfo, StreamInfo,
};

/// Colour palette for cycling input/output colours.
pub const COLOR_PALETTE: &[&str] = &[
    "#4A90D9", "#E74C3C", "#2ECC71", "#F39C12", "#8E44AD",
    "#3498DB", "#E67E22", "#1ABC9C", "#9B59B6", "#27AE60",
];

// ---------------------------------------------------------------------------
// Overlay / focus model (replaces Panel enum)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    None,
    Dsp,
    Settings,
    Profiles,
    Beacn,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusArea {
    Matrix,
    Streams,
    Capture,
    Playback,
    Rules,
}

impl FocusArea {
    pub fn next(self) -> Self {
        match self {
            Self::Matrix => Self::Streams,
            Self::Streams => Self::Capture,
            Self::Capture => Self::Playback,
            Self::Playback => Self::Rules,
            Self::Rules => Self::Matrix,
        }
    }
    pub fn prev(self) -> Self {
        match self {
            Self::Matrix => Self::Rules,
            Self::Streams => Self::Matrix,
            Self::Capture => Self::Streams,
            Self::Playback => Self::Capture,
            Self::Rules => Self::Playback,
        }
    }
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum AppAction {
    Quit,
    NextFocus,
    PrevFocus,
    // Matrix navigation
    CursorUp,
    CursorDown,
    CursorLeft,
    CursorRight,
    // Volume
    VolumeUp { fine: bool },
    VolumeDown { fine: bool },
    ToggleMute,
    // Overlays
    OpenDsp,
    OpenSettings,
    OpenProfiles,
    ToggleHelp,
    CloseOverlay,
    // Channel management (in settings overlay)
    AddChannel,
    RemoveChannel,
    StartRename,
    ConfirmRename,
    CancelRename,
    RenameChar(char),
    RenameBackspace,
    CycleColor,
    SetOutputTarget,
    MoveUp,
    MoveDown,
    SetDefault,
    // Footer actions
    FooterUp,
    FooterDown,
    AssignToInput(usize),
    DeleteItem,
    UnbindItem,
    // DSP (in DSP overlay)
    ToggleEq,
    ToggleGate,
    ToggleDeesser,
    ToggleCompressor,
    ToggleLimiter,
    DspResetEq,
    EnterDspEdit,
    ExitDspEdit,
    DspValueUp { fine: bool },
    DspValueDown { fine: bool },
    DspParamNext,
    DspParamPrev,
    DspCursorUp,
    DspCursorDown,
    // Profile actions (in profile overlay)
    ProfileSave,
    ProfileLoad,
    ProfileDelete,
    ProfileNameChar(char),
    ProfileNameBackspace,
    ProfileConfirmName,
    ProfileCancelName,
    // Beacn overlay
    OpenBeacn,
    BeacnUp,
    BeacnDown,
    BeacnLeft,
    BeacnRight,
    BeacnToggleEdit,
    BeacnCycleAction,
    BeacnCycleActionBack,
}

// ---------------------------------------------------------------------------
// Daemon signals
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum DaemonSignal {
    RouteUpdated(RouteInfo),
    OutputsRefreshed(Vec<OutputInfo>),
    StreamsRefreshed(Vec<StreamInfo>),
    FullRefresh {
        inputs: Vec<InputInfo>,
        outputs: Vec<OutputInfo>,
        all_routes: Vec<RouteInfo>,
        streams: Vec<StreamInfo>,
        default_input: u32,
        default_output: u32,
        custom_inputs: Vec<CustomInputInfo>,
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
    ProfilesRefreshed(Vec<String>),
    CustomInputsRefreshed(Vec<CustomInputInfo>),
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

pub struct AppState {
    // Data
    pub inputs: Vec<InputInfo>,
    pub outputs: Vec<OutputInfo>,
    pub all_routes: Vec<RouteInfo>,
    pub streams: Vec<StreamInfo>,
    pub rules: Vec<AppRuleInfo>,
    pub capture_devices: Vec<CaptureDeviceInfo>,
    pub playback_devices: Vec<PlaybackDeviceInfo>,
    pub components: Vec<ComponentInfo>,
    pub default_input: u32,
    pub default_output: u32,
    pub profiles: Vec<String>,
    pub custom_inputs: Vec<CustomInputInfo>,

    // Config
    pub config: TuiConfig,
    pub beacn_connected: bool,
    pub beacn_config: Option<BeacnConfig>,

    // Navigation
    pub overlay: Overlay,
    pub focus: FocusArea,
    /// Matrix cursor: (row, col). row=0 is output header, 1..=N are input rows.
    /// col=0 is input label column, 1..=M are output columns.
    pub matrix_row: usize,
    pub matrix_col: usize,
    pub footer_cursor: usize,

    // Overlay state
    pub rename_buf: Option<String>,
    pub settings_cursor: usize,

    // DSP state caches
    pub dsp_input_eq: HashMap<u32, (bool, Vec<EqBandInfo>)>,
    pub dsp_input_gate: HashMap<u32, GateInfo>,
    pub dsp_input_deesser: HashMap<u32, DeesserInfo>,
    pub dsp_output_compressor: HashMap<u32, CompressorInfo>,
    pub dsp_output_limiter: HashMap<u32, LimiterInfo>,
    pub dsp_cursor: usize,
    pub dsp_editing: bool,
    pub dsp_param_cursor: usize,

    // Profile overlay state
    pub profile_cursor: usize,
    pub profile_name_buf: Option<String>,

    // Beacn overlay state
    pub beacn_cursor: usize,
    pub beacn_field: usize,
    pub beacn_editing: bool,
}

impl AppState {
    pub fn new(
        inputs: Vec<InputInfo>,
        outputs: Vec<OutputInfo>,
        all_routes: Vec<RouteInfo>,
        streams: Vec<StreamInfo>,
        rules: Vec<AppRuleInfo>,
        capture_devices: Vec<CaptureDeviceInfo>,
        playback_devices: Vec<PlaybackDeviceInfo>,
        components: Vec<ComponentInfo>,
        beacn_config: Option<BeacnConfig>,
        config: TuiConfig,
        default_input: u32,
        default_output: u32,
        profiles: Vec<String>,
        custom_inputs: Vec<CustomInputInfo>,
    ) -> Self {
        let beacn_connected = components.iter().any(|c| c.component_type.starts_with("beacn"));
        Self {
            inputs,
            outputs,
            all_routes,
            streams,
            rules,
            capture_devices,
            playback_devices,
            components,
            default_input,
            default_output,
            profiles,
            custom_inputs,
            config,
            beacn_connected,
            beacn_config,
            overlay: Overlay::None,
            focus: FocusArea::Matrix,
            matrix_row: 1, // start on first input row
            matrix_col: 1, // start on first output column
            footer_cursor: 0,
            rename_buf: None,
            settings_cursor: 0,
            dsp_input_eq: HashMap::new(),
            dsp_input_gate: HashMap::new(),
            dsp_input_deesser: HashMap::new(),
            dsp_output_compressor: HashMap::new(),
            dsp_output_limiter: HashMap::new(),
            dsp_cursor: 0,
            dsp_editing: false,
            dsp_param_cursor: 0,
            profile_cursor: 0,
            profile_name_buf: None,
            beacn_cursor: 0,
            beacn_field: 0,
            beacn_editing: false,
        }
    }

    // -- Matrix helpers --

    /// Get the route for the matrix cell at (input_row_1based, output_col_1based).
    pub fn route_at(&self, row: usize, col: usize) -> Option<&RouteInfo> {
        let input = self.inputs.get(row.checked_sub(1)?)?;
        let output = self.outputs.get(col.checked_sub(1)?)?;
        self.all_routes.iter().find(|r| r.input_id == input.id && r.output_id == output.id)
    }

    /// Get the input for a matrix row (1-based).
    pub fn input_at_row(&self, row: usize) -> Option<&InputInfo> {
        self.inputs.get(row.checked_sub(1)?)
    }

    /// Get the output for a matrix column (1-based).
    pub fn output_at_col(&self, col: usize) -> Option<&OutputInfo> {
        self.outputs.get(col.checked_sub(1)?)
    }

    /// Whether the given matrix row corresponds to a custom input.
    /// Custom input rows come after all regular input rows (1..=inputs.len()).
    pub fn is_custom_input_row(&self, row: usize) -> bool {
        row > self.inputs.len() && row <= self.inputs.len() + self.custom_inputs.len()
    }

    /// Get the custom input for a matrix row, if it is a custom input row.
    pub fn custom_input_at_row(&self, row: usize) -> Option<&CustomInputInfo> {
        let ci_idx = row.checked_sub(self.inputs.len() + 1)?;
        self.custom_inputs.get(ci_idx)
    }

    /// Number of items in the currently focused footer section.
    pub fn footer_item_count(&self) -> usize {
        match self.focus {
            FocusArea::Streams => self.streams.len(),
            FocusArea::Capture => self.capture_devices.len(),
            FocusArea::Playback => self.playback_devices.len(),
            FocusArea::Rules => self.rules.len(),
            FocusArea::Matrix => 0,
        }
    }

    fn clamp_cursors(&mut self) {
        let max_row = self.inputs.len() + self.custom_inputs.len(); // 0=header, 1..=N inputs, N+1..=N+C custom inputs
        let max_col = self.outputs.len();
        if max_row > 0 {
            self.matrix_row = self.matrix_row.clamp(0, max_row);
        } else {
            self.matrix_row = 0;
        }
        if max_col > 0 {
            self.matrix_col = self.matrix_col.clamp(0, max_col);
        } else {
            self.matrix_col = 0;
        }
        let footer_count = self.footer_item_count();
        if footer_count > 0 {
            self.footer_cursor = self.footer_cursor.min(footer_count - 1);
        } else {
            self.footer_cursor = 0;
        }
        let settings_total = self.inputs.len() + self.outputs.len();
        if settings_total > 0 {
            self.settings_cursor = self.settings_cursor.min(settings_total - 1);
        } else {
            self.settings_cursor = 0;
        }
        let dsp_total = self.inputs.len() + self.outputs.len();
        if dsp_total > 0 {
            self.dsp_cursor = self.dsp_cursor.min(dsp_total - 1);
        } else {
            self.dsp_cursor = 0;
        }
    }

    // -- Signal handling --

    pub fn handle_signal(&mut self, signal: DaemonSignal) {
        match signal {
            DaemonSignal::RouteUpdated(route) => {
                if let Some(existing) = self.all_routes.iter_mut().find(|r| {
                    r.input_id == route.input_id && r.output_id == route.output_id
                }) {
                    *existing = route;
                }
            }
            DaemonSignal::OutputsRefreshed(outputs) => {
                self.outputs = outputs;
            }
            DaemonSignal::StreamsRefreshed(streams) => self.streams = streams,
            DaemonSignal::FullRefresh { inputs, outputs, all_routes, streams, default_input, default_output, custom_inputs } => {
                self.inputs = inputs;
                self.outputs = outputs;
                self.all_routes = all_routes;
                self.streams = streams;
                self.default_input = default_input;
                self.default_output = default_output;
                self.custom_inputs = custom_inputs;
            }
            DaemonSignal::RulesRefreshed(rules) => self.rules = rules,
            DaemonSignal::CaptureDevicesRefreshed(devices) => self.capture_devices = devices,
            DaemonSignal::PlaybackDevicesRefreshed(devices) => self.playback_devices = devices,
            DaemonSignal::ComponentsRefreshed(components) => {
                self.beacn_connected = components.iter().any(|c| c.component_type.starts_with("beacn"));
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
            DaemonSignal::ProfilesRefreshed(profiles) => {
                self.profiles = profiles;
            }
            DaemonSignal::CustomInputsRefreshed(custom_inputs) => {
                self.custom_inputs = custom_inputs;
            }
        }
        self.clamp_cursors();
    }

    // -- Action handling --

    pub async fn handle_action(&mut self, action: AppAction, proxy: &MixCtlProxy<'_>) {
        match action {
            AppAction::Quit => {}
            AppAction::NextFocus => {
                self.focus = self.focus.next();
                self.footer_cursor = 0;
            }
            AppAction::PrevFocus => {
                self.focus = self.focus.prev();
                self.footer_cursor = 0;
            }

            // -- Matrix navigation --
            AppAction::CursorUp => {
                if self.matrix_row > 0 {
                    self.matrix_row -= 1;
                }
            }
            AppAction::CursorDown => {
                if self.matrix_row < self.inputs.len() + self.custom_inputs.len() {
                    self.matrix_row += 1;
                }
            }
            AppAction::CursorLeft => {
                if self.matrix_col > 0 {
                    self.matrix_col -= 1;
                }
            }
            AppAction::CursorRight => {
                if self.matrix_col < self.outputs.len() {
                    self.matrix_col += 1;
                }
            }

            // -- Volume --
            AppAction::VolumeUp { fine } => {
                let step: i16 = if fine { self.config.volume_fine_step as i16 } else { self.config.volume_step as i16 };
                if self.is_custom_input_row(self.matrix_row) {
                    // Custom input row: adjust custom input value
                    if let Some(ci) = self.custom_input_at_row(self.matrix_row) {
                        let new_val = (ci.value as i16 + step).min(100) as u8;
                        proxy.set_custom_input_value(ci.id, new_val).await.ok();
                    }
                } else if self.matrix_row == 0 && self.matrix_col >= 1 {
                    // Output header row: adjust output master volume
                    if let Some(output) = self.output_at_col(self.matrix_col) {
                        let new_vol = (output.volume as i16 + step).min(100) as u8;
                        proxy.set_output_volume(output.id, new_vol).await.ok();
                    }
                } else if self.matrix_row >= 1 && self.matrix_col >= 1 {
                    // Route cell
                    if let Some(route) = self.route_at(self.matrix_row, self.matrix_col) {
                        let new_vol = (route.volume as i16 + step).min(100) as u8;
                        proxy.set_route_volume(route.input_id, route.output_id, new_vol).await.ok();
                    }
                }
            }
            AppAction::VolumeDown { fine } => {
                let step: i16 = if fine { self.config.volume_fine_step as i16 } else { self.config.volume_step as i16 };
                if self.is_custom_input_row(self.matrix_row) {
                    if let Some(ci) = self.custom_input_at_row(self.matrix_row) {
                        let new_val = (ci.value as i16 - step).max(0) as u8;
                        proxy.set_custom_input_value(ci.id, new_val).await.ok();
                    }
                } else if self.matrix_row == 0 && self.matrix_col >= 1 {
                    if let Some(output) = self.output_at_col(self.matrix_col) {
                        let new_vol = (output.volume as i16 - step).max(0) as u8;
                        proxy.set_output_volume(output.id, new_vol).await.ok();
                    }
                } else if self.matrix_row >= 1 && self.matrix_col >= 1 {
                    if let Some(route) = self.route_at(self.matrix_row, self.matrix_col) {
                        let new_vol = (route.volume as i16 - step).max(0) as u8;
                        proxy.set_route_volume(route.input_id, route.output_id, new_vol).await.ok();
                    }
                }
            }
            AppAction::ToggleMute => {
                if self.is_custom_input_row(self.matrix_row) {
                    // Mute is a no-op for custom inputs
                } else if self.matrix_row == 0 && self.matrix_col >= 1 {
                    if let Some(output) = self.output_at_col(self.matrix_col) {
                        proxy.set_output_mute(output.id, !output.muted).await.ok();
                    }
                } else if self.matrix_row >= 1 && self.matrix_col >= 1 {
                    if let Some(route) = self.route_at(self.matrix_row, self.matrix_col) {
                        proxy.set_route_mute(route.input_id, route.output_id, !route.muted).await.ok();
                    }
                }
            }

            // -- Overlays --
            AppAction::OpenDsp => {
                // Select the input or output under cursor for DSP editing
                if self.matrix_row >= 1 {
                    self.dsp_cursor = self.matrix_row - 1; // input index
                } else if self.matrix_col >= 1 {
                    self.dsp_cursor = self.inputs.len() + (self.matrix_col - 1); // output index
                }
                self.dsp_editing = false;
                self.dsp_param_cursor = 0;
                self.overlay = Overlay::Dsp;
            }
            AppAction::OpenSettings => {
                // Select input or output under cursor
                if self.matrix_row >= 1 && self.matrix_col == 0 {
                    self.settings_cursor = self.matrix_row - 1;
                } else if self.matrix_row == 0 && self.matrix_col >= 1 {
                    self.settings_cursor = self.inputs.len() + (self.matrix_col - 1);
                }
                self.overlay = Overlay::Settings;
            }
            AppAction::OpenProfiles => {
                self.profiles = proxy.list_profiles().await.unwrap_or_default();
                self.profile_cursor = 0;
                self.profile_name_buf = None;
                self.overlay = Overlay::Profiles;
            }
            AppAction::ToggleHelp => {
                self.overlay = if self.overlay == Overlay::Help { Overlay::None } else { Overlay::Help };
            }
            AppAction::CloseOverlay => {
                self.dsp_editing = false;
                self.beacn_editing = false;
                self.overlay = Overlay::None;
            }

            // -- Settings overlay --
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
            AppAction::CancelRename => { self.rename_buf = None; }
            AppAction::RenameChar(c) => { if let Some(ref mut buf) = self.rename_buf { buf.push(c); } }
            AppAction::RenameBackspace => { if let Some(ref mut buf) = self.rename_buf { buf.pop(); } }
            AppAction::CycleColor => {
                if self.settings_cursor < self.inputs.len() {
                    if let Some(input) = self.inputs.get(self.settings_cursor) {
                        proxy.set_input_color(input.id, next_palette_color(&input.color)).await.ok();
                    }
                } else {
                    let idx = self.settings_cursor - self.inputs.len();
                    if let Some(output) = self.outputs.get(idx) {
                        proxy.set_output_color(output.id, next_palette_color(&output.color)).await.ok();
                    }
                }
            }
            AppAction::SetOutputTarget => {
                if self.settings_cursor >= self.inputs.len() {
                    let idx = self.settings_cursor - self.inputs.len();
                    if let Some(output) = self.outputs.get(idx) {
                        if !self.playback_devices.is_empty() {
                            let current_idx = self.playback_devices.iter()
                                .position(|d| d.device_name == output.target_device)
                                .unwrap_or(0);
                            let next_idx = (current_idx + 1) % self.playback_devices.len();
                            proxy.set_output_target(output.id, &self.playback_devices[next_idx].device_name).await.ok();
                        }
                    }
                }
            }
            AppAction::MoveUp => {
                if self.overlay == Overlay::Settings {
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
            }
            AppAction::MoveDown => {
                if self.overlay == Overlay::Settings {
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
            }
            AppAction::SetDefault => {
                if self.matrix_row >= 1 && self.matrix_col == 0 {
                    // Set default input
                    if let Some(input) = self.input_at_row(self.matrix_row) {
                        let new_id = if self.default_input == input.id { 0 } else { input.id };
                        proxy.set_default_input(new_id).await.ok();
                        self.default_input = new_id;
                    }
                } else if self.matrix_row == 0 && self.matrix_col >= 1 {
                    // Set default output
                    if let Some(output) = self.output_at_col(self.matrix_col) {
                        let new_id = if self.default_output == output.id { 0 } else { output.id };
                        proxy.set_default_output(new_id).await.ok();
                        self.default_output = new_id;
                    }
                }
            }

            // -- Footer actions --
            AppAction::FooterUp => {
                self.footer_cursor = self.footer_cursor.saturating_sub(1);
            }
            AppAction::FooterDown => {
                let count = self.footer_item_count();
                if count > 0 && self.footer_cursor + 1 < count {
                    self.footer_cursor += 1;
                }
            }
            AppAction::AssignToInput(n) => {
                match self.focus {
                    FocusArea::Streams => {
                        if let Some(stream) = self.streams.get(self.footer_cursor) {
                            if let Some(input) = self.inputs.get(n - 1) {
                                proxy.assign_stream(stream.pw_node_id, input.id, false).await.ok();
                            }
                        }
                    }
                    FocusArea::Capture => {
                        if let Some(device) = self.capture_devices.get(self.footer_cursor) {
                            if let Some(input) = self.inputs.get(n - 1) {
                                proxy.bind_capture_to_input(input.id, &device.device_name).await.ok();
                            }
                        }
                    }
                    FocusArea::Rules => {
                        if let Some(rule) = self.rules.get(self.footer_cursor) {
                            if let Some(input) = self.inputs.get(n - 1) {
                                proxy.set_app_rule(&rule.app_name, input.id).await.ok();
                            }
                        }
                    }
                    _ => {}
                }
            }
            AppAction::DeleteItem => {
                match self.focus {
                    FocusArea::Rules => {
                        if let Some(rule) = self.rules.get(self.footer_cursor) {
                            proxy.remove_app_rule(&rule.app_name).await.ok();
                        }
                    }
                    FocusArea::Capture => {
                        if let Some(device) = self.capture_devices.get(self.footer_cursor) {
                            if device.is_added && device.input_id > 0 {
                                proxy.remove_capture_input(device.input_id).await.ok();
                            }
                        }
                    }
                    _ => {}
                }
            }
            AppAction::UnbindItem => {
                match self.focus {
                    FocusArea::Capture => {
                        if let Some(device) = self.capture_devices.get(self.footer_cursor) {
                            if device.is_added {
                                proxy.bind_capture_to_input(0, &device.device_name).await.ok();
                            }
                        }
                    }
                    FocusArea::Playback => {
                        // Unbind playback device from any output that targets it
                        if let Some(device) = self.playback_devices.get(self.footer_cursor) {
                            for output in &self.outputs {
                                if output.target_device == device.device_name {
                                    proxy.set_output_target(output.id, "").await.ok();
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }

            // -- DSP overlay --
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
            AppAction::DspResetEq => {
                if let Some(input) = self.dsp_selected_input() {
                    proxy.reset_input_eq(input.id).await.ok();
                }
            }
            AppAction::EnterDspEdit => { self.dsp_editing = true; self.dsp_param_cursor = 0; }
            AppAction::ExitDspEdit => { self.dsp_editing = false; }
            AppAction::DspParamNext => {
                let max = self.dsp_param_count();
                if max > 0 && self.dsp_param_cursor + 1 < max { self.dsp_param_cursor += 1; }
            }
            AppAction::DspParamPrev => { self.dsp_param_cursor = self.dsp_param_cursor.saturating_sub(1); }
            AppAction::DspValueUp { fine } => { self.dsp_adjust_value(proxy, fine, true).await; }
            AppAction::DspValueDown { fine } => { self.dsp_adjust_value(proxy, fine, false).await; }
            AppAction::DspCursorUp => { self.dsp_cursor = self.dsp_cursor.saturating_sub(1); }
            AppAction::DspCursorDown => {
                let total = self.inputs.len() + self.outputs.len();
                if total > 0 && self.dsp_cursor + 1 < total { self.dsp_cursor += 1; }
            }

            // -- Profiles overlay --
            AppAction::ProfileSave => {
                self.profile_name_buf = Some(String::new());
            }
            AppAction::ProfileLoad => {
                if let Some(name) = self.profiles.get(self.profile_cursor) {
                    proxy.load_profile(name).await.ok();
                }
            }
            AppAction::ProfileDelete => {
                if let Some(name) = self.profiles.get(self.profile_cursor).cloned() {
                    proxy.delete_profile(&name).await.ok();
                    self.profiles = proxy.list_profiles().await.unwrap_or_default();
                    self.clamp_cursors();
                }
            }
            AppAction::ProfileNameChar(c) => {
                if let Some(ref mut buf) = self.profile_name_buf { buf.push(c); }
            }
            AppAction::ProfileNameBackspace => {
                if let Some(ref mut buf) = self.profile_name_buf { buf.pop(); }
            }
            AppAction::ProfileConfirmName => {
                if let Some(name) = self.profile_name_buf.take() {
                    if !name.is_empty() {
                        proxy.save_profile(&name).await.ok();
                        self.profiles = proxy.list_profiles().await.unwrap_or_default();
                    }
                }
            }
            AppAction::ProfileCancelName => {
                self.profile_name_buf = None;
            }

            // -- Beacn overlay --
            AppAction::OpenBeacn => {
                self.beacn_cursor = 0;
                self.beacn_field = 0;
                self.beacn_editing = false;
                self.overlay = Overlay::Beacn;
            }
            AppAction::BeacnUp => {
                self.beacn_cursor = self.beacn_cursor.saturating_sub(1);
            }
            AppAction::BeacnDown => {
                let max = ButtonMappings::BUTTON_NAMES.len();
                if max > 0 && self.beacn_cursor + 1 < max {
                    self.beacn_cursor += 1;
                }
            }
            AppAction::BeacnLeft => {
                if self.beacn_field > 0 {
                    self.beacn_field -= 1;
                }
            }
            AppAction::BeacnRight => {
                if self.beacn_field < 1 {
                    self.beacn_field += 1;
                }
            }
            AppAction::BeacnToggleEdit => {
                self.beacn_editing = !self.beacn_editing;
            }
            AppAction::BeacnCycleAction => {
                if self.beacn_editing {
                    self.beacn_cycle_action(proxy, true).await;
                }
            }
            AppAction::BeacnCycleActionBack => {
                if self.beacn_editing {
                    self.beacn_cycle_action(proxy, false).await;
                }
            }
        }
    }

    // -- DSP helpers (preserved from original) --

    pub fn dsp_selected_input(&self) -> Option<&InputInfo> {
        if self.dsp_cursor < self.inputs.len() {
            self.inputs.get(self.dsp_cursor)
        } else {
            None
        }
    }

    pub fn dsp_selected_output(&self) -> Option<&OutputInfo> {
        if self.dsp_cursor >= self.inputs.len() {
            self.outputs.get(self.dsp_cursor - self.inputs.len())
        } else {
            None
        }
    }

    pub fn dsp_param_count(&self) -> usize {
        if let Some(input) = self.dsp_selected_input() {
            let eq_count = self.dsp_input_eq.get(&input.id)
                .map(|(_, bands)| bands.len() * 3)
                .unwrap_or(0);
            eq_count + 4 + 3 // gate(4) + deesser(3)
        } else if self.dsp_selected_output().is_some() {
            6 + 2 // compressor(6) + limiter(2)
        } else {
            0
        }
    }

    pub fn dsp_param_label(&self) -> Option<(String, f64)> {
        if let Some(input) = self.dsp_selected_input() {
            let eq_bands = self.dsp_input_eq.get(&input.id).map(|(_, b)| b.as_slice()).unwrap_or(&[]);
            let eq_params = eq_bands.len() * 3;
            let cursor = self.dsp_param_cursor;
            if cursor < eq_params {
                let (bi, pi) = (cursor / 3, cursor % 3);
                if let Some(band) = eq_bands.get(bi) {
                    return match pi {
                        0 => Some((format!("Band {} Freq", bi + 1), band.frequency)),
                        1 => Some((format!("Band {} Gain", bi + 1), band.gain_db)),
                        2 => Some((format!("Band {} Q", bi + 1), band.q)),
                        _ => None,
                    };
                }
            }
            let gs = eq_params;
            if cursor >= gs && cursor < gs + 4 {
                if let Some(g) = self.dsp_input_gate.get(&input.id) {
                    return match cursor - gs {
                        0 => Some(("Gate Threshold".into(), g.threshold_db)),
                        1 => Some(("Gate Attack".into(), g.attack_ms)),
                        2 => Some(("Gate Release".into(), g.release_ms)),
                        3 => Some(("Gate Hold".into(), g.hold_ms)),
                        _ => None,
                    };
                }
            }
            let ds = gs + 4;
            if cursor >= ds && cursor < ds + 3 {
                if let Some(d) = self.dsp_input_deesser.get(&input.id) {
                    return match cursor - ds {
                        0 => Some(("De-esser Freq".into(), d.frequency)),
                        1 => Some(("De-esser Threshold".into(), d.threshold_db)),
                        2 => Some(("De-esser Ratio".into(), d.ratio)),
                        _ => None,
                    };
                }
            }
        } else if let Some(output) = self.dsp_selected_output() {
            let cursor = self.dsp_param_cursor;
            if cursor < 6 {
                if let Some(c) = self.dsp_output_compressor.get(&output.id) {
                    return match cursor {
                        0 => Some(("Comp Threshold".into(), c.threshold_db)),
                        1 => Some(("Comp Ratio".into(), c.ratio)),
                        2 => Some(("Comp Attack".into(), c.attack_ms)),
                        3 => Some(("Comp Release".into(), c.release_ms)),
                        4 => Some(("Comp Makeup".into(), c.makeup_gain_db)),
                        5 => Some(("Comp Knee".into(), c.knee_db)),
                        _ => None,
                    };
                }
            }
            if cursor >= 6 && cursor < 8 {
                if let Some(l) = self.dsp_output_limiter.get(&output.id) {
                    return match cursor - 6 {
                        0 => Some(("Limiter Ceiling".into(), l.ceiling_db)),
                        1 => Some(("Limiter Release".into(), l.release_ms)),
                        _ => None,
                    };
                }
            }
        }
        None
    }

    async fn dsp_adjust_value(&mut self, proxy: &MixCtlProxy<'_>, fine: bool, up: bool) {
        let fd = if fine { 5.0 } else { 1.0 };
        let s = if up { 1.0 } else { -1.0 };

        if let Some(input) = self.dsp_selected_input().cloned() {
            let eq_bands = self.dsp_input_eq.get(&input.id).map(|(_, b)| b.clone()).unwrap_or_default();
            let eq_params = eq_bands.len() * 3;
            let cursor = self.dsp_param_cursor;
            if cursor < eq_params {
                let (bi, pi) = (cursor / 3, cursor % 3);
                if let Some(band) = eq_bands.get(bi) {
                    let (mut f, mut g, mut q) = (band.frequency, band.gain_db, band.q);
                    match pi {
                        0 => f = (f + s * 100.0 / fd).clamp(20.0, 20000.0),
                        1 => g = (g + s * 0.5 / fd).clamp(-24.0, 24.0),
                        2 => q = (q + s * 0.1 / fd).clamp(0.1, 20.0),
                        _ => {}
                    }
                    proxy.set_input_eq_band(input.id, bi as u8, &band.band_type, f, g, q).await.ok();
                }
                return;
            }
            let gs = eq_params;
            if cursor >= gs && cursor < gs + 4 {
                if let Some(gate) = self.dsp_input_gate.get(&input.id).cloned() {
                    let (mut t, mut a, mut r, mut h) = (gate.threshold_db, gate.attack_ms, gate.release_ms, gate.hold_ms);
                    match cursor - gs {
                        0 => t = (t + s * 1.0 / fd).clamp(-80.0, 0.0),
                        1 => a = (a + s * 1.0 / fd).clamp(0.1, 200.0),
                        2 => r = (r + s * 10.0 / fd).clamp(1.0, 2000.0),
                        3 => h = (h + s * 5.0 / fd).clamp(0.0, 500.0),
                        _ => {}
                    }
                    proxy.set_input_gate(input.id, t, a, r, h).await.ok();
                }
                return;
            }
            let ds = gs + 4;
            if cursor >= ds && cursor < ds + 3 {
                if let Some(de) = self.dsp_input_deesser.get(&input.id).cloned() {
                    let (mut freq, mut thresh, mut ratio) = (de.frequency, de.threshold_db, de.ratio);
                    match cursor - ds {
                        0 => freq = (freq + s * 100.0 / fd).clamp(1000.0, 16000.0),
                        1 => thresh = (thresh + s * 1.0 / fd).clamp(-60.0, 0.0),
                        2 => ratio = (ratio + s * 0.5 / fd).clamp(1.0, 20.0),
                        _ => {}
                    }
                    proxy.set_input_deesser(input.id, freq, thresh, ratio).await.ok();
                }
            }
        } else if let Some(output) = self.dsp_selected_output().cloned() {
            let cursor = self.dsp_param_cursor;
            if cursor < 6 {
                if let Some(c) = self.dsp_output_compressor.get(&output.id).cloned() {
                    let (mut t, mut ratio, mut a, mut r, mut m, mut k) = (c.threshold_db, c.ratio, c.attack_ms, c.release_ms, c.makeup_gain_db, c.knee_db);
                    match cursor {
                        0 => t = (t + s * 1.0 / fd).clamp(-60.0, 0.0),
                        1 => ratio = (ratio + s * 0.5 / fd).clamp(1.0, 20.0),
                        2 => a = (a + s * 1.0 / fd).clamp(0.1, 200.0),
                        3 => r = (r + s * 10.0 / fd).clamp(1.0, 2000.0),
                        4 => m = (m + s * 0.5 / fd).clamp(-12.0, 24.0),
                        5 => k = (k + s * 0.5 / fd).clamp(0.0, 12.0),
                        _ => {}
                    }
                    proxy.set_output_compressor(output.id, t, ratio, a, r, m, k).await.ok();
                }
                return;
            }
            if cursor >= 6 && cursor < 8 {
                if let Some(l) = self.dsp_output_limiter.get(&output.id).cloned() {
                    let (mut ceil, mut rel) = (l.ceiling_db, l.release_ms);
                    match cursor - 6 {
                        0 => ceil = (ceil + s * 0.5 / fd).clamp(-24.0, 0.0),
                        1 => rel = (rel + s * 5.0 / fd).clamp(1.0, 500.0),
                        _ => {}
                    }
                    proxy.set_output_limiter(output.id, ceil, rel).await.ok();
                }
            }
        }
    }

    async fn beacn_cycle_action(&mut self, proxy: &MixCtlProxy<'_>, forward: bool) {
        let Some(config) = self.beacn_config.clone() else { return };
        let Some(name) = ButtonMappings::BUTTON_NAMES.get(self.beacn_cursor) else { return };
        let Some(mapping) = config.button_mappings.get(name) else { return };

        let all = ButtonAction::ALL_SIMPLE;
        let current = if self.beacn_field == 0 { &mapping.press } else { &mapping.hold };

        // Find the current action's index in ALL_SIMPLE
        let cur_idx = all.iter().position(|a| {
            a.display_name() == current.display_name()
        }).unwrap_or(0);

        let next_idx = if forward {
            (cur_idx + 1) % all.len()
        } else {
            (cur_idx + all.len() - 1) % all.len()
        };

        let new_action = all[next_idx].clone();
        let new_mapping = if self.beacn_field == 0 {
            ButtonMapping { press: new_action, hold: mapping.hold.clone() }
        } else {
            ButtonMapping { press: mapping.press.clone(), hold: new_action }
        };

        let mut new_config = config.clone();
        new_config.button_mappings.set(name, new_mapping);

        if let Ok(json) = serde_json::to_string(&new_config) {
            proxy.set_config_section("beacn", &json).await.ok();
        }
        self.beacn_config = Some(new_config);
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
