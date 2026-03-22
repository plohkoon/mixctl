use anyhow::Result;
use futures_lite::StreamExt;
use mixctl_core::config_sections::BeacnConfig;
use mixctl_core::dbus::MixCtlProxy;
use tokio::sync::mpsc;
use zbus::Connection;

use crate::app::{AppState, DaemonSignal};

/// Connect to the daemon and load initial state.
pub async fn connect_and_load() -> Result<(MixCtlProxy<'static>, AppState)> {
    let conn = Connection::session().await?;
    let proxy = MixCtlProxy::new(&conn).await?;
    proxy.ping().await?;
    proxy.register_component("tui").await.ok();

    let inputs = proxy.list_inputs().await?;
    let outputs = proxy.list_outputs().await?;
    let streams = proxy.list_streams().await?;
    let rules = proxy.list_app_rules().await.unwrap_or_default();
    let capture_devices = proxy.list_capture_devices().await.unwrap_or_default();
    let components = proxy.list_components().await.unwrap_or_default();

    let routes = if let Some(first_output) = outputs.first() {
        proxy.list_routes_for_output(first_output.id).await?
    } else {
        vec![]
    };

    let beacn_config = match proxy.get_config_section("beacn").await {
        Ok(json) => serde_json::from_str::<BeacnConfig>(&json).ok(),
        Err(_) => None,
    };

    let mut state = AppState::new(inputs, outputs, routes, streams, rules, capture_devices, components, beacn_config);

    // Load initial DSP state for all inputs
    for input in &state.inputs {
        let id = input.id;
        let eq_enabled = proxy.get_input_eq_enabled(id).await.unwrap_or(false);
        let eq_bands = proxy.get_input_eq(id).await.unwrap_or_default();
        state.dsp_input_eq.insert(id, (eq_enabled, eq_bands));
        if let Ok(gate) = proxy.get_input_gate(id).await {
            state.dsp_input_gate.insert(id, gate);
        }
        if let Ok(deesser) = proxy.get_input_deesser(id).await {
            state.dsp_input_deesser.insert(id, deesser);
        }
    }
    // Load initial DSP state for all outputs
    for output in &state.outputs {
        let id = output.id;
        if let Ok(comp) = proxy.get_output_compressor(id).await {
            state.dsp_output_compressor.insert(id, comp);
        }
        if let Ok(lim) = proxy.get_output_limiter(id).await {
            state.dsp_output_limiter.insert(id, lim);
        }
    }

    Ok((proxy, state))
}

