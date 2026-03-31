//! Fake device adapter for testing the mixctl adapter SDK.
//!
//! Simulates a MIDI-style controller with 8 faders and 16 buttons (no screen,
//! no LEDs). Deliberately different from the Beacn Mix Create to validate
//! that the SDK's capability model generalizes beyond a single device.
//!
//! Generates random fader movements and button presses at configurable intervals
//! to exercise the full adapter contract.
//!
//! Usage:
//!   cargo run -p mixctl-fake-device
//!   cargo run -p mixctl-fake-device -- --interval 500   # slower events (ms)
//!   cargo run -p mixctl-fake-device -- --quiet           # no simulated input

use std::sync::atomic::Ordering;
use std::time::Duration;

use mixctl_adapter_sdk::{
    AdapterRunner, ButtonKind, Capability, DeviceAdapter, MixCtlProxy, MixerEvent,
};
use tokio::sync::mpsc;
use tracing::{info, warn, Level};
use tracing_subscriber::EnvFilter;

/// Simulated hardware event from the fake device "thread"
#[derive(Debug)]
enum FakeHwEvent {
    FaderMoved { index: u8, delta: i8 },
    ButtonPressed { index: u8 },
}

struct FakeDeviceAdapter {
    hw_rx: mpsc::UnboundedReceiver<FakeHwEvent>,
    /// Cached input/output IDs from the mixer daemon
    input_ids: Vec<u32>,
    output_ids: Vec<u32>,
    /// Local route volume cache for fader-to-volume mapping
    route_volumes: std::collections::HashMap<(u32, u32), u8>,
    /// Which output the faders currently control
    current_output_id: Option<u32>,
}

impl FakeDeviceAdapter {
    fn new(hw_rx: mpsc::UnboundedReceiver<FakeHwEvent>) -> Self {
        Self {
            hw_rx,
            input_ids: Vec::new(),
            output_ids: Vec::new(),
            route_volumes: std::collections::HashMap::new(),
            current_output_id: None,
        }
    }

    async fn refresh_state(&mut self, proxy: &MixCtlProxy<'static>) {
        match proxy.list_inputs().await {
            Ok(inputs) => self.input_ids = inputs.iter().map(|i| i.id).collect(),
            Err(e) => warn!("failed to list inputs: {e}"),
        }
        match proxy.list_outputs().await {
            Ok(outputs) => {
                self.output_ids = outputs.iter().map(|o| o.id).collect();
                if self.current_output_id.is_none() {
                    self.current_output_id = self.output_ids.first().copied();
                }
            }
            Err(e) => warn!("failed to list outputs: {e}"),
        }
        // Fetch route volumes for current output
        if let Some(out_id) = self.current_output_id {
            match proxy.list_routes_for_output(out_id).await {
                Ok(routes) => {
                    for r in routes {
                        self.route_volumes
                            .insert((r.input_id, r.output_id), r.volume);
                    }
                }
                Err(e) => warn!("failed to list routes: {e}"),
            }
        }
    }
}

impl DeviceAdapter for FakeDeviceAdapter {
    fn capabilities(&self) -> Vec<Capability> {
        vec![
            Capability::Fader {
                count: 8,
                range: (0.0, 1.0),
            },
            Capability::Button {
                count: 16,
                kind: ButtonKind::Momentary,
            },
        ]
    }

    fn device_name(&self) -> &str {
        "fake-midi-controller"
    }

