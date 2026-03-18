use std::sync::Mutex;

use futures_lite::StreamExt;
use ksni::TrayMethods;
use mixctl_core::dbus::MixCtlProxy;

use crate::tray::MixCtlTray;
use crate::{AppletMsg, UserAction};

pub(crate) async fn run_background(
    msg_tx: std::sync::mpsc::Sender<AppletMsg>,
    mut action_rx: tokio::sync::mpsc::UnboundedReceiver<UserAction>,
) {
    let conn = zbus::Connection::session().await.unwrap();
    let proxy = MixCtlProxy::new(&conn).await.unwrap();

    // Send initial full update
    send_full_update(&proxy, &msg_tx, 0).await;

    // Spawn ksni tray
    let tray = MixCtlTray {
        msg_tx: Mutex::new(msg_tx.clone()),
    };
    let _tray_handle: ksni::Handle<MixCtlTray> = tray.spawn().await.unwrap();

    // Track selected output on the tokio side
    let selected_output_id = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));

    // Spawn signal listener: output state changed
    let state_proxy = proxy.clone();
    let state_tx = msg_tx.clone();
    tokio::spawn(async move {
        let mut stream = state_proxy.receive_output_state_changed().await.unwrap();
        while let Some(signal) = stream.next().await {
            let args = signal.args().unwrap();
            if let Ok(out) = state_proxy.get_output(args.id).await {
                state_tx.send(AppletMsg::OutputStateUpdated(out)).ok();
            }
        }
    });

    // Spawn signal listener: route changed
    let route_proxy = proxy.clone();
    let route_tx = msg_tx.clone();
    tokio::spawn(async move {
        let mut stream = route_proxy.receive_route_changed().await.unwrap();
        while let Some(signal) = stream.next().await {
            let args = signal.args().unwrap();
            if let Ok(route) = route_proxy.get_route(args.input_id, args.output_id).await {
                route_tx.send(AppletMsg::RouteUpdated(route)).ok();
            }
        }
    });

    // Spawn signal listener: inputs config changed
    let inputs_proxy = proxy.clone();
    let inputs_tx = msg_tx.clone();
    let inputs_sel = selected_output_id.clone();
    tokio::spawn(async move {
        let mut stream = inputs_proxy.receive_inputs_config_changed().await.unwrap();
        while let Some(_) = stream.next().await {
            let sel = inputs_sel.load(std::sync::atomic::Ordering::Relaxed);
            send_full_update(&inputs_proxy, &inputs_tx, sel).await;
        }
    });

    // Spawn signal listener: outputs config changed
    let outputs_proxy = proxy.clone();
    let outputs_tx = msg_tx.clone();
    let outputs_sel = selected_output_id.clone();
    tokio::spawn(async move {
        let mut stream = outputs_proxy.receive_outputs_config_changed().await.unwrap();
        while let Some(_) = stream.next().await {
            let sel = outputs_sel.load(std::sync::atomic::Ordering::Relaxed);
            send_full_update(&outputs_proxy, &outputs_tx, sel).await;
        }
    });

    // Process user actions from GTK thread
    while let Some(action) = action_rx.recv().await {
        match action {
            UserAction::SetOutputVolume { id, volume } => {
                proxy.set_output_volume(id, volume).await.ok();
            }
            UserAction::SetOutputMute { id, muted } => {
                proxy.set_output_mute(id, muted).await.ok();
            }
            UserAction::SetRouteVolume { input_id, output_id, volume } => {
                proxy.set_route_volume(input_id, output_id, volume).await.ok();
            }
            UserAction::SetRouteMute { input_id, output_id, muted } => {
                proxy.set_route_mute(input_id, output_id, muted).await.ok();
            }
            UserAction::SelectOutput { output_id } => {
                selected_output_id.store(output_id, std::sync::atomic::Ordering::Relaxed);
                send_full_update(&proxy, &msg_tx, output_id).await;
            }
        }
    }
}

async fn send_full_update(
    proxy: &MixCtlProxy<'_>,
    msg_tx: &std::sync::mpsc::Sender<AppletMsg>,
    selected_output_id: u32,
) {
    let outputs = proxy.list_outputs().await.unwrap_or_default();
    let inputs = proxy.list_inputs().await.unwrap_or_default();
    // Get routes for the selected output (or first output if none selected)
    let output_id = if selected_output_id > 0 {
        selected_output_id
    } else {
        outputs.first().map(|o| o.id).unwrap_or(0)
    };
    let routes = if output_id > 0 {
        proxy.list_routes_for_output(output_id).await.unwrap_or_default()
    } else {
        Vec::new()
    };
    msg_tx.send(AppletMsg::FullUpdate { outputs, inputs, routes }).ok();
}
