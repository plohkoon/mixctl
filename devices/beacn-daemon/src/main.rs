mod state;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use mixctl_adapter_sdk::{
    AdapterRunner, ButtonKind, Capability, ColorMode, DeviceAdapter, MixCtlProxy, MixerEvent,
    ScreenFormat,
};
use mixctl_core::config_sections::BeacnConfig;
use mixctl_beacn_device::{DeviceCommand, DeviceEvent, DeviceThread};
use mixctl_beacn_display::DeviceLayoutKind;
use tokio::sync::mpsc;
use tracing::{info, warn, Level};
use tracing_subscriber::EnvFilter;

use crate::state::BeacnState;

/// Drop guard that ensures the device thread receives a Shutdown command
/// regardless of how the process exits (panic, early return, signal).
struct ShutdownGuard {
    tx: Option<mpsc::UnboundedSender<DeviceCommand>>,
    flag: Arc<AtomicBool>,
}

impl Drop for ShutdownGuard {
    fn drop(&mut self) {
        self.flag.store(true, Ordering::Release);
        if let Some(tx) = self.tx.take() {
            tx.send(DeviceCommand::Shutdown).ok();
        }
    }
}

struct BeacnAdapter {
    state: BeacnState,
    dev_cmd_tx: mpsc::UnboundedSender<DeviceCommand>,
    dev_event_rx: mpsc::UnboundedReceiver<DeviceEvent>,
    shutdown_flag: Arc<AtomicBool>,
}

impl BeacnAdapter {
    fn new(
        dev_cmd_tx: mpsc::UnboundedSender<DeviceCommand>,
        dev_event_rx: mpsc::UnboundedReceiver<DeviceEvent>,
        shutdown_flag: Arc<AtomicBool>,
    ) -> Self {
        Self {
            state: BeacnState::new_with_config(3, 0.85),
            dev_cmd_tx,
            dev_event_rx,
            shutdown_flag,
        }
    }

    /// Fetch and apply beacn config from the mixer daemon.
    async fn apply_config(&mut self, proxy: &MixCtlProxy<'static>) {
        let beacn_config = match proxy.get_config_section("beacn").await {
            Ok(json) => serde_json::from_str::<BeacnConfig>(&json).unwrap_or_default(),
            Err(_) => BeacnConfig::default(),
        };

        self.state.dial_sensitivity = beacn_config.dial_sensitivity;
        self.state.level_decay = beacn_config.level_decay;

        let layout_kind = DeviceLayoutKind::from_str_loose(&beacn_config.layout);
        self.dev_cmd_tx
            .send(DeviceCommand::ChangeLayout(layout_kind.create_layout()))
            .ok();
        self.dev_cmd_tx
            .send(DeviceCommand::SetBrightness {
                display: beacn_config.display_brightness,
                led: beacn_config.led_brightness,
            })
            .ok();
        self.dev_cmd_tx
            .send(DeviceCommand::SetButtonConfig {
                mappings: beacn_config.button_mappings.clone(),
                hold_threshold: Duration::from_millis(beacn_config.hold_threshold_ms),
            })
            .ok();

        info!(
            "beacn config: layout={}, dial_sensitivity={}, brightness={}:{}, level_decay={}",
            beacn_config.layout,
            beacn_config.dial_sensitivity,
            beacn_config.display_brightness,
            beacn_config.led_brightness,
            beacn_config.level_decay
        );
    }

    /// Full state refresh from D-Bus + send snapshot to device.
    async fn refresh_all(&mut self, proxy: &MixCtlProxy<'static>) {
        if let Err(e) = self.state.refresh_from_dbus(proxy).await {
            warn!("D-Bus refresh failed: {e}");
            return;
        }
        if let Err(e) = self.state.refresh_custom_inputs(proxy).await {
            warn!("custom input refresh failed: {e}");
        }
        self.send_snapshot();
    }

    /// Send current state snapshot to the device thread.
    fn send_snapshot(&self) {
        let snapshot = self.state.build_snapshot();
        self.dev_cmd_tx
            .send(DeviceCommand::UpdateState(snapshot))
            .ok();
    }

