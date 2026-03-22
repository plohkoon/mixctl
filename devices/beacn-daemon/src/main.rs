mod state;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_lite::StreamExt;
use mixctl_core::config_sections::BeacnConfig;
use mixctl_core::dbus::MixCtlProxy;
use mixctl_beacn_device::{DeviceCommand, DeviceEvent, DeviceThread};
use mixctl_beacn_display::DeviceLayoutKind;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{info, warn, Level};
use tracing_subscriber::EnvFilter;

use crate::state::BeacnState;

/// Drop guard that ensures the device thread receives a Shutdown command
/// regardless of how the process exits (panic, early return, signal).
struct ShutdownGuard {
    tx: Option<tokio::sync::mpsc::UnboundedSender<DeviceCommand>>,
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(Level::INFO.into()))
        .init();

    info!("mixctl-beacn-daemon starting");

    let shutdown_flag = Arc::new(AtomicBool::new(false));

    // Device channels — device thread runs independently of D-Bus
    let (dev_cmd_tx, dev_cmd_rx) = tokio::sync::mpsc::unbounded_channel::<DeviceCommand>();
    let (dev_event_tx, dev_event_rx) = tokio::sync::mpsc::unbounded_channel::<DeviceEvent>();

    // Parse layout from args (or use default, will be overridden by config)
    let layout_name = std::env::args().nth(1).unwrap_or_default();
    let layout_kind = if layout_name.is_empty() {
        DeviceLayoutKind::Column
    } else {
        DeviceLayoutKind::from_str_loose(&layout_name)
    };
    let layout = layout_kind.create_layout();
    info!("initial display layout: {layout_kind:?}");

    let device_thread = DeviceThread::spawn(
        shutdown_flag.clone(),
        dev_event_tx,
        dev_cmd_rx,
        layout,
    );

    // Signal handler (Ctrl-C and SIGTERM)
    let sf = shutdown_flag.clone();
    let shutdown_tx = dev_cmd_tx.clone();
    tokio::spawn(async move {
        let mut sigterm = tokio::signal::unix::signal(
            tokio::signal::unix::SignalKind::terminate(),
        ).expect("failed to register SIGTERM handler");

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Ctrl-C received, shutting down");
            }
            _ = sigterm.recv() => {
                info!("SIGTERM received, shutting down");
            }
        }
        sf.store(true, Ordering::Release);
        shutdown_tx.send(DeviceCommand::Shutdown).ok();
    });

    // Safety guard: ensure device gets Shutdown on any exit path
    let guard_tx = dev_cmd_tx.clone();
    let guard_flag = shutdown_flag.clone();
    let _shutdown_guard = ShutdownGuard { tx: Some(guard_tx), flag: guard_flag };

    // D-Bus reconnection loop — retries when daemon is unavailable
    let state = Arc::new(Mutex::new(BeacnState::new_with_config(3, 0.85)));
    let dev_event_rx = Arc::new(Mutex::new(dev_event_rx));

    loop {
        if shutdown_flag.load(Ordering::Acquire) {
            break;
        }

        match run_daemon_session(
            &state,
            &dev_cmd_tx,
            &dev_event_rx,
            &shutdown_flag,
        ).await {
            Ok(()) => break, // clean shutdown
            Err(e) => {
                warn!("daemon session ended: {e}");
                dev_cmd_tx.send(DeviceCommand::ShowWaiting).ok();
                // Wait before retrying
                for _ in 0..20 {
                    if shutdown_flag.load(Ordering::Acquire) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    // Ensure device gets shut down cleanly
    shutdown_flag.store(true, Ordering::Release);
    dev_cmd_tx.send(DeviceCommand::Shutdown).ok();
    info!("waiting for device thread");
    device_thread.join();
    info!("mixctl-beacn-daemon stopped");
    Ok(())
}

/// Run a single daemon session: connect, subscribe, process events.
/// Returns Ok(()) on clean shutdown, Err on daemon disconnect.
async fn run_daemon_session(
    state: &Arc<Mutex<BeacnState>>,
    dev_cmd_tx: &tokio::sync::mpsc::UnboundedSender<DeviceCommand>,
    dev_event_rx: &Arc<Mutex<tokio::sync::mpsc::UnboundedReceiver<DeviceEvent>>>,
    shutdown_flag: &Arc<AtomicBool>,
) -> anyhow::Result<()> {
    // Connect to D-Bus
    let conn = zbus::Connection::session().await?;
    let proxy = MixCtlProxy::new(&conn).await?;

    // Verify daemon is alive
    proxy.ping().await?;
    info!("connected to mixer daemon via D-Bus");

    // Register as component
    proxy.register_component("beacn").await.ok();

    // Fetch config
    let beacn_config = match proxy.get_config_section("beacn").await {
        Ok(json) => serde_json::from_str::<BeacnConfig>(&json).unwrap_or_default(),
        Err(_) => BeacnConfig::default(),
    };

    // Apply config to state
    {
        let mut s = state.lock().await;
        s.dial_sensitivity = beacn_config.dial_sensitivity;
        s.level_decay = beacn_config.level_decay;
    }

    // Apply layout, brightness, and button mappings from config
    let layout_kind = DeviceLayoutKind::from_str_loose(&beacn_config.layout);
    dev_cmd_tx.send(DeviceCommand::ChangeLayout(layout_kind.create_layout())).ok();
    dev_cmd_tx.send(DeviceCommand::SetBrightness {
        display: beacn_config.display_brightness,
        led: beacn_config.led_brightness,
    }).ok();
    dev_cmd_tx.send(DeviceCommand::SetButtonMappings(beacn_config.button_mappings.clone())).ok();
    info!("beacn config: layout={}, dial_sensitivity={}, brightness={}:{}, level_decay={}",
        beacn_config.layout, beacn_config.dial_sensitivity,
        beacn_config.display_brightness, beacn_config.led_brightness,
        beacn_config.level_decay);

    // Build initial snapshot
    {
        let mut s = state.lock().await;
        if let Err(e) = s.refresh_from_dbus(&proxy).await {
            warn!("initial D-Bus refresh failed: {e}");
        }
        if let Err(e) = s.refresh_streams(&proxy).await {
            warn!("initial stream fetch failed: {e}");
        }
        match proxy.get_broadcast_levels().await {
            Ok(enabled) => s.levels_enabled = enabled,
            Err(e) => warn!("get_broadcast_levels failed: {e}"),
        }
        let snapshot = s.build_snapshot();
        dev_cmd_tx.send(DeviceCommand::UpdateState(snapshot)).ok();
    }

    // Subscribe to D-Bus signals — spawn listeners and track handles for cleanup
    let mut signal_tasks: Vec<JoinHandle<()>> = Vec::new();

    // Helper macro to reduce signal subscription boilerplate
    macro_rules! spawn_signal {
        ($stream:expr, $body:expr) => {{
            let mut stream = $stream;
            signal_tasks.push(tokio::spawn(async move {
                while stream.next().await.is_some() {
                    $body;
                }
            }));
        }};
    }

    // inputs/outputs/output_state/route/page → full refresh
    let (s1, t1, p1) = (state.clone(), dev_cmd_tx.clone(), proxy.clone());
    spawn_signal!(proxy.receive_inputs_config_changed().await?, {
        refresh_and_notify(&s1, &t1, &p1).await;
    });
    let (s2, t2, p2) = (state.clone(), dev_cmd_tx.clone(), proxy.clone());
    spawn_signal!(proxy.receive_outputs_config_changed().await?, {
        refresh_and_notify(&s2, &t2, &p2).await;
    });
    let (s3, t3, p3) = (state.clone(), dev_cmd_tx.clone(), proxy.clone());
    spawn_signal!(proxy.receive_output_state_changed().await?, {
        refresh_and_notify(&s3, &t3, &p3).await;
    });
    let (s4, t4, p4) = (state.clone(), dev_cmd_tx.clone(), proxy.clone());
    spawn_signal!(proxy.receive_route_changed().await?, {
        refresh_and_notify(&s4, &t4, &p4).await;
    });
    let (s5, t5, p5) = (state.clone(), dev_cmd_tx.clone(), proxy.clone());
    spawn_signal!(proxy.receive_page_changed().await?, {
        refresh_and_notify(&s5, &t5, &p5).await;
    });

    // streams_changed → refresh streams only
    let s_streams = state.clone();
    let t_streams = dev_cmd_tx.clone();
    let p_streams = proxy.clone();
    let mut streams_changed = proxy.receive_streams_changed().await?;
    signal_tasks.push(tokio::spawn(async move {
        while streams_changed.next().await.is_some() {
            let mut s = s_streams.lock().await;
            if let Err(e) = s.refresh_streams(&p_streams).await {
                warn!("stream refresh failed: {e}");
                continue;
            }
            let snapshot = s.build_snapshot();
            t_streams.send(DeviceCommand::UpdateState(snapshot)).ok();
        }
    }));

    // broadcast_levels_changed
    let s6 = state.clone();
    let t6 = dev_cmd_tx.clone();
    let p6 = proxy.clone();
    let mut broadcast_levels = proxy.receive_broadcast_levels_changed().await?;
    signal_tasks.push(tokio::spawn(async move {
        while let Some(signal) = broadcast_levels.next().await {
            let args = match signal.args() {
                Ok(a) => a,
                Err(_) => continue,
            };
            let enabled = args.enabled;
            let mut s = s6.lock().await;
            s.levels_enabled = enabled;
            if !enabled {
                s.input_levels.clear();
            }
            let snapshot = s.build_snapshot();
            t6.send(DeviceCommand::UpdateState(snapshot)).ok();
            if enabled {
                drop(s);
                refresh_and_notify(&s6, &t6, &p6).await;
            }
        }
    }));

    // config_section_changed
    let s8 = state.clone();
    let t8 = dev_cmd_tx.clone();
    let p8 = proxy.clone();
    let mut config_section = proxy.receive_config_section_changed().await?;
    signal_tasks.push(tokio::spawn(async move {
        while let Some(signal) = config_section.next().await {
            let args = match signal.args() {
                Ok(a) => a,
                Err(_) => continue,
            };
            if args.section != "beacn" {
                continue;
            }
            match p8.get_config_section("beacn").await {
                Ok(json) => {
                    if let Ok(config) = serde_json::from_str::<BeacnConfig>(&json) {
                        let mut s = s8.lock().await;
                        s.dial_sensitivity = config.dial_sensitivity;
                        s.level_decay = config.level_decay;
                        info!("beacn config updated: dial_sensitivity={}, level_decay={}",
                            config.dial_sensitivity, config.level_decay);
                        let snapshot = s.build_snapshot();
                        t8.send(DeviceCommand::UpdateState(snapshot)).ok();
                        let new_kind = DeviceLayoutKind::from_str_loose(&config.layout);
                        let new_layout = new_kind.create_layout();
                        info!("switching display layout to {new_kind:?}");
                        t8.send(DeviceCommand::ChangeLayout(new_layout)).ok();
                        t8.send(DeviceCommand::SetBrightness {
                            display: config.display_brightness,
                            led: config.led_brightness,
                        }).ok();
                        t8.send(DeviceCommand::SetButtonMappings(config.button_mappings.clone())).ok();
                    }
                }
                Err(e) => warn!("failed to re-fetch beacn config: {e}"),
            }
        }
    }));

    // input_levels_changed — ~20Hz updates
    let s7 = state.clone();
    let t7 = dev_cmd_tx.clone();
    let mut input_levels = proxy.receive_input_levels_changed().await?;
    signal_tasks.push(tokio::spawn(async move {
        while let Some(signal) = input_levels.next().await {
            let args = match signal.args() {
                Ok(a) => a,
                Err(_) => continue,
            };
            let mut s = s7.lock().await;
            if !s.levels_enabled {
                continue;
            }
            s.decay_levels();
            for &(id, level) in &args.levels {
                s.input_levels.insert(id, level as f32);
            }
            let snapshot = s.build_snapshot();
            t7.send(DeviceCommand::UpdateState(snapshot)).ok();
        }
    }));

    // Device event loop — handle hardware input
    let mut dev_rx = dev_event_rx.lock().await;
    loop {
        let event = tokio::select! {
            e = dev_rx.recv() => match e {
                Some(e) => e,
                None => break,
            },
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                if shutdown_flag.load(Ordering::Acquire) {
                    // Abort signal listeners and return clean shutdown
                    for task in &signal_tasks { task.abort(); }
                    return Ok(());
                }
                // Check if daemon is still alive by pinging
                if proxy.ping().await.is_err() {
                    for task in &signal_tasks { task.abort(); }
                    return Err(anyhow::anyhow!("daemon disconnected"));
                }
                continue;
            }
        };

        match event {
            DeviceEvent::Connected => {
                info!("device connected");
                let mut s = state.lock().await;
                if let Err(e) = s.refresh_from_dbus(&proxy).await {
                    warn!("D-Bus refresh failed: {e}");
                }
                if let Err(e) = s.refresh_streams(&proxy).await {
                    warn!("stream refresh failed: {e}");
                }
                let snapshot = s.build_snapshot();
                dev_cmd_tx.send(DeviceCommand::UpdateState(snapshot)).ok();
            }
            DeviceEvent::Disconnected => {
                info!("device disconnected");
            }
            DeviceEvent::AdjustRouteVolume { input_id, output_id, delta } => {
                let mut s = state.lock().await;
                let old_vol = s.route_volume(input_id, output_id);
                let sensitivity = s.dial_sensitivity as i16;
                let new_vol = (old_vol as i16 + delta as i16 * sensitivity).clamp(0, 100) as u8;
                if let Err(e) = proxy.set_route_volume(input_id, output_id, new_vol).await {
                    warn!("set_route_volume failed: {e}");
                }
                s.set_route_volume(input_id, output_id, new_vol);
                let snapshot = s.build_snapshot();
                dev_cmd_tx.send(DeviceCommand::UpdateState(snapshot)).ok();
            }
            DeviceEvent::ToggleRouteMute { input_id, output_id } => {
                let mut s = state.lock().await;
                let muted = s.route_muted(input_id, output_id);
                if let Err(e) = proxy.set_route_mute(input_id, output_id, !muted).await {
                    warn!("set_route_mute failed: {e}");
                }
                s.set_route_muted(input_id, output_id, !muted);
                let snapshot = s.build_snapshot();
                dev_cmd_tx.send(DeviceCommand::UpdateState(snapshot)).ok();
            }
            DeviceEvent::ToggleGlobalMute { input_id } => {
                let mut s = state.lock().await;
                let all_muted = s.is_globally_muted(input_id);
                let new_muted = !all_muted;
                for &output_id in &s.output_ids() {
                    if let Err(e) = proxy.set_route_mute(input_id, output_id, new_muted).await {
                        warn!("set_route_mute failed: {e}");
                    }
                    s.set_route_muted(input_id, output_id, new_muted);
                }
                let snapshot = s.build_snapshot();
                dev_cmd_tx.send(DeviceCommand::UpdateState(snapshot)).ok();
            }
            DeviceEvent::NextOutput => {
                let mut s = state.lock().await;
                s.next_output();
                let snapshot = s.build_snapshot();
                dev_cmd_tx.send(DeviceCommand::UpdateState(snapshot)).ok();
            }
            DeviceEvent::PrevOutput => {
                let mut s = state.lock().await;
                s.prev_output();
                let snapshot = s.build_snapshot();
                dev_cmd_tx.send(DeviceCommand::UpdateState(snapshot)).ok();
            }
            DeviceEvent::PageLeft => {
                let mut s = state.lock().await;
                if s.current_page > 0 {
                    s.current_page -= 1;
                    if let Err(e) = proxy.set_current_page(s.current_page).await {
                        warn!("set_current_page failed: {e}");
                    }
                    let snapshot = s.build_snapshot();
                    dev_cmd_tx.send(DeviceCommand::UpdateState(snapshot)).ok();
                }
            }
            DeviceEvent::PageRight => {
                let mut s = state.lock().await;
                let max = s.max_page();
                if s.current_page < max {
                    s.current_page += 1;
                    if let Err(e) = proxy.set_current_page(s.current_page).await {
                        warn!("set_current_page failed: {e}");
                    }
                    let snapshot = s.build_snapshot();
                    dev_cmd_tx.send(DeviceCommand::UpdateState(snapshot)).ok();
                }
            }
        }
    }

    // Signal tasks will be cleaned up when their proxies are dropped
    for task in &signal_tasks {
        task.abort();
    }
    Ok(())
}

async fn refresh_and_notify(
    state: &Arc<Mutex<BeacnState>>,
    tx: &tokio::sync::mpsc::UnboundedSender<DeviceCommand>,
    proxy: &MixCtlProxy<'_>,
) {
    let mut s = state.lock().await;
    if let Err(e) = s.refresh_from_dbus(proxy).await {
        warn!("D-Bus refresh failed: {e}");
        return;
    }
    let snapshot = s.build_snapshot();
    tx.send(DeviceCommand::UpdateState(snapshot)).ok();
}