    async fn run(
        &mut self,
        proxy: MixCtlProxy<'static>,
        mut mixer_events: mpsc::UnboundedReceiver<MixerEvent>,
    ) -> anyhow::Result<()> {
        // Initial state fetch
        self.refresh_state(&proxy).await;
        info!(
            inputs = self.input_ids.len(),
            outputs = self.output_ids.len(),
            "fake device ready"
        );

        loop {
            tokio::select! {
                // Handle mixer state changes
                event = mixer_events.recv() => {
                    let Some(event) = event else { break };
                    match event {
                        MixerEvent::InputsChanged
                        | MixerEvent::OutputsChanged
                        | MixerEvent::RouteChanged { .. }
                        | MixerEvent::OutputStateChanged { .. } => {
                            self.refresh_state(&proxy).await;
                        }
                        MixerEvent::LevelsChanged { levels } => {
                            // A real device with meters would display these.
                            // We just log occasionally for visibility.
                            if !levels.is_empty() {
                                tracing::trace!(
                                    count = levels.len(),
                                    "levels update"
                                );
                            }
                        }
                        _ => {} // ignore events we don't care about
                    }
                }

                // Handle simulated hardware input
                hw = self.hw_rx.recv() => {
                    let Some(hw) = hw else { break };
                    match hw {
                        FakeHwEvent::FaderMoved { index, delta } => {
                            // Map fader index to input_id (fader N controls input N)
                            let Some(&input_id) = self.input_ids.get(index as usize) else {
                                continue;
                            };
                            let Some(output_id) = self.current_output_id else {
                                continue;
                            };

                            let old_vol = self
                                .route_volumes
                                .get(&(input_id, output_id))
                                .copied()
                                .unwrap_or(100);
                            let new_vol =
                                (old_vol as i16 + delta as i16).clamp(0, 100) as u8;

                            if let Err(e) =
                                proxy.set_route_volume(input_id, output_id, new_vol).await
                            {
                                warn!("set_route_volume failed: {e}");
                            }
                            self.route_volumes.insert((input_id, output_id), new_vol);
                            info!(
                                fader = index,
                                input_id,
                                output_id,
                                old_vol,
                                new_vol,
                                "fader moved"
                            );
                        }
                        FakeHwEvent::ButtonPressed { index } => {
                            // Buttons 0-7: toggle mute on input N
                            // Buttons 8-15: switch to output N-8
                            if index < 8 {
                                let Some(&input_id) = self.input_ids.get(index as usize)
                                else {
                                    continue;
                                };
                                let Some(output_id) = self.current_output_id else {
                                    continue;
                                };
                                match proxy
                                    .get_route(input_id, output_id)
                                    .await
                                {
                                    Ok(route) => {
                                        proxy
                                            .set_route_mute(
                                                input_id,
                                                output_id,
                                                !route.muted,
                                            )
                                            .await
                                            .ok();
                                        info!(
                                            button = index,
                                            input_id,
                                            output_id,
                                            muted = !route.muted,
                                            "toggle mute"
                                        );
                                    }
                                    Err(e) => warn!("get_route failed: {e}"),
                                }
                            } else {
                                let out_idx = (index - 8) as usize;
                                if let Some(&out_id) = self.output_ids.get(out_idx) {
                                    self.current_output_id = Some(out_id);
                                    self.refresh_state(&proxy).await;
                                    info!(
                                        button = index,
                                        output_id = out_id,
                                        "switched output"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn shutdown(&mut self) {
        info!("fake device shutting down");
    }
}

/// Spawn a task that generates random hardware events at an interval
fn spawn_hw_simulator(
    tx: mpsc::UnboundedSender<FakeHwEvent>,
    interval: Duration,
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        use rand::Rng;

        loop {
            tokio::time::sleep(interval).await;
            if shutdown.load(Ordering::Acquire) {
                break;
            }

            // Generate event using thread_rng (not held across await)
            let event = {
                let mut rng = rand::rng();
                if rng.random_bool(0.7) {
                    FakeHwEvent::FaderMoved {
                        index: rng.random_range(0..8),
                        delta: rng.random_range(-5..=5),
                    }
                } else {
                    FakeHwEvent::ButtonPressed {
                        index: rng.random_range(0..16),
                    }
                }
            };

            if tx.send(event).is_err() {
                break;
            }
        }
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(Level::INFO.into()))
        .init();

    // Parse args
    let args: Vec<String> = std::env::args().collect();
    let quiet = args.iter().any(|a| a == "--quiet");
    let interval_ms: u64 = args
        .windows(2)
        .find(|w| w[0] == "--interval")
        .and_then(|w| w[1].parse().ok())
        .unwrap_or(1000);

    info!("mixctl-fake-device starting (interval={}ms, quiet={})", interval_ms, quiet);

    let runner = AdapterRunner::new();
    let shutdown = runner.shutdown_flag();

    // Signal handler
    let sf = shutdown.clone();
    tokio::spawn(async move {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to register SIGTERM");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
        sf.store(true, Ordering::Release);
    });

    // Create the hardware event channel
    let (hw_tx, hw_rx) = mpsc::unbounded_channel();

    // Spawn the simulated hardware thread (unless --quiet)
    let _hw_handle = if !quiet {
        Some(spawn_hw_simulator(
            hw_tx,
            Duration::from_millis(interval_ms),
            shutdown.clone(),
        ))
    } else {
        drop(hw_tx);
        None
    };

    let mut adapter = FakeDeviceAdapter::new(hw_rx);
    runner.run(&mut adapter).await
}
