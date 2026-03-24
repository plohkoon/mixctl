mod audio;
mod config;
mod dbus_adapter;
mod service;
mod shutdown;
mod state;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use mixctl_core::dbus::{BUS_NAME, OBJ_PATH};
use tracing::{info, warn, Level};
use tracing_subscriber::EnvFilter;
use zbus::connection::Builder as ConnectionBuilder;

use crate::audio::{PwCommand, PwEngine, PwEvent};
use crate::audio::engine::{
    PwCaptureInputConfig, PwEngineConfig, PwInputConfig, PwOutputConfig, PwOutputTargetConfig,
    PwRouteConfig,
};
use crate::audio::volume::combine_pw_volume;
use crate::config::ConfigFile;
use crate::service::{Service, ServiceSignal};
use crate::state::StateFile;


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(Level::INFO.into()))
        .init();

    // Load config (creates with defaults if missing)
    let config = ConfigFile::load_or_create()?;
    info!("loaded config with {} inputs and {} outputs", config.inputs.len(), config.outputs.len());

    // Load state and reconcile with config
    let mut state = StateFile::load()?;
    let reconciled = state.reconcile(&config);

    // Prepare PipeWire engine config from current state
    let inputs: Vec<PwInputConfig> = config
        .inputs
        .iter()
        .map(|c| PwInputConfig {
            input_id: c.id(),
            description: c.name.clone(),
        })
        .collect();

    let outputs: Vec<PwOutputConfig> = config
        .outputs
        .iter()
        .map(|c| PwOutputConfig {
            output_id: c.id(),
            description: c.name.clone(),
        })
        .collect();

    let mut routes = Vec::new();
    for inp in &config.inputs {
        for out in &config.outputs {
            let rs = state
                .route_state(inp.id(), out.id())
                .cloned()
                .unwrap_or_default();
            let os = state
                .output_state(out.id())
                .cloned()
                .unwrap_or_default();
            routes.push(PwRouteConfig {
                input_id: inp.id(),
                output_id: out.id(),
                pw_volume: combine_pw_volume(rs.volume, rs.muted, os.volume, os.muted),
            });
        }
    }

    let output_targets: Vec<PwOutputTargetConfig> = config
        .outputs
        .iter()
        .filter_map(|c| {
            c.target_device
                .as_ref()
                .map(|d| PwOutputTargetConfig {
                    output_id: c.id(),
                    device_name: d.clone(),
                })
        })
        .collect();

    let capture_inputs: Vec<PwCaptureInputConfig> = config
        .inputs
        .iter()
        .filter_map(|c| {
            c.capture_device
                .as_ref()
                .map(|d| PwCaptureInputConfig {
                    input_id: c.id(),
                    capture_device_name: d.clone(),
                })
        })
        .collect();

    let default_input_id = config.default_input;
    let default_output_id = config.default_output;

    let broadcast_levels = config.broadcast_levels.unwrap_or(false);
    let pw_config = PwEngineConfig {
        inputs,
        outputs,
        routes,
        output_targets,
        capture_inputs,
        default_input_id,
        default_output_id,
        broadcast_levels,
    };

    // Create command channel: tokio → relay → pipewire channel → PW thread
    let (pw_cmd_tx, mut pw_cmd_rx) = tokio::sync::mpsc::unbounded_channel::<PwCommand>();

    // Shared PW channel sender, swapped on reconnect via ChannelReady events
    let pw_chan_tx: Arc<tokio::sync::Mutex<Option<pipewire::channel::Sender<PwCommand>>>> =
        Arc::new(tokio::sync::Mutex::new(None));

    // Create event channel: PW thread → tokio
    let (pw_event_tx, mut pw_event_rx) = tokio::sync::mpsc::unbounded_channel::<PwEvent>();

    // Shutdown flag shared between tokio and PW thread
    let shutdown_flag = Arc::new(AtomicBool::new(false));

    // Spawn the PipeWire engine on a dedicated OS thread (handles reconnection internally)
    let pw_engine = PwEngine::spawn(pw_config, shutdown_flag.clone(), pw_event_tx);

    // Restore persisted DSP state by sending commands (buffered until PW channel is ready)
    for inp in &config.inputs {
        let id = inp.id();
        if let Some(eq) = state.input_eq_state(id) {
            if eq.enabled {
                pw_cmd_tx.send(PwCommand::SetInputEqEnabled { input_id: id, enabled: true }).ok();
            }
            for (i, band) in eq.bands.iter().enumerate() {
                if band.gain_db != 0.0 || band.band_type != "peaking" {
                    pw_cmd_tx.send(PwCommand::SetInputEqBand {
                        input_id: id,
                        band: i as u8,
                        band_type: band.band_type.clone(),
                        freq: band.frequency,
                        gain_db: band.gain_db,
                        q: band.q,
                    }).ok();
                }
            }
        }
        if let Some(gate) = state.input_gate_state(id) {
            if gate.enabled {
                pw_cmd_tx.send(PwCommand::SetInputGateEnabled { input_id: id, enabled: true }).ok();
            }
            pw_cmd_tx.send(PwCommand::SetInputGate {
                input_id: id,
                threshold_db: gate.threshold_db,
                attack_ms: gate.attack_ms,
                release_ms: gate.release_ms,
                hold_ms: gate.hold_ms,
            }).ok();
        }
        if let Some(ds) = state.input_deesser_state(id) {
            if ds.enabled {
                pw_cmd_tx.send(PwCommand::SetInputDeesserEnabled { input_id: id, enabled: true }).ok();
            }
            pw_cmd_tx.send(PwCommand::SetInputDeesser {
                input_id: id,
                frequency: ds.frequency,
                threshold_db: ds.threshold_db,
                ratio: ds.ratio,
            }).ok();
        }
    }
    for out in &config.outputs {
        let id = out.id();
        if let Some(comp) = state.output_compressor_state(id) {
            if comp.enabled {
                pw_cmd_tx.send(PwCommand::SetOutputCompressorEnabled { output_id: id, enabled: true }).ok();
            }
            pw_cmd_tx.send(PwCommand::SetOutputCompressor {
                output_id: id,
                threshold_db: comp.threshold_db,
                ratio: comp.ratio,
                attack_ms: comp.attack_ms,
                release_ms: comp.release_ms,
                makeup_gain_db: comp.makeup_gain_db,
                knee_db: comp.knee_db,
            }).ok();
        }
        if let Some(lim) = state.output_limiter_state(id) {
            if lim.enabled {
                pw_cmd_tx.send(PwCommand::SetOutputLimiterEnabled { output_id: id, enabled: true }).ok();
            }
            pw_cmd_tx.send(PwCommand::SetOutputLimiter {
                output_id: id,
                ceiling_db: lim.ceiling_db,
                release_ms: lim.release_ms,
            }).ok();
        }
    }

    // Relay task: forward commands from tokio mpsc to pipewire channel
    let relay_tx = pw_chan_tx.clone();
    let relay_handle = tokio::spawn(async move {
        while let Some(cmd) = pw_cmd_rx.recv().await {
            let guard = relay_tx.lock().await;
            if let Some(tx) = guard.as_ref() {
                if tx.send(cmd).is_err() {
                    warn!("PipeWire channel closed, dropping command");
                }
            }
        }
    });

    let (signal_tx, mut signal_rx) = tokio::sync::mpsc::unbounded_channel::<ServiceSignal>();

    let svc = Service::new(config, state, pw_cmd_tx.clone(), signal_tx);

    // Mark state dirty if reconcile made changes
    if reconciled {
        svc.inner.lock().await.state_dirty = true;
    }

    // Connection must stay alive for D-Bus service registration
    let conn = ConnectionBuilder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJ_PATH, svc.clone())?
        .build()
        .await?;

    info!("daemon running: {} {}", BUS_NAME, OBJ_PATH);

    // Signal emission task: emits D-Bus signals from the connection context
    let signal_conn = conn.clone();
    let signal_handle = tokio::spawn(async move {
        let iface_ref = signal_conn
            .object_server()
            .interface::<_, Service>(OBJ_PATH)
            .await
            .expect("Service interface must be registered");
        while let Some(signal) = signal_rx.recv().await {
            let emitter = iface_ref.signal_emitter();
            signal.emit(&emitter).await;
        }
    });

    // Component cleanup task: watch for D-Bus client disconnects
    let cleanup_svc = svc.clone();
    let cleanup_conn = conn.clone();
    let _cleanup_handle = tokio::spawn(async move {
        use futures_lite::StreamExt;
        let dbus_proxy = match zbus::fdo::DBusProxy::new(&cleanup_conn).await {
            Ok(p) => p,
            Err(e) => {
                warn!("failed to create DBus proxy for component tracking: {e}");
                return;
            }
        };
        let mut stream = match dbus_proxy.receive_name_owner_changed().await {
            Ok(s) => s,
            Err(e) => {
                warn!("failed to subscribe to NameOwnerChanged: {e}");
                return;
            }
        };
        while let Some(signal) = stream.next().await {
            if let Ok(args) = signal.args() {
                // Check if the departed name was a registered component
                let new_owner: &str = args.new_owner.as_deref().unwrap_or("");
                if new_owner.is_empty() {
                    let name = args.name.to_string();
                    let mut shared = cleanup_svc.inner.lock().await;
                    if shared.components.remove(&name).is_some() {
                        info!("component disconnected: {name}");
                        shared.signal_tx.send(ServiceSignal::ComponentChanged).ok();
                    }
                }
            }
        }
    });

    // Event consumer task: process PipeWire events (including ChannelReady for reconnection)
    let event_svc = svc.clone();
    let event_chan_tx = pw_chan_tx.clone();
    let event_handle = tokio::spawn(async move {
        while let Some(event) = pw_event_rx.recv().await {
            match event {
                PwEvent::ChannelReady { sender } => {
                    // Swap in the new PW channel sender on (re)connection
                    let mut guard = event_chan_tx.lock().await;
                    *guard = Some(sender);
                    info!("PipeWire channel ready");
                }
                other => {
                    event_svc.handle_pw_event(other).await;
                }
            }
        }
    });

    // Periodic flush task (30s)
    let flush_svc = svc.clone();
    let flush_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            let mut shared = flush_svc.inner.lock().await;
            if shared.config_dirty {
                if let Err(e) = shared.config.save() {
                    warn!("failed to flush config: {e}");
                } else {
                    shared.config_dirty = false;
                }
            }
            if shared.state_dirty {
                if let Err(e) = shared.state.save() {
                    warn!("failed to flush state: {e}");
                } else {
                    shared.state_dirty = false;
                }
            }
        }
    });

    // ShutdownGuard handles all cleanup (restore originals, persist, teardown)
    // on drop — whether from normal signal, error, or panic.
    let guard = shutdown::ShutdownGuard::new(
        svc.clone(),
        shutdown_flag.clone(),
        pw_chan_tx.clone(),
        pw_engine,
        vec![relay_handle, event_handle, signal_handle, flush_handle],
    );

    shutdown::wait_for_signal().await;
    info!("shutting down");

    drop(guard);
    Ok(())
}
