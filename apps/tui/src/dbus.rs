use anyhow::Result;
use futures_lite::StreamExt;
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

    let state = AppState::new(inputs, outputs, routes, streams, rules, capture_devices, components);
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