    /// Handle a mixer event from the SDK.
    async fn handle_mixer_event(
        &mut self,
        event: MixerEvent,
        proxy: &MixCtlProxy<'static>,
    ) {
        match event {
            MixerEvent::InputsChanged
            | MixerEvent::OutputsChanged
            | MixerEvent::OutputStateChanged { .. }
            | MixerEvent::RouteChanged { .. } => {
                self.refresh_all(proxy).await;
            }
            MixerEvent::StreamsChanged => {
                if let Err(e) = self.state.refresh_streams(proxy).await {
                    warn!("stream refresh failed: {e}");
                    return;
                }
                self.send_snapshot();
            }
            MixerEvent::BroadcastLevelsChanged { enabled } => {
                self.state.levels_enabled = enabled;
                if !enabled {
                    self.state.input_levels.clear();
                }
                self.send_snapshot();
                if enabled {
                    self.refresh_all(proxy).await;
                }
            }
            MixerEvent::ConfigSectionChanged { ref section } => {
                if section == "beacn" {
                    self.apply_config(proxy).await;
                    self.send_snapshot();
                }
            }
            MixerEvent::CustomInputChanged { .. } => {
                if let Err(e) = self.state.refresh_custom_inputs(proxy).await {
                    warn!("custom input refresh failed: {e}");
                    return;
                }
                self.send_snapshot();
            }
            MixerEvent::LevelsChanged { levels } => {
                if !self.state.levels_enabled {
                    return;
                }
                self.state.decay_levels();
                for &(id, level) in &levels {
                    self.state.input_levels.insert(id, level as f32);
                }
                self.send_snapshot();
            }
            // Events we don't need to handle
            MixerEvent::AudioStatusChanged
            | MixerEvent::ComponentChanged
            | MixerEvent::InputDspChanged { .. }
            | MixerEvent::OutputDspChanged { .. }
            | MixerEvent::ProfileChanged { .. }
            | MixerEvent::CaptureDevicesChanged
            | MixerEvent::PlaybackDevicesChanged => {}
        }
    }

