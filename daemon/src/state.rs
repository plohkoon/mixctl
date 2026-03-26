use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::config::ConfigFile;

/// Runtime state for an active audio stream (not persisted).
#[derive(Debug, Clone)]
pub struct StreamState {
    pub app_name: String,
    pub media_name: String,
    pub input_id: u32,
}

/// Runtime state for a discovered capture device (not persisted).
#[derive(Debug, Clone)]
pub struct CaptureDeviceState {
    pub name: String,
    pub device_name: String,
}

/// Runtime state for a discovered playback device (not persisted).
#[derive(Debug, Clone)]
pub struct PlaybackDeviceState {
    pub name: String,
    pub device_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateFile {
    pub version: u32,
    pub outputs: HashMap<String, OutputState>,
    pub routes: HashMap<String, RouteState>,
    #[serde(default)]
    pub capture_volumes: HashMap<String, CaptureVolumeState>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub input_eq: HashMap<String, InputEqDspState>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub input_gate: HashMap<String, GateDspState>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub input_deesser: HashMap<String, DeesserDspState>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub output_compressor: HashMap<String, CompressorDspState>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub output_limiter: HashMap<String, LimiterDspState>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub custom_input_values: HashMap<String, u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputState {
    pub volume: u8,
    pub muted: bool,
}

impl Default for OutputState {
    fn default() -> Self {
        Self {
            volume: 100,
            muted: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteState {
    pub volume: u8,
    pub muted: bool,
}

impl Default for RouteState {
    fn default() -> Self {
        Self {
            volume: 100,
            muted: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureVolumeState {
    pub volume: u8,
    pub muted: bool,
}

impl Default for CaptureVolumeState {
    fn default() -> Self {
        Self {
            volume: 100,
            muted: false,
        }
    }
}

// ---------------------------------------------------------------------------
// DSP state types (persisted as shadow copies)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EqBandDspState {
    pub band_type: String,
    pub frequency: f64,
    pub gain_db: f64,
    pub q: f64,
}

impl Default for EqBandDspState {
    fn default() -> Self {
        Self {
            band_type: "peaking".to_string(),
            frequency: 1000.0,
            gain_db: 0.0,
            q: 1.4,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputEqDspState {
    pub enabled: bool,
    pub bands: Vec<EqBandDspState>,
}

impl Default for InputEqDspState {
    fn default() -> Self {
        use crate::audio::dsp::DEFAULT_EQ_FREQS;
        Self {
            enabled: false,
            bands: DEFAULT_EQ_FREQS.iter().map(|&freq| EqBandDspState {
                band_type: "peaking".to_string(),
                frequency: freq as f64,
                gain_db: 0.0,
                q: 1.4,
            }).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateDspState {
    pub enabled: bool,
    pub threshold_db: f64,
    pub attack_ms: f64,
    pub release_ms: f64,
    pub hold_ms: f64,
}

impl Default for GateDspState {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold_db: -40.0,
            attack_ms: 1.0,
            release_ms: 100.0,
            hold_ms: 50.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeesserDspState {
    pub enabled: bool,
    pub frequency: f64,
    pub threshold_db: f64,
    pub ratio: f64,
}

impl Default for DeesserDspState {
    fn default() -> Self {
        Self {
            enabled: false,
            frequency: 6000.0,
            threshold_db: -20.0,
            ratio: 4.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressorDspState {
    pub enabled: bool,
    pub threshold_db: f64,
    pub ratio: f64,
    pub attack_ms: f64,
    pub release_ms: f64,
    pub makeup_gain_db: f64,
    pub knee_db: f64,
}

impl Default for CompressorDspState {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold_db: -18.0,
            ratio: 4.0,
            attack_ms: 5.0,
            release_ms: 50.0,
            makeup_gain_db: 0.0,
            knee_db: 6.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimiterDspState {
    pub enabled: bool,
    pub ceiling_db: f64,
    pub release_ms: f64,
}

impl Default for LimiterDspState {
    fn default() -> Self {
        Self {
            enabled: false,
            ceiling_db: -0.5,
            release_ms: 10.0,
        }
    }
}

impl Default for StateFile {
    fn default() -> Self {
        Self {
            version: 1,
            outputs: HashMap::new(),
            routes: HashMap::new(),
            capture_volumes: HashMap::new(),
            input_eq: HashMap::new(),
            input_gate: HashMap::new(),
            input_deesser: HashMap::new(),
            output_compressor: HashMap::new(),
            output_limiter: HashMap::new(),
            custom_input_values: HashMap::new(),
        }
    }
}

fn route_key(input_id: u32, output_id: u32) -> String {
    format!("{}:{}", input_id, output_id)
}

impl StateFile {
    pub fn path() -> PathBuf {
        dirs::home_dir()
            .expect("no home directory")
            .join(".local/state/mixctl.toml")
    }

    pub fn load() -> anyhow::Result<Self> {
        let path = Self::path();
        if path.exists() {
            let text = fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            let state: Self = toml::from_str(&text)
                .with_context(|| format!("parsing {}", path.display()))?;
            if state.version != 1 {
                bail!(
                    "unsupported state version {}, expected 1 ({})",
                    state.version,
                    path.display()
                );
            }
            Ok(state)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let text = toml::to_string_pretty(self)?;
        fs::write(&path, text)
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    /// Reconcile state with config: ensure entries exist for all outputs and
    /// input×output routes, remove stale entries, clamp page.
    /// Returns true if anything changed.
    pub fn reconcile(&mut self, config: &ConfigFile) -> bool {
        let mut changed = false;

        // Drop output entries whose key doesn't parse as u32
        let bad_keys: Vec<String> = self.outputs.keys()
            .filter(|k| k.parse::<u32>().is_err())
            .cloned()
            .collect();
        for key in bad_keys {
            warn!("dropping output state entry with non-numeric key '{}'", key);
            self.outputs.remove(&key);
            changed = true;
        }

        // Ensure OutputState for each output in config
        for out in &config.outputs {
            let key = out.id().to_string();
            if !self.outputs.contains_key(&key) {
                self.outputs.insert(key, OutputState::default());
                changed = true;
            }
        }

        // Remove stale output entries
        let output_ids: Vec<String> = config.outputs.iter().map(|o| o.id().to_string()).collect();
        self.outputs.retain(|key, _| {
            let keep = output_ids.contains(key);
            if !keep {
                changed = true;
            }
            keep
        });

        // Ensure RouteState for each (input, output) pair
        for inp in &config.inputs {
            for out in &config.outputs {
                let key = route_key(inp.id(), out.id());
                if !self.routes.contains_key(&key) {
                    self.routes.insert(key, RouteState::default());
                    changed = true;
                }
            }
        }

        // Remove stale route entries
        let valid_keys: Vec<String> = config.inputs.iter()
            .flat_map(|inp| config.outputs.iter().map(move |out| route_key(inp.id(), out.id())))
            .collect();
        self.routes.retain(|key, _| {
            let keep = valid_keys.contains(key);
            if !keep {
                changed = true;
            }
            keep
        });

        // Ensure CaptureVolumeState for each input with a capture device
        let capture_input_ids: Vec<String> = config.inputs.iter()
            .filter(|i| i.capture_device.is_some())
            .map(|i| i.id().to_string())
            .collect();
        for key in &capture_input_ids {
            if !self.capture_volumes.contains_key(key) {
                self.capture_volumes.insert(key.clone(), CaptureVolumeState::default());
                changed = true;
            }
        }

        // Remove stale capture volume entries
        self.capture_volumes.retain(|key, _| {
            let keep = capture_input_ids.contains(key);
            if !keep {
                changed = true;
            }
            keep
        });

        changed
    }

    // -- Output state accessors --

    pub fn output_state(&self, id: u32) -> Option<&OutputState> {
        self.outputs.get(&id.to_string())
    }

    pub fn set_output_volume(&mut self, id: u32, volume: u8) {
        if let Some(st) = self.outputs.get_mut(&id.to_string()) {
            st.volume = volume;
        }
    }

    pub fn set_output_muted(&mut self, id: u32, muted: bool) {
        if let Some(st) = self.outputs.get_mut(&id.to_string()) {
            st.muted = muted;
        }
    }

    pub fn ensure_output(&mut self, id: u32) -> &mut OutputState {
        self.outputs
            .entry(id.to_string())
            .or_insert_with(OutputState::default)
    }

    pub fn remove_output(&mut self, id: u32) {
        self.outputs.remove(&id.to_string());
    }

    // -- Route state accessors --

    pub fn route_state(&self, input_id: u32, output_id: u32) -> Option<&RouteState> {
        self.routes.get(&route_key(input_id, output_id))
    }

    pub fn set_route_volume(&mut self, input_id: u32, output_id: u32, volume: u8) {
        if let Some(st) = self.routes.get_mut(&route_key(input_id, output_id)) {
            st.volume = volume;
        }
    }

    pub fn set_route_muted(&mut self, input_id: u32, output_id: u32, muted: bool) {
        if let Some(st) = self.routes.get_mut(&route_key(input_id, output_id)) {
            st.muted = muted;
        }
    }

    /// Auto-create routes for a new input to all given output IDs.
    pub fn ensure_routes_for_input(&mut self, input_id: u32, output_ids: &[u32]) {
        for &output_id in output_ids {
            let key = route_key(input_id, output_id);
            self.routes.entry(key).or_insert_with(RouteState::default);
        }
    }

    /// Copy routes from a source output for a new output. If source doesn't
    /// exist or source_output_id is 0, defaults to volume=100/muted=false.
    pub fn copy_routes_for_output(
        &mut self,
        new_output_id: u32,
        source_output_id: u32,
        input_ids: &[u32],
    ) {
        for &input_id in input_ids {
            let route = if source_output_id > 0 {
                self.route_state(input_id, source_output_id)
                    .cloned()
                    .unwrap_or_default()
            } else {
                RouteState::default()
            };
            self.routes.insert(
                route_key(input_id, new_output_id),
                route,
            );
        }
    }

    /// Remove all routes involving a given input.
    pub fn remove_routes_for_input(&mut self, input_id: u32) {
        let prefix = format!("{}:", input_id);
        self.routes.retain(|key, _| !key.starts_with(&prefix));
    }

    /// Remove all routes involving a given output.
    pub fn remove_routes_for_output(&mut self, output_id: u32) {
        let suffix = format!(":{}", output_id);
        self.routes.retain(|key, _| !key.ends_with(&suffix));
    }

    // -- Capture volume state accessors --

    pub fn capture_volume_state(&self, input_id: u32) -> Option<&CaptureVolumeState> {
        self.capture_volumes.get(&input_id.to_string())
    }

    pub fn set_capture_volume(&mut self, input_id: u32, volume: u8) {
        self.capture_volumes
            .entry(input_id.to_string())
            .or_insert_with(CaptureVolumeState::default)
            .volume = volume;
    }

    pub fn set_capture_muted(&mut self, input_id: u32, muted: bool) {
        self.capture_volumes
            .entry(input_id.to_string())
            .or_insert_with(CaptureVolumeState::default)
            .muted = muted;
    }

    pub fn ensure_capture_volume(&mut self, input_id: u32) -> &mut CaptureVolumeState {
        self.capture_volumes
            .entry(input_id.to_string())
            .or_insert_with(CaptureVolumeState::default)
    }

    // -- DSP state accessors --

    pub fn input_eq_state(&self, id: u32) -> Option<&InputEqDspState> {
        self.input_eq.get(&id.to_string())
    }

    pub fn ensure_input_eq(&mut self, id: u32) -> &mut InputEqDspState {
        self.input_eq
            .entry(id.to_string())
            .or_insert_with(InputEqDspState::default)
    }

    pub fn input_gate_state(&self, id: u32) -> Option<&GateDspState> {
        self.input_gate.get(&id.to_string())
    }

    pub fn ensure_input_gate(&mut self, id: u32) -> &mut GateDspState {
        self.input_gate
            .entry(id.to_string())
            .or_insert_with(GateDspState::default)
    }

    pub fn input_deesser_state(&self, id: u32) -> Option<&DeesserDspState> {
        self.input_deesser.get(&id.to_string())
    }

    pub fn ensure_input_deesser(&mut self, id: u32) -> &mut DeesserDspState {
        self.input_deesser
            .entry(id.to_string())
            .or_insert_with(DeesserDspState::default)
    }

    pub fn output_compressor_state(&self, id: u32) -> Option<&CompressorDspState> {
        self.output_compressor.get(&id.to_string())
    }

    pub fn ensure_output_compressor(&mut self, id: u32) -> &mut CompressorDspState {
        self.output_compressor
            .entry(id.to_string())
            .or_insert_with(CompressorDspState::default)
    }

    pub fn output_limiter_state(&self, id: u32) -> Option<&LimiterDspState> {
        self.output_limiter.get(&id.to_string())
    }

    pub fn ensure_output_limiter(&mut self, id: u32) -> &mut LimiterDspState {
        self.output_limiter
            .entry(id.to_string())
            .or_insert_with(LimiterDspState::default)
    }

    // -- Custom input value accessors --

    pub fn custom_input_value(&self, id: u32) -> u8 {
        self.custom_input_values
            .get(&id.to_string())
            .copied()
            .unwrap_or(50)
    }

    pub fn set_custom_input_value(&mut self, id: u32, value: u8) {
        self.custom_input_values
            .insert(id.to_string(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ChannelConfig, ConfigFile};

    fn make_channel(id: u32, name: &str) -> ChannelConfig {
        ChannelConfig {
            id: Some(id),
            name: name.into(),
            color: "#000000".into(),
            target_device: None,
            capture_device: None,
        }
    }

    fn make_config(inputs: Vec<ChannelConfig>, outputs: Vec<ChannelConfig>) -> ConfigFile {
        ConfigFile {
            version: 1,
            inputs,
            outputs,
            default_input: None,
            default_output: None,
            app_rules: Vec::new(),
            custom_inputs: vec![],
            broadcast_levels: None,
            beacn: Default::default(),
            ui: Default::default(),
            applet: Default::default(),
            cli: Default::default(),
            tui: Default::default(),
        }
    }

    #[test]
    fn reconcile_empty_config_clears_state() {
        let config = make_config(vec![], vec![]);
        let mut state = StateFile::default();
        // Pre-populate with stale data
        state.outputs.insert("99".into(), OutputState::default());
        state.routes.insert("99:88".into(), RouteState::default());

        let changed = state.reconcile(&config);

        assert!(changed);
        assert!(state.outputs.is_empty());
        assert!(state.routes.is_empty());
    }

    #[test]
    fn reconcile_creates_defaults_for_new_channels() {
        let config = make_config(
            vec![make_channel(1, "Sys"), make_channel(2, "Game")],
            vec![make_channel(5, "Mix1")],
        );
        let mut state = StateFile::default();

        let changed = state.reconcile(&config);

        assert!(changed);
        // Output state created
        assert!(state.outputs.contains_key("5"));
        // Route states created for each input x output
        assert!(state.routes.contains_key("1:5"));
        assert!(state.routes.contains_key("2:5"));
        // Defaults: volume=100, muted=false
        let route = state.route_state(1, 5).unwrap();
        assert_eq!(route.volume, 100);
        assert!(!route.muted);
    }

    #[test]
    fn reconcile_removes_stale_routes() {
        let config = make_config(
            vec![make_channel(1, "Sys")],
            vec![make_channel(5, "Mix1")],
        );
        let mut state = StateFile::default();
        // Pre-populate with routes for a now-removed input 2
        state.routes.insert("2:5".into(), RouteState::default());
        state.routes.insert("1:5".into(), RouteState::default());

        let changed = state.reconcile(&config);

        assert!(changed);
        assert!(state.routes.contains_key("1:5"));
        assert!(!state.routes.contains_key("2:5")); // stale, removed
    }

    #[test]
    fn reconcile_removes_stale_outputs() {
        let config = make_config(
            vec![make_channel(1, "Sys")],
            vec![make_channel(5, "Mix1")],
        );
        let mut state = StateFile::default();
        state.outputs.insert("5".into(), OutputState::default());
        state.outputs.insert("99".into(), OutputState::default()); // stale

        state.reconcile(&config);

        assert!(state.outputs.contains_key("5"));
        assert!(!state.outputs.contains_key("99"));
    }

    #[test]
    fn reconcile_no_change_returns_false() {
        let config = make_config(
            vec![make_channel(1, "Sys")],
            vec![make_channel(5, "Mix1")],
        );
        let mut state = StateFile::default();
        // First reconcile creates everything
        state.reconcile(&config);

        // Second reconcile should find nothing to change
        let changed = state.reconcile(&config);
        assert!(!changed);
    }
}
