use std::collections::HashMap;

use mixctl_core::dbus::MixCtlProxy;
use mixctl_core::parse_hex_color;
use mixctl_beacn_display::{DisplayState, OutputTab, SlotView};

/// Local mirror of the mixer state, populated from D-Bus.
pub struct BeacnState {
    pub inputs: Vec<InputEntry>,
    pub outputs: Vec<OutputEntry>,
    /// Key: "input_id:output_id" → RouteEntry
    pub routes: HashMap<String, RouteEntry>,
    pub current_output_index: usize,
    pub current_page: u32,
    /// Real-time audio levels per input_id (0.0-1.0)
    pub input_levels: HashMap<u32, f32>,
    /// Whether level monitoring is currently enabled
    pub levels_enabled: bool,
    /// App names grouped by input_id
    pub streams_by_input: HashMap<u32, Vec<String>>,
    /// Multiplier for dial delta (from config)
    pub dial_sensitivity: u32,
    /// Exponential decay factor per frame (from config)
    pub level_decay: f64,
    /// Custom inputs (non-audio controls with a single value)
    pub custom_inputs: Vec<CustomInputEntry>,
}

#[derive(Clone)]
pub struct InputEntry {
    pub id: u32,
    pub name: String,
    pub color: (u8, u8, u8),
}

#[derive(Clone)]
pub struct CustomInputEntry {
    pub id: u32,
    pub name: String,
    pub color: (u8, u8, u8),
    pub value: u8,
}

#[derive(Clone)]
pub struct OutputEntry {
    pub id: u32,
    pub name: String,
    pub color: (u8, u8, u8),
}

#[derive(Clone)]
pub struct RouteEntry {
    pub volume: u8,
    pub muted: bool,
}

fn route_key(input_id: u32, output_id: u32) -> String {
    format!("{input_id}:{output_id}")
}

impl BeacnState {
    pub fn new_with_config(dial_sensitivity: u32, level_decay: f64) -> Self {
        Self {
            inputs: Vec::new(),
            outputs: Vec::new(),
            routes: HashMap::new(),
            current_output_index: 0,
            current_page: 0,
            streams_by_input: HashMap::new(),
            input_levels: HashMap::new(),
            levels_enabled: false,
            dial_sensitivity,
            level_decay,
            custom_inputs: Vec::new(),
        }
    }

    /// Refresh all state from the mixer daemon via D-Bus.
    pub async fn refresh_from_dbus(&mut self, proxy: &MixCtlProxy<'_>) -> anyhow::Result<()> {
        // Fetch inputs and outputs concurrently to reduce round-trips
        let (inputs_res, outputs_res) = tokio::join!(
            proxy.list_inputs(),
            proxy.list_outputs(),
        );
        let inputs = inputs_res?;
        let outputs = outputs_res?;

        self.inputs = inputs
            .iter()
            .map(|i| InputEntry {
                id: i.id,
                name: i.name.clone(),
                color: parse_hex_color(&i.color).unwrap_or((128, 128, 128)),
            })
            .collect();

        self.outputs = outputs
            .iter()
            .map(|o| OutputEntry {
                id: o.id,
                name: o.name.clone(),
                color: parse_hex_color(&o.color).unwrap_or((128, 128, 128)),
            })
            .collect();

        // Clamp output index
        if !self.outputs.is_empty() && self.current_output_index >= self.outputs.len() {
            self.current_output_index = 0;
        }

        // Page state is managed locally (not fetched from daemon)
        // self.current_page stays at its current value (initialized to 0 on boot)

        // Fetch routes for all outputs
        self.routes.clear();
        for output in &outputs {
            let routes = proxy.list_routes_for_output(output.id).await?;
            for route in routes {
                self.routes.insert(
                    route_key(route.input_id, route.output_id),
                    RouteEntry {
                        volume: route.volume,
                        muted: route.muted,
                    },
                );
            }
        }

        Ok(())
    }

    /// Refresh stream assignments from the mixer daemon via D-Bus.
    pub async fn refresh_streams(&mut self, proxy: &MixCtlProxy<'_>) -> anyhow::Result<()> {
        let streams = proxy.list_streams().await?;
        self.streams_by_input.clear();
        for stream in streams {
            // Filter out internal mixctl streams
            if stream.app_name.contains("mixctl.") || stream.app_name.starts_with("output.") {
                continue;
            }
            self.streams_by_input
                .entry(stream.input_id)
                .or_default()
                .push(stream.app_name);
        }
        // Deduplicate (same app can have multiple PW streams)
        for names in self.streams_by_input.values_mut() {
            names.sort();
            names.dedup();
        }
        Ok(())
    }

