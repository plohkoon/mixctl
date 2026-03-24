//! Standalone test harness for the Beacn Mix Create device integration.
//!
//! Simulates the daemon with fake mixer state — no PipeWire, no D-Bus.
//! Plug in the device and interact: dials adjust volumes, buttons toggle mutes,
//! AudienceMix rotates outputs, PageLeft/Right pages through inputs.
//!
//! All state changes are printed to the terminal.

use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use mixctl_beacn_device::{DeviceCommand, DeviceEvent, DeviceThread};
use mixctl_beacn_display::{DeviceLayoutKind, DisplayState, OutputTab, SlotView};
use tracing::{info, Level};
use tracing_subscriber::EnvFilter;

/// Fake mixer state that mirrors what the daemon would hold.
struct FakeMixer {
    inputs: Vec<InputState>,
    outputs: Vec<OutputState>,
    /// Per (input_index, output_index) — volume and muted
    routes: Vec<Vec<RouteState>>,
    current_output_index: usize,
    current_page: u32,
    /// Simulated audio levels per input index
    levels: Vec<f32>,
    /// Whether level simulation is active
    levels_enabled: bool,
}

#[derive(Clone)]
struct InputState {
    id: u32,
    name: String,
    color: (u8, u8, u8),
}

#[derive(Clone)]
struct OutputState {
    id: u32,
    name: String,
    color: (u8, u8, u8),
}

#[derive(Clone)]
struct RouteState {
    volume: u8,
    muted: bool,
}

impl FakeMixer {
    fn new() -> Self {
        let inputs = vec![
            InputState { id: 1, name: "System".into(), color: (74, 144, 217) },
            InputState { id: 2, name: "Game".into(), color: (231, 76, 60) },
            InputState { id: 3, name: "Music".into(), color: (46, 204, 113) },
            InputState { id: 4, name: "Chat".into(), color: (243, 156, 18) },
            InputState { id: 5, name: "Browser".into(), color: (155, 89, 182) },
            InputState { id: 6, name: "Discord".into(), color: (88, 101, 242) },
        ];
        let outputs = vec![
            OutputState { id: 10, name: "Personal".into(), color: (142, 68, 173) },
            OutputState { id: 11, name: "Stream".into(), color: (52, 152, 219) },
            OutputState { id: 12, name: "VOD".into(), color: (230, 126, 34) },
        ];

        let num_inputs = inputs.len();
        let num_outputs = outputs.len();
        let routes = vec![
            vec![RouteState { volume: 100, muted: false }; num_outputs];
            num_inputs
        ];

        Self {
            inputs,
            outputs,
            routes,
            current_output_index: 0,
            current_page: 0,
            levels: vec![0.0; num_inputs],
            levels_enabled: true,
        }
    }

    fn max_page(&self) -> u32 {
        let n = self.inputs.len() as u32;
        if n == 0 { 0 } else { (n - 1) / 4 }
    }

    fn total_pages(&self) -> u32 {
        self.max_page() + 1
    }

    fn build_snapshot(&self) -> DisplayState {
        let outputs: Vec<OutputTab> = self.outputs.iter().enumerate().map(|(i, o)| {
            OutputTab {
                id: o.id,
                name: o.name.clone(),
                color: o.color,
                is_current: i == self.current_output_index,
            }
        }).collect();

        let start = (self.current_page * 4) as usize;
        let mut visible_inputs: [Option<SlotView>; 4] = [None, None, None, None];

        for i in 0..4usize {
            let idx = start + i;
            if idx < self.inputs.len() {
                let inp = &self.inputs[idx];
                let route = &self.routes[idx][self.current_output_index];

                // Global muted = muted on ALL outputs
                let global_muted = self.routes[idx].iter().all(|r| r.muted);

                let level = if self.levels_enabled {
                    Some(self.levels[idx])
                } else {
                    None
                };
                visible_inputs[i] = Some(SlotView {
                    input_id: inp.id,
                    name: inp.name.clone(),
                    color: inp.color,
                    volume: route.volume,
                    route_muted: route.muted,
                    global_muted,
                    level,
                    streams: vec![],
                });
            }
        }

        DisplayState {
            current_output_index: self.current_output_index,
            outputs,
            visible_inputs,
            page: self.current_page,
            total_pages: self.total_pages(),
        }
    }

    fn find_input_index(&self, input_id: u32) -> Option<usize> {
        self.inputs.iter().position(|i| i.id == input_id)
    }

    fn find_output_index(&self, output_id: u32) -> Option<usize> {
        self.outputs.iter().position(|o| o.id == output_id)
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(Level::INFO.into()))
        .init();

    let layout_name = env::args().nth(1).unwrap_or_else(|| "grid".into());
    let layout_kind = DeviceLayoutKind::from_str_loose(&layout_name);
    let layout = layout_kind.create_layout();

    println!("=== mixctl device test harness ===");
    println!("Layout: {layout_kind:?}  (options: grid, column, dial)");
    println!("No PipeWire, no D-Bus — fake mixer state only.");
    println!();
    println!("Inputs: System, Game, Music, Chat, Browser, Discord (2 pages)");
    println!("Outputs: Personal, Stream, VOD");
    println!();
    println!("Controls:");
    println!("  Dials 1-4       adjust volume (current output)");
    println!("  Dial press      toggle route mute (current output)");
    println!("  Audience 1-4    toggle global mute (all outputs)");
    println!("  AudienceMix     next output tab");
    println!("  PageLeft/Right  switch input page");
    println!();

    let shutdown_flag = Arc::new(AtomicBool::new(false));

    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel::<DeviceCommand>();
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<DeviceEvent>();

    let device_thread = DeviceThread::spawn(
        shutdown_flag.clone(),
        event_tx,
        cmd_rx,
        layout,
    );

