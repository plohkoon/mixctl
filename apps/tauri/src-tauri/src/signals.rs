use crate::state::AppState;
use futures_lite::StreamExt;
use log::{info, warn};
use mixctl_core::dbus::MixCtlProxy;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

/// Spawn D-Bus signal listeners that emit Tauri events to the frontend.
/// Returns a join handle for the reconnection supervisor task.
pub fn spawn_signal_listeners(app_handle: AppHandle) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            info!("Connecting to mixctl daemon...");
            match run_signal_loop(&app_handle).await {
                Ok(()) => {
                    info!("Signal loop exited cleanly");
                    break;
                }
                Err(e) => {
                    warn!("Signal loop error: {e}, reconnecting in 2s...");
                    app_handle.emit("mixer:disconnected", ()).ok();
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }
    })
}

async fn run_signal_loop(app: &AppHandle) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let state = app.state::<Mutex<AppState>>();
    let proxy = {
        let s = state.lock().await;
        s.proxy().map_err(|e| e.to_string())?.clone()
    };

    // Sentinel channel: if any signal stream ends, we know the daemon disconnected
    let (disc_tx, mut disc_rx) = tokio::sync::mpsc::unbounded_channel::<()>();

    // Spawn individual signal listeners
    let handles = vec![
        spawn_route_changed(app.clone(), proxy.clone(), disc_tx.clone()),
        spawn_output_state_changed(app.clone(), proxy.clone(), disc_tx.clone()),
        spawn_inputs_config_changed(app.clone(), proxy.clone(), disc_tx.clone()),
        spawn_outputs_config_changed(app.clone(), proxy.clone(), disc_tx.clone()),
        spawn_streams_changed(app.clone(), proxy.clone(), disc_tx.clone()),
        spawn_playback_devices_changed(app.clone(), proxy.clone(), disc_tx.clone()),
        spawn_audio_status_changed(app.clone(), proxy.clone(), disc_tx.clone()),
        spawn_component_changed(app.clone(), proxy.clone(), disc_tx.clone()),
        spawn_profile_changed(app.clone(), proxy.clone(), disc_tx.clone()),
        spawn_rules_changed(app.clone(), proxy.clone(), disc_tx.clone()),
        spawn_capture_devices_changed(app.clone(), proxy.clone(), disc_tx.clone()),
        spawn_custom_input_changed(app.clone(), proxy.clone(), disc_tx.clone()),
    ];

    app.emit("mixer:connected", ()).ok();

    // Wait for disconnection
    disc_rx.recv().await;

    // Abort all listener tasks
    for h in handles {
        h.abort();
    }

    Err("signal stream ended".into())
}

fn spawn_route_changed(
    app: AppHandle,
    proxy: MixCtlProxy<'static>,
    disc: tokio::sync::mpsc::UnboundedSender<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Ok(mut stream) = proxy.receive_route_changed().await else {
            disc.send(()).ok();
            return;
        };
        while let Some(signal) = stream.next().await {
            if let Ok(args) = signal.args() {
                let state = app.state::<Mutex<AppState>>();
                let s = state.lock().await;
                if let Some(proxy) = &s.proxy {
                    if let Ok(route) = proxy.get_route(args.input_id, args.output_id).await {
                        app.emit("mixer:route-changed", &route).ok();
                    }
                }
            }
        }
        disc.send(()).ok();
    })
}

fn spawn_output_state_changed(
    app: AppHandle,
    proxy: MixCtlProxy<'static>,
    disc: tokio::sync::mpsc::UnboundedSender<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Ok(mut stream) = proxy.receive_output_state_changed().await else {
            disc.send(()).ok();
            return;
        };
        while stream.next().await.is_some() {
            emit_full_refresh(&app).await;
        }
        disc.send(()).ok();
    })
}

fn spawn_inputs_config_changed(
    app: AppHandle,
    proxy: MixCtlProxy<'static>,
    disc: tokio::sync::mpsc::UnboundedSender<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Ok(mut stream) = proxy.receive_inputs_config_changed().await else {
            disc.send(()).ok();
            return;
        };
        while stream.next().await.is_some() {
            emit_full_refresh(&app).await;
        }
        disc.send(()).ok();
    })
}

fn spawn_outputs_config_changed(
    app: AppHandle,
    proxy: MixCtlProxy<'static>,
    disc: tokio::sync::mpsc::UnboundedSender<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Ok(mut stream) = proxy.receive_outputs_config_changed().await else {
            disc.send(()).ok();
            return;
        };
        while stream.next().await.is_some() {
            emit_full_refresh(&app).await;
        }
        disc.send(()).ok();
    })
}

fn spawn_streams_changed(
    app: AppHandle,
    proxy: MixCtlProxy<'static>,
    disc: tokio::sync::mpsc::UnboundedSender<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Ok(mut stream) = proxy.receive_streams_changed().await else {
            disc.send(()).ok();
            return;
        };
        while stream.next().await.is_some() {
            let state = app.state::<Mutex<AppState>>();
            let s = state.lock().await;
            if let Ok(streams) = s.proxy.as_ref().unwrap().list_streams().await {
                app.emit("mixer:streams-changed", &streams).ok();
            }
        }
        disc.send(()).ok();
    })
}

