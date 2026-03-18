mod state;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures_lite::StreamExt;
use mixctl_core::config_sections::BeacnConfig;
use mixctl_core::dbus::MixCtlProxy;
use mixctl_beacn_device::{DeviceCommand, DeviceEvent, DeviceThread};
use mixctl_beacn_display::DeviceLayoutKind;
use tokio::sync::Mutex;
use tracing::{info, warn, Level};
use tracing_subscriber::EnvFilter;

use crate::state::BeacnState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(Level::INFO.into()))
        .init();

    info!("mixctl-beacn-daemon starting");

    // Connect to the mixer daemon via D-Bus
    let conn = zbus::Connection::session().await?;
    let proxy = MixCtlProxy::new(&conn).await?;
    info!("connected to mixer daemon via D-Bus");

    // Fetch beacn config section from daemon
    let beacn_config = match proxy.get_config_section("beacn").await {
        Ok(json) => serde_json::from_str::<BeacnConfig>(&json).unwrap_or_default(),
        Err(e) => {
            warn!("failed to fetch beacn config: {e}, using defaults");
            BeacnConfig::default()
        }
    };
    info!("beacn config: layout={}, dial_sensitivity={}, level_decay={}",
        beacn_config.layout, beacn_config.dial_sensitivity, beacn_config.level_decay);

    // Parse layout from args, falling back to config
    let layout_name = std::env::args().nth(1).unwrap_or_default();
    let layout_kind = if layout_name.is_empty() {
        DeviceLayoutKind::from_str_loose(&beacn_config.layout)
    } else {
        DeviceLayoutKind::from_str_loose(&layout_name)
    };
    let layout = layout_kind.create_layout();
    info!("display layout: {layout_kind:?}");

    let shutdown_flag = Arc::new(AtomicBool::new(false));

    // Device channels
    let (dev_cmd_tx, dev_cmd_rx) = tokio::sync::mpsc::unbounded_channel::<DeviceCommand>();
    let (dev_event_tx, mut dev_event_rx) = tokio::sync::mpsc::unbounded_channel::<DeviceEvent>();

    let device_thread = DeviceThread::spawn(
        shutdown_flag.clone(),
        dev_event_tx,
        dev_cmd_rx,
        layout,
    );

    // Shared state
    let state = Arc::new(Mutex::new(BeacnState::new_with_config(
        beacn_config.dial_sensitivity,
        beacn_config.level_decay,
    )));

    // Build initial snapshot from D-Bus
    {
        let mut s = state.lock().await;
        if let Err(e) = s.refresh_from_dbus(&proxy).await {
            warn!("initial D-Bus refresh failed (mixer daemon may not be running yet): {e}");
        }
        // Check if level broadcasting is enabled
        match proxy.get_broadcast_levels().await {
            Ok(enabled) => s.levels_enabled = enabled,
            Err(e) => warn!("get_broadcast_levels failed: {e}"),
        }
        let snapshot = s.build_snapshot();
        dev_cmd_tx.send(DeviceCommand::UpdateState(snapshot)).ok();
    }

    // Subscribe to D-Bus signals — any state change triggers a refresh
    let signal_state = state.clone();
    let signal_tx = dev_cmd_tx.clone();
    let signal_proxy = proxy.clone();

    // inputs_config_changed
    let s1 = signal_state.clone();
    let t1 = signal_tx.clone();
    let p1 = signal_proxy.clone();
    let mut inputs_changed = proxy.receive_inputs_config_changed().await?;
    tokio::spawn(async move {
        while inputs_changed.next().await.is_some() {
            refresh_and_notify(&s1, &t1, &p1).await;
        }
    });

    // outputs_config_changed
    let s2 = signal_state.clone();
    let t2 = signal_tx.clone();
    let p2 = signal_proxy.clone();
    let mut outputs_changed = proxy.receive_outputs_config_changed().await?;
    tokio::spawn(async move {
        while outputs_changed.next().await.is_some() {
            refresh_and_notify(&s2, &t2, &p2).await;
        }
    });

    // output_state_changed
    let s3 = signal_state.clone();
    let t3 = signal_tx.clone();
    let p3 = signal_proxy.clone();
    let mut output_state = proxy.receive_output_state_changed().await?;
    tokio::spawn(async move {
        while output_state.next().await.is_some() {
            refresh_and_notify(&s3, &t3, &p3).await;
        }
    });

    // route_changed
    let s4 = signal_state.clone();
    let t4 = signal_tx.clone();
    let p4 = signal_proxy.clone();
    let mut route = proxy.receive_route_changed().await?;
    tokio::spawn(async move {
        while route.next().await.is_some() {
            refresh_and_notify(&s4, &t4, &p4).await;
        }
    });

    // page_changed
    let s5 = signal_state.clone();
    let t5 = signal_tx.clone();
    let p5 = signal_proxy.clone();
    let mut page = proxy.receive_page_changed().await?;
    tokio::spawn(async move {
        while page.next().await.is_some() {
            refresh_and_notify(&s5, &t5, &p5).await;
        }
    });

    // broadcast_levels_changed — toggle level monitoring
    let s6 = signal_state.clone();
    let t6 = signal_tx.clone();
    let p6 = signal_proxy.clone();
    let mut broadcast_levels = proxy.receive_broadcast_levels_changed().await?;
    tokio::spawn(async move {
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
            // If re-enabled, refresh levels
            if enabled {
                drop(s);
                refresh_and_notify(&s6, &t6, &p6).await;
            }
        }
    });

    // config_section_changed — re-fetch beacn config
    let s8 = signal_state.clone();
    let t8 = signal_tx.clone();
    let p8 = signal_proxy.clone();
    let mut config_section = proxy.receive_config_section_changed().await?;
    tokio::spawn(async move {
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
                        if s.dial_sensitivity != config.dial_sensitivity || s.level_decay != config.level_decay {
                            let snapshot = s.build_snapshot();
                            t8.send(DeviceCommand::UpdateState(snapshot)).ok();
                        }
                        if config.layout != "column" && config.layout != "grid" && config.layout != "dial" {
                            warn!("layout change to '{}' requires restart", config.layout);
                        } else {
                            warn!("layout changes require restarting beacn-daemon");
                        }
                    }
                }
                Err(e) => warn!("failed to re-fetch beacn config: {e}"),
            }
        }
    });

    // input_levels_changed — update levels at ~20Hz
    let s7 = signal_state.clone();
    let t7 = signal_tx.clone();
    let mut input_levels = proxy.receive_input_levels_changed().await?;
    tokio::spawn(async move {
        while let Some(signal) = input_levels.next().await {
            let args = match signal.args() {
                Ok(a) => a,
                Err(_) => continue,
            };
            let mut s = s7.lock().await;
            if !s.levels_enabled {
                continue;
            }
            // Apply decay to existing levels first, then overlay new data
            s.decay_levels();
            for &(id, level) in &args.levels {
                s.input_levels.insert(id, level as f32);
            }
            let snapshot = s.build_snapshot();
            t7.send(DeviceCommand::UpdateState(snapshot)).ok();
        }
    });

    // Ctrl-C handler
    let sf = shutdown_flag.clone();
    let shutdown_tx = dev_cmd_tx.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("Ctrl-C received, shutting down");
        sf.store(true, Ordering::Release);
        shutdown_tx.send(DeviceCommand::Shutdown).ok();
    });

    // Device event loop — handle hardware input, call D-Bus methods
    loop {
        let event = tokio::select! {
            e = dev_event_rx.recv() => match e {
                Some(e) => e,
                None => break,
            },
            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                if shutdown_flag.load(Ordering::Acquire) {
                    break;
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
                // Optimistic local update for responsiveness
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

    info!("waiting for device thread");
    device_thread.join();
    info!("mixctl-beacn-daemon stopped");
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