    let mut mixer = FakeMixer::new();

    // Send initial state
    cmd_tx.send(DeviceCommand::UpdateState(mixer.build_snapshot())).ok();

    let sf = shutdown_flag.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("Ctrl-C received, shutting down");
        sf.store(true, Ordering::Relaxed);
    });

    // Level simulation channel
    let (level_tx, mut level_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<f32>>();
    let level_sf = shutdown_flag.clone();
    let num_inputs = mixer.inputs.len();
    tokio::spawn(async move {
        let mut phase: f32 = 0.0;
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(50));
        loop {
            interval.tick().await;
            if level_sf.load(Ordering::Relaxed) {
                break;
            }
            // Simulate different sine-wave levels per input
            let levels: Vec<f32> = (0..num_inputs)
                .map(|i| {
                    let freq = 0.5 + i as f32 * 0.3;
                    let val = ((phase * freq).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
                    // Add some noise for realism
                    (val * 0.8 + 0.1).clamp(0.0, 1.0)
                })
                .collect();
            phase += 0.15;
            level_tx.send(levels).ok();
        }
    });

    // Event loop — handle device events, update fake state, send back to device
    loop {
        let event = tokio::select! {
            e = event_rx.recv() => match e {
                Some(e) => e,
                None => break,
            },
            levels = level_rx.recv() => {
                if let Some(levels) = levels {
                    if mixer.levels_enabled {
                        mixer.levels = levels;
                        cmd_tx.send(DeviceCommand::UpdateState(mixer.build_snapshot())).ok();
                    }
                }
                continue;
            },
            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                if shutdown_flag.load(Ordering::Relaxed) {
                    break;
                }
                continue;
            }
        };

        match event {
            DeviceEvent::Connected => {
                info!("device connected — sending current state");
                cmd_tx.send(DeviceCommand::UpdateState(mixer.build_snapshot())).ok();
            }
            DeviceEvent::Disconnected => {
                info!("device disconnected");
            }
            DeviceEvent::AdjustRouteVolume { input_id, output_id, delta } => {
                if let (Some(ii), Some(oi)) = (mixer.find_input_index(input_id), mixer.find_output_index(output_id)) {
                    let route = &mut mixer.routes[ii][oi];
                    let old = route.volume;
                    route.volume = (route.volume as i16 + delta as i16 * 2).clamp(0, 100) as u8;
                    info!(
                        "volume: {} on {} : {} -> {}",
                        mixer.inputs[ii].name, mixer.outputs[oi].name,
                        old, route.volume
                    );
                    cmd_tx.send(DeviceCommand::UpdateState(mixer.build_snapshot())).ok();
                }
            }
            DeviceEvent::ToggleRouteMute { input_id, output_id } => {
                if let (Some(ii), Some(oi)) = (mixer.find_input_index(input_id), mixer.find_output_index(output_id)) {
                    let route = &mut mixer.routes[ii][oi];
                    route.muted = !route.muted;
                    info!(
                        "route mute: {} on {} = {}",
                        mixer.inputs[ii].name, mixer.outputs[oi].name,
                        if route.muted { "MUTED" } else { "unmuted" }
                    );
                    cmd_tx.send(DeviceCommand::UpdateState(mixer.build_snapshot())).ok();
                }
            }
            DeviceEvent::ToggleGlobalMute { input_id } => {
                if let Some(ii) = mixer.find_input_index(input_id) {
                    let all_muted = mixer.routes[ii].iter().all(|r| r.muted);
                    let new_muted = !all_muted;
                    for route in &mut mixer.routes[ii] {
                        route.muted = new_muted;
                    }
                    info!(
                        "global mute: {} = {}",
                        mixer.inputs[ii].name,
                        if new_muted { "MUTED on all outputs" } else { "unmuted on all outputs" }
                    );
                    cmd_tx.send(DeviceCommand::UpdateState(mixer.build_snapshot())).ok();
                }
            }
            DeviceEvent::NextOutput => {
                let count = mixer.outputs.len();
                mixer.current_output_index = (mixer.current_output_index + 1) % count;
                info!(
                    "output: {} ({}/{})",
                    mixer.outputs[mixer.current_output_index].name,
                    mixer.current_output_index + 1,
                    count
                );
                cmd_tx.send(DeviceCommand::UpdateState(mixer.build_snapshot())).ok();
            }
            DeviceEvent::PrevOutput => {
                let count = mixer.outputs.len();
                mixer.current_output_index = if mixer.current_output_index == 0 { count - 1 } else { mixer.current_output_index - 1 };
                info!(
                    "output: {} ({}/{})",
                    mixer.outputs[mixer.current_output_index].name,
                    mixer.current_output_index + 1,
                    count
                );
                cmd_tx.send(DeviceCommand::UpdateState(mixer.build_snapshot())).ok();
            }
            DeviceEvent::PageLeft => {
                if mixer.current_page > 0 {
                    mixer.current_page -= 1;
                    info!("page: {}/{}", mixer.current_page + 1, mixer.total_pages());
                    cmd_tx.send(DeviceCommand::UpdateState(mixer.build_snapshot())).ok();
                }
            }
            DeviceEvent::PageRight => {
                if mixer.current_page < mixer.max_page() {
                    mixer.current_page += 1;
                    info!("page: {}/{}", mixer.current_page + 1, mixer.total_pages());
                    cmd_tx.send(DeviceCommand::UpdateState(mixer.build_snapshot())).ok();
                }
            }
            other => {
                info!("unhandled event in test harness: {other:?}");
            }
        }
    }

    info!("sending shutdown to device");
    cmd_tx.send(DeviceCommand::Shutdown).ok();
    drop(cmd_tx);

    // Wait for device thread
    device_thread.join();
    info!("done");
}