    /// Refresh custom inputs from the mixer daemon via D-Bus.
    pub async fn refresh_custom_inputs(&mut self, proxy: &MixCtlProxy<'_>) -> anyhow::Result<()> {
        let custom_inputs = proxy.list_custom_inputs().await?;
        self.custom_inputs = custom_inputs
            .iter()
            .map(|ci| CustomInputEntry {
                id: ci.id,
                name: ci.name.clone(),
                color: parse_hex_color(&ci.color).unwrap_or((128, 128, 128)),
                value: ci.value,
            })
            .collect();
        Ok(())
    }

    /// Check if a given input_id belongs to a custom input.
    pub fn is_custom_input(&self, id: u32) -> bool {
        self.custom_inputs.iter().any(|ci| ci.id == id)
    }

    /// Update the value of a custom input in local state.
    pub fn set_custom_input_value(&mut self, id: u32, value: u8) {
        if let Some(ci) = self.custom_inputs.iter_mut().find(|ci| ci.id == id) {
            ci.value = value;
        }
    }

    pub fn max_page(&self) -> u32 {
        let n = (self.inputs.len() + self.custom_inputs.len()) as u32;
        if n == 0 { 0 } else { (n - 1) / 4 }
    }

    pub fn output_ids(&self) -> Vec<u32> {
        self.outputs.iter().map(|o| o.id).collect()
    }

    pub fn route_volume(&self, input_id: u32, output_id: u32) -> u8 {
        self.routes
            .get(&route_key(input_id, output_id))
            .map(|r| r.volume)
            .unwrap_or(100)
    }

    pub fn route_muted(&self, input_id: u32, output_id: u32) -> bool {
        self.routes
            .get(&route_key(input_id, output_id))
            .map(|r| r.muted)
            .unwrap_or(false)
    }

    pub fn set_route_volume(&mut self, input_id: u32, output_id: u32, volume: u8) {
        if let Some(r) = self.routes.get_mut(&route_key(input_id, output_id)) {
            r.volume = volume;
        }
    }

    pub fn set_route_muted(&mut self, input_id: u32, output_id: u32, muted: bool) {
        if let Some(r) = self.routes.get_mut(&route_key(input_id, output_id)) {
            r.muted = muted;
        }
    }

    pub fn is_globally_muted(&self, input_id: u32) -> bool {
        self.outputs.iter().all(|o| self.route_muted(input_id, o.id))
    }

    /// Apply exponential decay to levels that weren't updated this cycle.
    /// Removes levels that have decayed below a threshold.
    pub fn decay_levels(&mut self) {
        let decay = self.level_decay as f32;
        self.input_levels.retain(|_, level| {
            *level *= decay;
            *level > 0.01
        });
    }

    pub fn next_output(&mut self) {
        if !self.outputs.is_empty() {
            self.current_output_index = (self.current_output_index + 1) % self.outputs.len();
        }
    }

    pub fn prev_output(&mut self) {
        if !self.outputs.is_empty() {
            if self.current_output_index == 0 {
                self.current_output_index = self.outputs.len() - 1;
            } else {
                self.current_output_index -= 1;
            }
        }
    }

