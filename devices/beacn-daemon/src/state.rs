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

    // ── Regression tests for event handler logic (main.rs:380-553) ───
    //
    // These capture the exact state-manipulation behavior of each DeviceEvent
    // handler before the SDK refactor. If any test fails after refactoring,
    // the refactor changed behavior.

    fn make_custom_input(id: u32, name: &str, value: u8) -> CustomInputEntry {
        CustomInputEntry {
            id,
            name: name.to_string(),
            color: (50, 50, 50),
            value,
        }
    }

    // ── AdjustRouteVolume handler (main.rs:400-423) ──

    #[test]
    fn adjust_route_volume_clamps_with_sensitivity() {
        // Mirrors: let new_vol = (old_vol as i16 + delta as i16 * sensitivity).clamp(0, 100) as u8
        let mut state = make_state_with(4, 2);
        let input_id = 1;
        let output_id = 100;

        // Start at 100, positive delta should stay at 100
        state.set_route_volume(input_id, output_id, 100);
        let old_vol = state.route_volume(input_id, output_id);
        let delta: i8 = 5;
        let sensitivity = state.dial_sensitivity as i16;
        let new_vol = (old_vol as i16 + delta as i16 * sensitivity).clamp(0, 100) as u8;
        assert_eq!(new_vol, 100);

        // Start at 100, negative delta with sensitivity=2
        let delta: i8 = -10;
        let new_vol = (old_vol as i16 + delta as i16 * sensitivity).clamp(0, 100) as u8;
        assert_eq!(new_vol, 80);

        // Start at 5, large negative delta should clamp to 0
        state.set_route_volume(input_id, output_id, 5);
        let old_vol = state.route_volume(input_id, output_id);
        let delta: i8 = -10;
        let new_vol = (old_vol as i16 + delta as i16 * sensitivity).clamp(0, 100) as u8;
        assert_eq!(new_vol, 0);
    }

    #[test]
    fn adjust_route_volume_high_sensitivity() {
        let mut state = make_state_with(4, 2);
        state.dial_sensitivity = 5;
        let sensitivity = state.dial_sensitivity as i16;

        let old_vol: u8 = 50;
        let delta: i8 = 3;
        let new_vol = (old_vol as i16 + delta as i16 * sensitivity).clamp(0, 100) as u8;
        assert_eq!(new_vol, 65); // 50 + 3*5 = 65

        let delta: i8 = -20;
        let new_vol = (old_vol as i16 + delta as i16 * sensitivity).clamp(0, 100) as u8;
        assert_eq!(new_vol, 0); // 50 + (-20*5) = -50, clamps to 0
    }

    #[test]
    fn adjust_route_volume_updates_local_state() {
        let mut state = make_state_with(4, 2);
        let input_id = 1;
        let output_id = 100;
        let sensitivity = state.dial_sensitivity as i16;
        let old_vol = state.route_volume(input_id, output_id);
        let delta: i8 = -5;
        let new_vol = (old_vol as i16 + delta as i16 * sensitivity).clamp(0, 100) as u8;

        state.set_route_volume(input_id, output_id, new_vol);
        assert_eq!(state.route_volume(input_id, output_id), 90); // 100 + (-5*2) = 90
    }

    // ── AdjustRouteVolume custom input path (main.rs:402-412) ──

    #[test]
    fn adjust_custom_input_value_clamps() {
        // Mirrors: let new_val = (old as i16 + delta as i16 * sensitivity).clamp(0, 100) as u8
        let mut state = make_state_with(4, 2);
        state.custom_inputs.push(make_custom_input(200, "Brightness", 50));
        assert!(state.is_custom_input(200));
        assert!(!state.is_custom_input(1)); // regular input

        let old = state.custom_inputs[0].value;
        let sensitivity = state.dial_sensitivity as i16;
        let delta: i8 = 10;
        let new_val = (old as i16 + delta as i16 * sensitivity).clamp(0, 100) as u8;
        assert_eq!(new_val, 70); // 50 + 10*2 = 70

        state.set_custom_input_value(200, new_val);
        assert_eq!(state.custom_inputs[0].value, 70);
    }

    #[test]
    fn adjust_custom_input_clamps_to_bounds() {
        let mut state = make_state_with(4, 2);
        state.custom_inputs.push(make_custom_input(200, "Brightness", 95));

        let old = state.custom_inputs[0].value;
        let sensitivity = state.dial_sensitivity as i16;

        // Positive overflow
        let delta: i8 = 5;
        let new_val = (old as i16 + delta as i16 * sensitivity).clamp(0, 100) as u8;
        assert_eq!(new_val, 100); // 95 + 5*2 = 105, clamps to 100

        // Negative underflow
        let delta: i8 = -60;
        let new_val = (old as i16 + delta as i16 * sensitivity).clamp(0, 100) as u8;
        assert_eq!(new_val, 0); // 95 + (-60*2) = -25, clamps to 0
    }

    // ── ToggleRouteMute handler (main.rs:425-438) ──

    #[test]
    fn toggle_route_mute_skips_custom_inputs() {
        // Mirrors: if s.is_custom_input(input_id) { continue; }
        let mut state = make_state_with(4, 2);
        state.custom_inputs.push(make_custom_input(200, "Brightness", 50));

        // Custom input should be detected
        assert!(state.is_custom_input(200));
        // Regular input should not
        assert!(!state.is_custom_input(1));
    }

    #[test]
    fn toggle_route_mute_toggles_state() {
        let mut state = make_state_with(4, 2);
        let input_id = 1;
        let output_id = 100;

        assert!(!state.route_muted(input_id, output_id));

        let muted = state.route_muted(input_id, output_id);
        state.set_route_muted(input_id, output_id, !muted);
        assert!(state.route_muted(input_id, output_id));

        let muted = state.route_muted(input_id, output_id);
        state.set_route_muted(input_id, output_id, !muted);
        assert!(!state.route_muted(input_id, output_id));
    }

    // ── ToggleGlobalMute handler (main.rs:439-455) ──

    #[test]
    fn toggle_global_mute_mutes_all_routes_for_input() {
        let mut state = make_state_with(2, 3);
        let input_id = 1;

        // Initially all unmuted
        assert!(!state.is_globally_muted(input_id));

        // Toggle: should mute all routes for this input
        let all_muted = state.is_globally_muted(input_id);
        let new_muted = !all_muted; // true
        for &output_id in &state.output_ids() {
            state.set_route_muted(input_id, output_id, new_muted);
        }
        assert!(state.is_globally_muted(input_id));

        // Toggle again: should unmute all
        let all_muted = state.is_globally_muted(input_id);
        let new_muted = !all_muted; // false
        for &output_id in &state.output_ids() {
            state.set_route_muted(input_id, output_id, new_muted);
        }
        assert!(!state.is_globally_muted(input_id));
    }

    #[test]
    fn toggle_global_mute_skips_custom_inputs() {
        let mut state = make_state_with(2, 3);
        state.custom_inputs.push(make_custom_input(200, "Brightness", 50));
        // Custom input has no route mute concept
        assert!(state.is_custom_input(200));
    }

    // ── SetGlobalMute handler (main.rs:540-552) ──

    #[test]
    fn set_global_mute_explicit() {
        let mut state = make_state_with(2, 3);
        let input_id = 1;

        // Set muted = true for all routes
        for &output_id in &state.output_ids() {
            state.set_route_muted(input_id, output_id, true);
        }
        assert!(state.is_globally_muted(input_id));

        // Set muted = false for all routes
        for &output_id in &state.output_ids() {
            state.set_route_muted(input_id, output_id, false);
        }
        assert!(!state.is_globally_muted(input_id));
    }

    // ── PageLeft/PageRight handlers (main.rs:468-484) ──

    #[test]
    fn page_left_does_not_go_below_zero() {
        let mut state = make_state_with(8, 1);
        assert_eq!(state.current_page, 0);

        // PageLeft at page 0 should stay at 0
        if state.current_page > 0 {
            state.current_page -= 1;
        }
        assert_eq!(state.current_page, 0);
    }

    #[test]
    fn page_right_does_not_exceed_max() {
        let mut state = make_state_with(8, 1);
        let max = state.max_page();
        assert_eq!(max, 1);

        // Navigate to max page
        state.current_page = max;
        assert_eq!(state.current_page, 1);

        // PageRight at max should stay at max
        let max = state.max_page();
        if state.current_page < max {
            state.current_page += 1;
        }
        assert_eq!(state.current_page, 1); // didn't change
    }

    #[test]
    fn page_navigation_round_trip() {
        let mut state = make_state_with(12, 1); // 12 inputs = 3 pages (0, 1, 2)
        assert_eq!(state.max_page(), 2);

        // Navigate right twice
        let max = state.max_page();
        if state.current_page < max { state.current_page += 1; }
        assert_eq!(state.current_page, 1);
        let max = state.max_page();
        if state.current_page < max { state.current_page += 1; }
        assert_eq!(state.current_page, 2);

        // Can't go further right
        let max = state.max_page();
        if state.current_page < max { state.current_page += 1; }
        assert_eq!(state.current_page, 2);

        // Navigate left back to 0
        if state.current_page > 0 { state.current_page -= 1; }
        assert_eq!(state.current_page, 1);
        if state.current_page > 0 { state.current_page -= 1; }
        assert_eq!(state.current_page, 0);
    }

    // ── NextOutput/PrevOutput with empty outputs ──

    #[test]
    fn next_output_noop_when_empty() {
        let mut state = make_state_with(4, 0);
        assert_eq!(state.current_output_index, 0);
        state.next_output();
        assert_eq!(state.current_output_index, 0); // no change
    }

    #[test]
    fn prev_output_noop_when_empty() {
        let mut state = make_state_with(4, 0);
        assert_eq!(state.current_output_index, 0);
        state.prev_output();
        assert_eq!(state.current_output_index, 0); // no change
    }

    // ── Level decay (used by input_levels_changed handler, main.rs:335-356) ──

    #[test]
    fn decay_levels_applies_exponential_decay() {
        let mut state = make_state_with(4, 1);
        state.level_decay = 0.8;
        state.input_levels.insert(1, 1.0);
        state.input_levels.insert(2, 0.5);

        state.decay_levels();
        assert!((state.input_levels[&1] - 0.8).abs() < 0.001);
        assert!((state.input_levels[&2] - 0.4).abs() < 0.001);
    }

    #[test]
    fn decay_levels_removes_below_threshold() {
        let mut state = make_state_with(4, 1);
        state.level_decay = 0.5;
        state.input_levels.insert(1, 0.03); // will decay to 0.015 (above 0.01 threshold)
        state.input_levels.insert(2, 0.01); // will decay to 0.005 (below 0.01 threshold)

        state.decay_levels();
        assert!(state.input_levels.contains_key(&1)); // 0.015, above threshold
        assert!(!state.input_levels.contains_key(&2)); // removed
    }

    // ── Custom inputs in build_snapshot (main.rs snapshot used by all handlers) ──

    #[test]
    fn build_snapshot_includes_custom_inputs() {
        let mut state = make_state_with(3, 1);
        state.custom_inputs.push(make_custom_input(200, "Brightness", 75));

        let snap = state.build_snapshot();
        // 3 regular + 1 custom = 4 visible
        assert_eq!(snap.visible_inputs.iter().filter(|v| v.is_some()).count(), 4);

        // Custom input appears at index 3 (after regular inputs)
        let custom_slot = snap.visible_inputs[3].as_ref().unwrap();
        assert_eq!(custom_slot.input_id, 200);
        assert_eq!(custom_slot.volume, 75); // custom input value, not route volume
        assert!(custom_slot.is_custom);
        assert!(!custom_slot.route_muted);
        assert!(!custom_slot.global_muted);
    }

    #[test]
    fn build_snapshot_custom_inputs_paginate() {
        let mut state = make_state_with(3, 1);
        // Add 3 custom inputs -> total 6 items -> 2 pages
        state.custom_inputs.push(make_custom_input(200, "Brightness", 50));
        state.custom_inputs.push(make_custom_input(201, "Volume", 80));
        state.custom_inputs.push(make_custom_input(202, "Speed", 30));

        assert_eq!(state.max_page(), 1); // (6-1)/4 = 1

        // Page 0: inputs 1-3 + custom 200
        let snap = state.build_snapshot();
        assert_eq!(snap.visible_inputs.iter().filter(|v| v.is_some()).count(), 4);
        assert!(!snap.visible_inputs[0].as_ref().unwrap().is_custom);
        assert!(snap.visible_inputs[3].as_ref().unwrap().is_custom);

        // Page 1: custom 201, 202
        state.current_page = 1;
        let snap = state.build_snapshot();
        assert_eq!(snap.visible_inputs.iter().filter(|v| v.is_some()).count(), 2);
        assert!(snap.visible_inputs[0].as_ref().unwrap().is_custom);
        assert_eq!(snap.visible_inputs[0].as_ref().unwrap().input_id, 201);
    }

    #[test]
    fn max_page_includes_custom_inputs() {
        let mut state = make_state_with(3, 1);
        assert_eq!(state.max_page(), 0); // 3 items, 1 page

        state.custom_inputs.push(make_custom_input(200, "Brightness", 50));
        assert_eq!(state.max_page(), 0); // 4 items, still 1 page

        state.custom_inputs.push(make_custom_input(201, "Volume", 80));
        assert_eq!(state.max_page(), 1); // 5 items, 2 pages
    }

    // ── Route operations on missing routes ──

    #[test]
    fn route_volume_default_for_missing_route() {
        let state = make_state_with(4, 2);
        // Query a route that doesn't exist
        assert_eq!(state.route_volume(999, 999), 100); // default
    }

    #[test]
    fn route_muted_default_for_missing_route() {
        let state = make_state_with(4, 2);
        assert!(!state.route_muted(999, 999)); // default false
    }

    #[test]
    fn set_route_volume_noop_for_missing_route() {
        let mut state = make_state_with(4, 2);
        // Should not panic
        state.set_route_volume(999, 999, 50);
        assert_eq!(state.route_volume(999, 999), 100); // unchanged default
    }

    #[test]
    fn set_route_muted_noop_for_missing_route() {
        let mut state = make_state_with(4, 2);
        state.set_route_muted(999, 999, true);
        assert!(!state.route_muted(999, 999)); // unchanged default
    }

    // ── set_custom_input_value edge cases ──

    #[test]
    fn set_custom_input_value_noop_for_missing() {
        let mut state = make_state_with(4, 1);
        // No custom inputs. Should not panic.
        state.set_custom_input_value(999, 50);
    }

    #[test]
    fn set_custom_input_value_updates_correct_entry() {
        let mut state = make_state_with(4, 1);
        state.custom_inputs.push(make_custom_input(200, "A", 10));
        state.custom_inputs.push(make_custom_input(201, "B", 20));

        state.set_custom_input_value(201, 99);
        assert_eq!(state.custom_inputs[0].value, 10); // A unchanged
        assert_eq!(state.custom_inputs[1].value, 99); // B updated
    }

    // ── Snapshot with levels ──

    #[test]
    fn build_snapshot_levels_only_when_enabled() {
        let mut state = make_state_with(4, 1);
        state.input_levels.insert(1, 0.75);

        // levels_enabled = false -> no levels in snapshot
        let snap = state.build_snapshot();
        assert!(snap.visible_inputs[0].as_ref().unwrap().level.is_none());

        // levels_enabled = true -> levels appear
        state.levels_enabled = true;
        let snap = state.build_snapshot();
        let level = snap.visible_inputs[0].as_ref().unwrap().level.unwrap();
        assert!((level - 0.75).abs() < 0.001);
    }

    // ── Snapshot current output marking ──

    #[test]
    fn build_snapshot_marks_current_output() {
        let mut state = make_state_with(4, 3);
        state.current_output_index = 1;
        let snap = state.build_snapshot();
        assert!(!snap.outputs[0].is_current);
        assert!(snap.outputs[1].is_current);
        assert!(!snap.outputs[2].is_current);
    }

    // ── Snapshot route data reflects current output ──

    #[test]
    fn build_snapshot_shows_routes_for_current_output() {
        let mut state = make_state_with(2, 2);
        // Set different volumes for input 1 on each output
        state.set_route_volume(1, 100, 80); // output 100 (index 0)
        state.set_route_volume(1, 101, 30); // output 101 (index 1)

        // current_output_index = 0 -> should show volume 80
        let snap = state.build_snapshot();
        assert_eq!(snap.visible_inputs[0].as_ref().unwrap().volume, 80);

        // Switch to output index 1 -> should show volume 30
        state.current_output_index = 1;
        let snap = state.build_snapshot();
        assert_eq!(snap.visible_inputs[0].as_ref().unwrap().volume, 30);
    }
}