    /// Handle a hardware event from the device thread.
    /// This is the event dispatch logic from the original daemon, preserved exactly.
    async fn handle_device_event(
        &mut self,
        event: DeviceEvent,
        proxy: &MixCtlProxy<'static>,
    ) {
        match event {
            DeviceEvent::Connected => {
                info!("device connected");
                if let Err(e) = self.state.refresh_from_dbus(proxy).await {
                    warn!("D-Bus refresh failed: {e}");
                }
                if let Err(e) = self.state.refresh_streams(proxy).await {
                    warn!("stream refresh failed: {e}");
                }
                if let Err(e) = self.state.refresh_custom_inputs(proxy).await {
                    warn!("custom input refresh failed: {e}");
                }
                self.send_snapshot();
            }
            DeviceEvent::Disconnected => {
                info!("device disconnected");
            }
            DeviceEvent::AdjustRouteVolume {
                input_id,
                output_id,
                delta,
            } => {
                if self.state.is_custom_input(input_id) {
                    let old = self
                        .state
                        .custom_inputs
                        .iter()
                        .find(|ci| ci.id == input_id)
                        .map(|ci| ci.value)
                        .unwrap_or(50);
                    let sensitivity = self.state.dial_sensitivity as i16;
                    let new_val =
                        (old as i16 + delta as i16 * sensitivity).clamp(0, 100) as u8;
                    proxy.set_custom_input_value(input_id, new_val).await.ok();
                    self.state.set_custom_input_value(input_id, new_val);
                    self.send_snapshot();
                } else {
                    let old_vol = self.state.route_volume(input_id, output_id);
                    let sensitivity = self.state.dial_sensitivity as i16;
                    let new_vol =
                        (old_vol as i16 + delta as i16 * sensitivity).clamp(0, 100) as u8;
                    if let Err(e) =
                        proxy.set_route_volume(input_id, output_id, new_vol).await
                    {
                        warn!("set_route_volume failed: {e}");
                    }
                    self.state.set_route_volume(input_id, output_id, new_vol);
                    self.send_snapshot();
                }
            }
            DeviceEvent::ToggleRouteMute {
                input_id,
                output_id,
            } => {
                if self.state.is_custom_input(input_id) {
                    return;
                }
                let muted = self.state.route_muted(input_id, output_id);
                if let Err(e) =
                    proxy.set_route_mute(input_id, output_id, !muted).await
                {
                    warn!("set_route_mute failed: {e}");
                }
                self.state.set_route_muted(input_id, output_id, !muted);
                self.send_snapshot();
            }
            DeviceEvent::ToggleGlobalMute { input_id } => {
                if self.state.is_custom_input(input_id) {
                    return;
                }
                let all_muted = self.state.is_globally_muted(input_id);
                let new_muted = !all_muted;
                for &output_id in &self.state.output_ids() {
                    if let Err(e) =
                        proxy.set_route_mute(input_id, output_id, new_muted).await
                    {
                        warn!("set_route_mute failed: {e}");
                    }
                    self.state
                        .set_route_muted(input_id, output_id, new_muted);
                }
                self.send_snapshot();
            }
            DeviceEvent::NextOutput => {
                self.state.next_output();
                self.send_snapshot();
            }
            DeviceEvent::PrevOutput => {
                self.state.prev_output();
                self.send_snapshot();
            }
            DeviceEvent::PageLeft => {
                if self.state.current_page > 0 {
                    self.state.current_page -= 1;
                    self.send_snapshot();
                }
            }
            DeviceEvent::PageRight => {
                let max = self.state.max_page();
                if self.state.current_page < max {
                    self.state.current_page += 1;
                    self.send_snapshot();
                }
            }
            DeviceEvent::ToggleOutputMute { output_id } => {
                match proxy.get_output(output_id).await {
                    Ok(info) => {
                        if let Err(e) =
                            proxy.set_output_mute(output_id, !info.muted).await
                        {
                            warn!("set_output_mute failed: {e}");
                        }
                    }
                    Err(e) => warn!("get_output failed: {e}"),
                }
            }
            DeviceEvent::ToggleAllOutputsMute => {
                let output_ids = self.state.output_ids();
                let mut any_unmuted = false;
                for &id in &output_ids {
                    if let Ok(info) = proxy.get_output(id).await {
                        if !info.muted {
                            any_unmuted = true;
                            break;
                        }
                    }
                }
                let new_muted = any_unmuted;
                for &id in &output_ids {
                    proxy.set_output_mute(id, new_muted).await.ok();
                }
            }
            DeviceEvent::ToggleEq { input_id } => {
                if let Ok(enabled) = proxy.get_input_eq_enabled(input_id).await {
                    proxy
                        .set_input_eq_enabled(input_id, !enabled)
                        .await
                        .ok();
                }
            }
            DeviceEvent::ToggleGate { input_id } => {
                if let Ok(info) = proxy.get_input_gate(input_id).await {
                    proxy
                        .set_input_gate_enabled(input_id, !info.enabled)
                        .await
                        .ok();
                }
            }
            DeviceEvent::ToggleDeesser { input_id } => {
                if let Ok(info) = proxy.get_input_deesser(input_id).await {
                    proxy
                        .set_input_deesser_enabled(input_id, !info.enabled)
                        .await
                        .ok();
                }
            }
            DeviceEvent::ToggleCompressor { output_id } => {
                if let Ok(info) = proxy.get_output_compressor(output_id).await {
                    proxy
                        .set_output_compressor_enabled(output_id, !info.enabled)
                        .await
                        .ok();
                }
            }
            DeviceEvent::ToggleLimiter { output_id } => {
                if let Ok(info) = proxy.get_output_limiter(output_id).await {
                    proxy
                        .set_output_limiter_enabled(output_id, !info.enabled)
                        .await
                        .ok();
                }
            }
            DeviceEvent::LoadProfile { name } => {
                if let Err(e) = proxy.load_profile(&name).await {
                    warn!("load_profile failed: {e}");
                }
            }
            DeviceEvent::SetGlobalMute { input_id, muted } => {
                if self.state.is_custom_input(input_id) {
                    return;
                }
                for &output_id in &self.state.output_ids() {
                    proxy
                        .set_route_mute(input_id, output_id, muted)
                        .await
                        .ok();
                    self.state.set_route_muted(input_id, output_id, muted);
                }
                self.send_snapshot();
            }
        }
    }
}

impl DeviceAdapter for BeacnAdapter {
    fn capabilities(&self) -> Vec<Capability> {
        vec![
            Capability::Fader {
                count: 4,
                range: (0.0, 1.0),
            },
            Capability::Button {
                count: 11,
                kind: ButtonKind::Momentary,
            },
            Capability::Screen {
                width: 800,
                height: 480,
                format: ScreenFormat::Jpeg,
            },
            Capability::Led {
                count: 12,
                color_mode: ColorMode::Rgb,
            },
            Capability::Meter { count: 4 },
        ]
    }