fn spawn_playback_devices_changed(
    app: AppHandle,
    proxy: MixCtlProxy<'static>,
    disc: tokio::sync::mpsc::UnboundedSender<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Ok(mut stream) = proxy.receive_playback_devices_changed().await else {
            disc.send(()).ok();
            return;
        };
        while stream.next().await.is_some() {
            emit_full_refresh(&app).await;
        }
        disc.send(()).ok();
    })
}

fn spawn_audio_status_changed(
    app: AppHandle,
    proxy: MixCtlProxy<'static>,
    disc: tokio::sync::mpsc::UnboundedSender<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Ok(mut stream) = proxy.receive_audio_status_changed().await else {
            disc.send(()).ok();
            return;
        };
        while stream.next().await.is_some() {
            let state = app.state::<Mutex<AppState>>();
            let s = state.lock().await;
            if let Ok(status) = s.proxy.as_ref().unwrap().get_audio_status().await {
                app.emit("mixer:status-changed", status == "connected").ok();
            }
        }
        disc.send(()).ok();
    })
}

fn spawn_component_changed(
    app: AppHandle,
    proxy: MixCtlProxy<'static>,
    disc: tokio::sync::mpsc::UnboundedSender<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Ok(mut stream) = proxy.receive_component_changed().await else {
            disc.send(()).ok();
            return;
        };
        while stream.next().await.is_some() {
            let state = app.state::<Mutex<AppState>>();
            let s = state.lock().await;
            if let Ok(components) = s.proxy.as_ref().unwrap().list_components().await {
                let beacn = components.iter().any(|c| c.component_type == "beacn");
                app.emit("mixer:beacn-changed", beacn).ok();
            }
        }
        disc.send(()).ok();
    })
}

fn spawn_profile_changed(
    app: AppHandle,
    proxy: MixCtlProxy<'static>,
    disc: tokio::sync::mpsc::UnboundedSender<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Ok(mut stream) = proxy.receive_profile_changed().await else {
            disc.send(()).ok();
            return;
        };
        while stream.next().await.is_some() {
            // Profile loaded — do a full refresh to update all routing/DSP state
            emit_full_refresh(&app).await;
        }
        disc.send(()).ok();
    })
}

fn spawn_rules_changed(
    app: AppHandle,
    proxy: MixCtlProxy<'static>,
    disc: tokio::sync::mpsc::UnboundedSender<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Ok(mut stream) = proxy.receive_app_rules_changed().await else {
            disc.send(()).ok();
            return;
        };
        while stream.next().await.is_some() {
            app.emit("mixer:rules-changed", ()).ok();
        }
        disc.send(()).ok();
    })
}

fn spawn_capture_devices_changed(
    app: AppHandle,
    proxy: MixCtlProxy<'static>,
    disc: tokio::sync::mpsc::UnboundedSender<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Ok(mut stream) = proxy.receive_capture_devices_changed().await else {
            disc.send(()).ok();
            return;
        };
        while stream.next().await.is_some() {
            app.emit("mixer:capture-devices-changed", ()).ok();
        }
        disc.send(()).ok();
    })
}

fn spawn_custom_input_changed(
    app: AppHandle,
    proxy: MixCtlProxy<'static>,
    disc: tokio::sync::mpsc::UnboundedSender<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Ok(mut stream) = proxy.receive_custom_input_changed().await else {
            disc.send(()).ok();
            return;
        };
        while stream.next().await.is_some() {
            app.emit("mixer:custom-inputs-changed", ()).ok();
        }
        disc.send(()).ok();
    })
}

async fn emit_full_refresh(app: &AppHandle) {
    use crate::commands::mixer::FullState;

    let state = app.state::<Mutex<AppState>>();
    let s = state.lock().await;
    let Some(proxy) = &s.proxy else { return };

    let inputs = proxy.list_inputs().await.unwrap_or_default();
    let outputs = proxy.list_outputs().await.unwrap_or_default();
    let streams = proxy.list_streams().await.unwrap_or_default();
    let playback_devices = proxy.list_playback_devices().await.unwrap_or_default();
    let default_input_id = proxy.get_default_input().await.unwrap_or(0);
    let default_output_id = proxy.get_default_output().await.unwrap_or(0);
    let audio_status = proxy.get_audio_status().await.unwrap_or_default();
    let components = proxy.list_components().await.unwrap_or_default();

    let mut selected_id = s.selected_output_id.load(Ordering::Relaxed);
    if !outputs.iter().any(|o| o.id == selected_id) {
        selected_id = outputs.first().map(|o| o.id).unwrap_or(0);
        s.selected_output_id.store(selected_id, Ordering::Relaxed);
    }

    // Fetch routes for ALL outputs (matrix view needs complete routing state)
    let mut routes = Vec::new();
    for output in &outputs {
        if let Ok(output_routes) = proxy.list_routes_for_output(output.id).await {
            routes.extend(output_routes);
        }
    }

    let full = FullState {
        inputs,
        outputs,
        routes,
        streams,
        playback_devices,
        selected_output_id: selected_id,
        default_input_id,
        default_output_id,
        audio_connected: audio_status == "connected",
        beacn_connected: components.iter().any(|c| c.component_type == "beacn"),
    };

    app.emit("mixer:full-refresh", &full).ok();
}
