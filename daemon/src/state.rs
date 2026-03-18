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
    pub current_page: u32,
    pub outputs: HashMap<String, OutputState>,
    pub routes: HashMap<String, RouteState>,
    #[serde(default)]
    pub capture_volumes: HashMap<String, CaptureVolumeState>,
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

impl Default for StateFile {
    fn default() -> Self {
        Self {
            version: 1,
            current_page: 0,
            outputs: HashMap::new(),
            routes: HashMap::new(),
            capture_volumes: HashMap::new(),
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

        // Clamp page
        let max = config.max_page();
        if self.current_page > max {
            self.current_page = max;
            changed = true;
        }

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
}