    pub fn build_snapshot(&self) -> DisplayState {
        let outputs: Vec<OutputTab> = self
            .outputs
            .iter()
            .enumerate()
            .map(|(i, o)| OutputTab {
                id: o.id,
                name: o.name.clone(),
                color: o.color,
                is_current: i == self.current_output_index,
            })
            .collect();

        let current_output_id = outputs
            .get(self.current_output_index)
            .map(|o| o.id)
            .unwrap_or(0);

        let combined = self.inputs.len() + self.custom_inputs.len();
        let start = (self.current_page * 4) as usize;
        let mut visible_inputs: [Option<SlotView>; 4] = [None, None, None, None];

        for i in 0..4usize {
            let idx = start + i;
            if idx < self.inputs.len() {
                let inp = &self.inputs[idx];

                let route = self
                    .routes
                    .get(&route_key(inp.id, current_output_id))
                    .cloned()
                    .unwrap_or(RouteEntry {
                        volume: 100,
                        muted: false,
                    });

                let global_muted = self.is_globally_muted(inp.id);

                let level = if self.levels_enabled {
                    self.input_levels.get(&inp.id).copied()
                } else {
                    None
                };

                let streams = self
                    .streams_by_input
                    .get(&inp.id)
                    .cloned()
                    .unwrap_or_default();

                visible_inputs[i] = Some(SlotView {
                    input_id: inp.id,
                    name: inp.name.clone(),
                    color: inp.color,
                    volume: route.volume,
                    route_muted: route.muted,
                    global_muted,
                    level,
                    streams,
                    is_custom: false,
                });
            } else if idx < combined {
                let ci_idx = idx - self.inputs.len();
                let ci = &self.custom_inputs[ci_idx];
                visible_inputs[i] = Some(SlotView {
                    input_id: ci.id,
                    name: ci.name.clone(),
                    color: ci.color,
                    volume: ci.value,
                    route_muted: false,
                    global_muted: false,
                    level: None,
                    streams: vec![],
                    is_custom: true,
                });
            }
        }

        DisplayState {
            current_output_index: self.current_output_index,
            outputs,
            visible_inputs,
            page: self.current_page,
            total_pages: self.max_page() + 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_inputs(n: usize) -> Vec<InputEntry> {
        (0..n)
            .map(|i| InputEntry {
                id: (i + 1) as u32,
                name: format!("Input{}", i + 1),
                color: (100, 100, 100),
            })
            .collect()
    }

    fn make_outputs(n: usize) -> Vec<OutputEntry> {
        (0..n)
            .map(|i| OutputEntry {
                id: (100 + i) as u32,
                name: format!("Output{}", i + 1),
                color: (200, 200, 200),
            })
            .collect()
    }

    fn make_state_with(n_inputs: usize, n_outputs: usize) -> BeacnState {
        let inputs = make_inputs(n_inputs);
        let outputs = make_outputs(n_outputs);
        let mut routes = HashMap::new();
        for inp in &inputs {
            for out in &outputs {
                routes.insert(
                    route_key(inp.id, out.id),
                    RouteEntry { volume: 100, muted: false },
                );
            }
        }
        BeacnState {
            inputs,
            outputs,
            routes,
            current_output_index: 0,
            current_page: 0,
            streams_by_input: HashMap::new(),
            input_levels: HashMap::new(),
            levels_enabled: false,
            dial_sensitivity: 2,
            level_decay: 0.8,
            custom_inputs: Vec::new(),
        }
    }

    #[test]
    fn build_snapshot_empty() {
        let state = make_state_with(0, 0);
        let snap = state.build_snapshot();
        assert!(snap.outputs.is_empty());
        assert!(snap.visible_inputs.iter().all(|v| v.is_none()));
        assert_eq!(snap.total_pages, 1); // max_page(0) = 0, +1 = 1
    }

    #[test]
    fn build_snapshot_4_inputs() {
        let state = make_state_with(4, 1);
        let snap = state.build_snapshot();
        assert_eq!(snap.visible_inputs.iter().filter(|v| v.is_some()).count(), 4);
        assert_eq!(snap.visible_inputs[0].as_ref().unwrap().input_id, 1);
        assert_eq!(snap.visible_inputs[3].as_ref().unwrap().input_id, 4);
    }

    #[test]
    fn build_snapshot_pagination() {
        let mut state = make_state_with(8, 1);
        state.current_page = 1;
        let snap = state.build_snapshot();
        // Page 1 should show inputs 5-8
        assert_eq!(snap.visible_inputs.iter().filter(|v| v.is_some()).count(), 4);
        assert_eq!(snap.visible_inputs[0].as_ref().unwrap().input_id, 5);
        assert_eq!(snap.visible_inputs[3].as_ref().unwrap().input_id, 8);
    }

    #[test]
    fn next_output_wraps() {
        let mut state = make_state_with(1, 3);
        assert_eq!(state.current_output_index, 0);
        state.next_output();
        assert_eq!(state.current_output_index, 1);
        state.next_output();
        assert_eq!(state.current_output_index, 2);
        state.next_output();
        assert_eq!(state.current_output_index, 0); // wraps
        state.next_output();
        assert_eq!(state.current_output_index, 1);
    }

    #[test]
    fn prev_output_wraps() {
        let mut state = make_state_with(1, 3);
        assert_eq!(state.current_output_index, 0);
        state.prev_output();
        assert_eq!(state.current_output_index, 2); // wraps to last
        state.prev_output();
        assert_eq!(state.current_output_index, 1);
    }

    #[test]
    fn is_globally_muted() {
        let mut state = make_state_with(2, 3);
        // Not globally muted initially (all routes unmuted)
        assert!(!state.is_globally_muted(1));

        // Mute input 1 on all outputs
        for out in &state.outputs.clone() {
            state.set_route_muted(1, out.id, true);
        }
        assert!(state.is_globally_muted(1));
        // Input 2 is still not globally muted
        assert!(!state.is_globally_muted(2));

        // Unmute one route for input 1 -> no longer globally muted
        let first_output_id = state.outputs[0].id;
        state.set_route_muted(1, first_output_id, false);
        assert!(!state.is_globally_muted(1));
    }

    #[test]
    fn max_page_calculation() {
        let mut state = make_state_with(0, 1);
        assert_eq!(state.max_page(), 0); // 0 inputs -> 0

        state.inputs = make_inputs(4);
        assert_eq!(state.max_page(), 0); // 4 inputs -> (4-1)/4 = 0

        state.inputs = make_inputs(5);
        assert_eq!(state.max_page(), 1); // 5 inputs -> (5-1)/4 = 1

        state.inputs = make_inputs(8);
        assert_eq!(state.max_page(), 1); // 8 inputs -> (8-1)/4 = 1
    }
}