    fn device_name(&self) -> &str {
        "beacn-mix-create"
    }

    async fn run(
        &mut self,
        proxy: MixCtlProxy<'static>,
        mut mixer_events: mpsc::UnboundedReceiver<MixerEvent>,
    ) -> anyhow::Result<()> {
        // Fetch and apply config
        self.apply_config(&proxy).await;

        // Initial state refresh
        if let Err(e) = self.state.refresh_from_dbus(&proxy).await {
            warn!("initial D-Bus refresh failed: {e}");
        }
        if let Err(e) = self.state.refresh_streams(&proxy).await {
            warn!("initial stream fetch failed: {e}");
        }
        if let Err(e) = self.state.refresh_custom_inputs(&proxy).await {
            warn!("initial custom inputs fetch failed: {e}");
        }
        match proxy.get_broadcast_levels().await {
            Ok(enabled) => self.state.levels_enabled = enabled,
            Err(e) => warn!("get_broadcast_levels failed: {e}"),
        }
        self.send_snapshot();

        // Main event loop: select over mixer events + device events + shutdown
        loop {
            tokio::select! {
                event = mixer_events.recv() => {
                    let Some(event) = event else { break };
                    self.handle_mixer_event(event, &proxy).await;
                }
                event = self.dev_event_rx.recv() => {
                    let Some(event) = event else { break };
                    self.handle_device_event(event, &proxy).await;
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    if self.shutdown_flag.load(Ordering::Acquire) {
                        return Ok(());
                    }
                    // Check if daemon is still alive
                    if proxy.ping().await.is_err() {
                        return Err(anyhow::anyhow!("daemon disconnected"));
                    }
                }
            }
        }

        Ok(())
    }

    async fn shutdown(&mut self) {
        info!("beacn adapter shutting down");
        self.dev_cmd_tx.send(DeviceCommand::ShowWaiting).ok();
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(Level::INFO.into()))
        .init();

    info!("mixctl-beacn-daemon starting");

    let shutdown_flag = Arc::new(AtomicBool::new(false));

    // Device channels
    let (dev_cmd_tx, dev_cmd_rx) = mpsc::unbounded_channel::<DeviceCommand>();
    let (dev_event_tx, dev_event_rx) = mpsc::unbounded_channel::<DeviceEvent>();

    // Parse layout from args
    let layout_name = std::env::args().nth(1).unwrap_or_default();
    let layout_kind = if layout_name.is_empty() {
        DeviceLayoutKind::Column
    } else {
        DeviceLayoutKind::from_str_loose(&layout_name)
    };
    let layout = layout_kind.create_layout();
    info!("initial display layout: {layout_kind:?}");

    let device_thread =
        DeviceThread::spawn(shutdown_flag.clone(), dev_event_tx, dev_cmd_rx, layout);

    // Safety guard: ensure device gets Shutdown on any exit path
    let guard_tx = dev_cmd_tx.clone();
    let guard_flag = shutdown_flag.clone();
    let _shutdown_guard = ShutdownGuard {
        tx: Some(guard_tx),
        flag: guard_flag,
    };

    // Create adapter and runner
    let mut adapter = BeacnAdapter::new(dev_cmd_tx.clone(), dev_event_rx, shutdown_flag.clone());

    let runner = AdapterRunner::new();

    // Signal handler
    let sf = runner.shutdown_flag();
    let device_sf = shutdown_flag.clone();
    let shutdown_tx = dev_cmd_tx.clone();
    tokio::spawn(async move {
        let mut sigterm = tokio::signal::unix::signal(
            tokio::signal::unix::SignalKind::terminate(),
        )
        .expect("failed to register SIGTERM handler");

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Ctrl-C received, shutting down");
            }
            _ = sigterm.recv() => {
                info!("SIGTERM received, shutting down");
            }
        }
        sf.store(true, Ordering::Release);
        device_sf.store(true, Ordering::Release);
        shutdown_tx.send(DeviceCommand::Shutdown).ok();
    });

    // Run the adapter via the SDK runner
    runner.run(&mut adapter).await?;

    // Ensure device gets shut down cleanly
    shutdown_flag.store(true, Ordering::Release);
    dev_cmd_tx.send(DeviceCommand::Shutdown).ok();
    info!("waiting for device thread");
    device_thread.join();
    info!("mixctl-beacn-daemon stopped");
    Ok(())
}