/// Subscribe to all relevant D-Bus signals, funneling into a single channel.
pub async fn subscribe_signals(
    proxy: &MixCtlProxy<'static>,
) -> Result<mpsc::UnboundedReceiver<DaemonSignal>> {
    let (tx, rx) = mpsc::unbounded_channel();

    // route_changed → fetch the specific updated route
    let p = proxy.clone();
    let t: mpsc::UnboundedSender<DaemonSignal> = tx.clone();
    let mut stream = proxy.receive_route_changed().await?;
    tokio::spawn(async move {
        while let Some(signal) = stream.next().await {
            if let Ok(args) = signal.args() {
                let input_id = args.input_id;
                let output_id = args.output_id;
                if let Ok(route) = p.get_route(input_id, output_id).await {
                    t.send(DaemonSignal::RouteUpdated(route)).ok();
                }
            }
        }
    });

    // output_state_changed → re-fetch outputs
    let p = proxy.clone();
    let t = tx.clone();
    let mut stream = proxy.receive_output_state_changed().await?;
    tokio::spawn(async move {
        while stream.next().await.is_some() {
            if let Ok(outputs) = p.list_outputs().await {
                t.send(DaemonSignal::OutputsRefreshed(outputs)).ok();
            }
        }
    });

    // inputs_config_changed → full refresh
    let p = proxy.clone();
    let t = tx.clone();
    let mut stream = proxy.receive_inputs_config_changed().await?;
    tokio::spawn(async move {
        while stream.next().await.is_some() {
            send_full_refresh(&p, &t).await;
        }
    });

    // outputs_config_changed → full refresh
    let p = proxy.clone();
    let t = tx.clone();
    let mut stream = proxy.receive_outputs_config_changed().await?;
    tokio::spawn(async move {
        while stream.next().await.is_some() {
            send_full_refresh(&p, &t).await;
        }
    });

    // streams_changed → re-fetch streams
    let p = proxy.clone();
    let t = tx.clone();
    let mut stream = proxy.receive_streams_changed().await?;
    tokio::spawn(async move {
        while stream.next().await.is_some() {
            if let Ok(streams) = p.list_streams().await {
                t.send(DaemonSignal::StreamsRefreshed(streams)).ok();
            }
        }
    });

    // app_rules_changed → re-fetch rules
    let p = proxy.clone();
    let t = tx.clone();
    let mut stream = proxy.receive_app_rules_changed().await?;
    tokio::spawn(async move {
        while stream.next().await.is_some() {
            if let Ok(rules) = p.list_app_rules().await {
                t.send(DaemonSignal::RulesRefreshed(rules)).ok();
            }
        }
    });

    // capture_devices_changed → re-fetch capture devices
    let p = proxy.clone();
    let t = tx.clone();
    let mut stream = proxy.receive_capture_devices_changed().await?;
    tokio::spawn(async move {
        while stream.next().await.is_some() {
            if let Ok(devices) = p.list_capture_devices().await {
                t.send(DaemonSignal::CaptureDevicesRefreshed(devices)).ok();
            }
        }
    });

    // component_changed → re-fetch components
    let p = proxy.clone();
    let t = tx.clone();
    let mut stream = proxy.receive_component_changed().await?;
    tokio::spawn(async move {
        while stream.next().await.is_some() {
            if let Ok(components) = p.list_components().await {
                t.send(DaemonSignal::ComponentsRefreshed(components)).ok();
            }
        }
    });

    // config_section_changed → refresh beacn config when section == "beacn"
    let p = proxy.clone();
    let t = tx.clone();
    let mut stream = proxy.receive_config_section_changed().await?;
    tokio::spawn(async move {
        while let Some(signal) = stream.next().await {
            if let Ok(args) = signal.args() {
                if args.section == "beacn" {
                    if let Ok(json) = p.get_config_section("beacn").await {
                        if let Ok(config) = serde_json::from_str::<BeacnConfig>(&json) {
                            t.send(DaemonSignal::BeacnConfigRefreshed(config)).ok();
                        }
                    }
                }
            }
        }
    });

    // input_dsp_changed → re-fetch DSP state for that input
    let p = proxy.clone();
    let t = tx.clone();
    let mut stream = proxy.receive_input_dsp_changed().await?;
    tokio::spawn(async move {
        while let Some(signal) = stream.next().await {
            if let Ok(args) = signal.args() {
                let input_id = args.input_id;
                let eq_enabled = p.get_input_eq_enabled(input_id).await.unwrap_or(false);
                let eq_bands = p.get_input_eq(input_id).await.unwrap_or_default();
                let gate = p.get_input_gate(input_id).await.unwrap_or(mixctl_core::GateInfo {
                    enabled: false,
                    threshold_db: -40.0,
                    attack_ms: 1.0,
                    release_ms: 50.0,
                    hold_ms: 5.0,
                });
                let deesser = p.get_input_deesser(input_id).await.unwrap_or(mixctl_core::DeesserInfo {
                    enabled: false,
                    frequency: 6000.0,
                    threshold_db: -20.0,
                    ratio: 4.0,
                });
                t.send(DaemonSignal::InputDspRefreshed {
                    input_id,
                    eq_enabled,
                    eq_bands,
                    gate,
                    deesser,
                }).ok();
            }
        }
    });

    // output_dsp_changed → re-fetch DSP state for that output
    let p = proxy.clone();
    let t = tx.clone();
    let mut stream = proxy.receive_output_dsp_changed().await?;
    tokio::spawn(async move {
        while let Some(signal) = stream.next().await {
            if let Ok(args) = signal.args() {
                let output_id = args.output_id;
                let compressor = p.get_output_compressor(output_id).await.unwrap_or(mixctl_core::CompressorInfo {
                    enabled: false,
                    threshold_db: -18.0,
                    ratio: 4.0,
                    attack_ms: 10.0,
                    release_ms: 100.0,
                    makeup_gain_db: 0.0,
                    knee_db: 0.0,
                });
                let limiter = p.get_output_limiter(output_id).await.unwrap_or(mixctl_core::LimiterInfo {
                    enabled: false,
                    ceiling_db: -0.5,
                    release_ms: 50.0,
                });
                t.send(DaemonSignal::OutputDspRefreshed {
                    output_id,
                    compressor,
                    limiter,
                }).ok();
            }
        }
    });

    Ok(rx)
}

async fn send_full_refresh(
    proxy: &MixCtlProxy<'_>,
    tx: &mpsc::UnboundedSender<DaemonSignal>,
) {
    let inputs = proxy.list_inputs().await.unwrap_or_default();
    let outputs = proxy.list_outputs().await.unwrap_or_default();
    let streams = proxy.list_streams().await.unwrap_or_default();
    let routes = if let Some(first) = outputs.first() {
        proxy.list_routes_for_output(first.id).await.unwrap_or_default()
    } else {
        vec![]
    };
    tx.send(DaemonSignal::FullRefresh { inputs, outputs, routes, streams }).ok();
}
