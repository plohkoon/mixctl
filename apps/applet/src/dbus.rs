use futures_lite::StreamExt;
use ksni::TrayMethods;
use mixctl_core::dbus::MixCtlProxy;
use mixctl_core::{InputInfo, OutputInfo, RouteInfo};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use tokio::task::JoinHandle;

use crate::tray::{MixCtlTray, TrayMsg};
use crate::{AppletState, OutputStripData, PopupWindow, RouteStripData, UserAction};

pub(crate) async fn run_background(
    window: slint::Weak<PopupWindow>,
    mut action_rx: tokio::sync::mpsc::UnboundedReceiver<UserAction>,
) {
    // Spawn ksni tray (lives for the entire app lifetime, outside reconnection loop)
    let (tray_tx, mut tray_rx) = tokio::sync::mpsc::unbounded_channel::<TrayMsg>();
    let tray = MixCtlTray {
        msg_tx: Mutex::new(tray_tx),
    };
    let _tray_handle: ksni::Handle<MixCtlTray> = tray.spawn().await.unwrap();

    // Handle tray messages
    let w_tray = window.clone();
    tokio::spawn(async move {
        while let Some(msg) = tray_rx.recv().await {
            match msg {
                TrayMsg::TogglePopup => {
                    let w = w_tray.clone();
                    slint::invoke_from_event_loop(move || {
                        if let Some(win) = w.upgrade() {
                            if win.window().is_visible() {
                                win.hide().ok();
                            } else {
                                win.show().ok();
                            }
                        }
                    }).ok();
                }
                TrayMsg::Quit => {
                    let w = w_tray.clone();
                    slint::invoke_from_event_loop(move || {
                        if let Some(win) = w.upgrade() {
                            win.hide().ok();
                        }
                        slint::quit_event_loop().ok();
                    }).ok();
                    break;
                }
            }
        }
    });

    // Reconnection loop
    loop {
        // Set disconnected state
        let w = window.clone();
        slint::invoke_from_event_loop(move || {
            if let Some(win) = w.upgrade() {
                win.global::<AppletState>().set_daemon_connected(false);
            }
        }).ok();

        match try_connect_and_run(&window, &mut action_rx).await {
            Ok(()) => {
                // action_rx was closed, app is shutting down
                break;
            }
            Err(e) => {
                eprintln!("Daemon connection lost: {e}");
                let w = window.clone();
                slint::invoke_from_event_loop(move || {
                    if let Some(win) = w.upgrade() {
                        win.global::<AppletState>().set_daemon_connected(false);
                    }
                }).ok();
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
    }
}

async fn try_connect_and_run(
    window: &slint::Weak<PopupWindow>,
    action_rx: &mut tokio::sync::mpsc::UnboundedReceiver<UserAction>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let conn = zbus::Connection::session().await?;
    let proxy = MixCtlProxy::new(&conn).await?;

    // Register with daemon
    proxy.register_component("applet").await.ok();

    // Mark connected
    let w = window.clone();
    slint::invoke_from_event_loop(move || {
        if let Some(win) = w.upgrade() {
            win.global::<AppletState>().set_daemon_connected(true);
        }
    }).ok();

    // Initial state
    let selected = Arc::new(AtomicU32::new(0));
    send_full_update(&proxy, window, &selected).await;

    // Channel for disconnect sentinel
    let (disconnect_tx, mut disconnect_rx) = tokio::sync::mpsc::unbounded_channel::<()>();

    // Spawn signal listeners, collecting handles
    let handles = spawn_signal_listeners(&proxy, window, &selected, disconnect_tx);

    // Process user actions until disconnect or channel close
    loop {
        tokio::select! {
            action = action_rx.recv() => {
                match action {
                    Some(action) => {
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
                                selected.store(output_id, Ordering::Relaxed);
                                send_full_update(&proxy, window, &selected).await;
                            }
                        }
                    }
                    None => {
                        // action channel closed, app is shutting down
                        for h in handles {
                            h.abort();
                        }
                        return Ok(());
                    }
                }
            }
            _ = disconnect_rx.recv() => {
                // A signal stream ended — daemon disconnected
                for h in handles {
                    h.abort();
                }
                return Err("signal stream ended (daemon disconnected)".into());
            }
        }
    }
}

fn spawn_signal_listeners(
    proxy: &MixCtlProxy<'static>,
    window: &slint::Weak<PopupWindow>,
    selected: &Arc<AtomicU32>,
    disconnect_tx: tokio::sync::mpsc::UnboundedSender<()>,
) -> Vec<JoinHandle<()>> {
    let mut handles = Vec::new();

    // Output state changed
    let p = proxy.clone();
    let w = window.clone();
    let sel = selected.clone();
    let disc = disconnect_tx.clone();
    handles.push(tokio::spawn(async move {
        let Ok(mut stream) = p.receive_output_state_changed().await else {
            disc.send(()).ok();
            return;
        };
        while let Some(signal) = stream.next().await {
            let args = signal.args().unwrap();
            if let Ok(out) = p.get_output(args.id).await {
                let sel_id = sel.load(Ordering::Relaxed);
                let w2 = w.clone();
                slint::invoke_from_event_loop(move || {
                    if let Some(win) = w2.upgrade() {
                        update_single_output(&win, &out, sel_id);
                    }
                }).ok();
            }
        }
        disc.send(()).ok();
    }));

    // Route changed
    let p = proxy.clone();
    let w = window.clone();
    let sel = selected.clone();
    let disc = disconnect_tx.clone();
    handles.push(tokio::spawn(async move {
        let Ok(mut stream) = p.receive_route_changed().await else {
            disc.send(()).ok();
            return;
        };
        while let Some(signal) = stream.next().await {
            let args = signal.args().unwrap();
            let sel_id = sel.load(Ordering::Relaxed);
            if args.output_id == sel_id {
                if let Ok(route) = p.get_route(args.input_id, args.output_id).await {
                    let inputs = p.list_inputs().await.unwrap_or_default();
                    let w2 = w.clone();
                    slint::invoke_from_event_loop(move || {
                        if let Some(win) = w2.upgrade() {
                            update_single_route(&win, &route, &inputs);
                        }
                    }).ok();
                }
            }
        }
        disc.send(()).ok();
    }));

    // Inputs config changed
    let p = proxy.clone();
    let w = window.clone();
    let sel = selected.clone();
    let disc = disconnect_tx.clone();
    handles.push(tokio::spawn(async move {
        let Ok(mut stream) = p.receive_inputs_config_changed().await else {
            disc.send(()).ok();
            return;
        };
        while let Some(_) = stream.next().await {
            send_full_update(&p, &w, &sel).await;
        }
        disc.send(()).ok();
    }));

    // Outputs config changed
    let p = proxy.clone();
    let w = window.clone();
    let sel = selected.clone();
    let disc = disconnect_tx.clone();
    handles.push(tokio::spawn(async move {
        let Ok(mut stream) = p.receive_outputs_config_changed().await else {
            disc.send(()).ok();
            return;
        };
        while let Some(_) = stream.next().await {
            send_full_update(&p, &w, &sel).await;
        }
        disc.send(()).ok();
    }));

    handles
}

async fn send_full_update(
    proxy: &MixCtlProxy<'_>,
    window: &slint::Weak<PopupWindow>,
    selected: &Arc<AtomicU32>,
) {
    let outputs = proxy.list_outputs().await.unwrap_or_default();
    let inputs = proxy.list_inputs().await.unwrap_or_default();

    let mut sel_id = selected.load(Ordering::Relaxed);
    if sel_id == 0 {
        if let Some(first) = outputs.first() {
            sel_id = first.id;
            selected.store(sel_id, Ordering::Relaxed);
        }
    }
    // If selected was removed, pick first
    if !outputs.iter().any(|o| o.id == sel_id) {
        sel_id = outputs.first().map(|o| o.id).unwrap_or(0);
        selected.store(sel_id, Ordering::Relaxed);
    }

    let routes = if sel_id > 0 {
        proxy.list_routes_for_output(sel_id).await.unwrap_or_default()
    } else {
        Vec::new()
    };

    let w = window.clone();
    slint::invoke_from_event_loop(move || {
        if let Some(win) = w.upgrade() {
            set_full_state(&win, &outputs, &inputs, &routes, sel_id);
        }
    }).ok();
}

fn set_full_state(
    win: &PopupWindow,
    outputs: &[OutputInfo],
    inputs: &[InputInfo],
    routes: &[RouteInfo],
    selected_id: u32,
) {
    let state = win.global::<AppletState>();

    // Set selected output name
    let sel_name = outputs.iter()
        .find(|o| o.id == selected_id)
        .map(|o| o.name.as_str())
        .unwrap_or("(none)");
    state.set_selected_output_name(SharedString::from(sel_name));

    // Output strips
    let output_data: Vec<OutputStripData> = outputs.iter().map(|o| {
        OutputStripData {
            id: o.id as i32,
            name: SharedString::from(o.name.as_str()),
            color: parse_color(&o.color),
            volume: o.volume as i32,
            muted: o.muted,
            selected: o.id == selected_id,
        }
    }).collect();
    state.set_outputs(ModelRc::new(VecModel::from(output_data)));

    // Route strips
    let route_data: Vec<RouteStripData> = routes.iter().map(|r| {
        let (name, color) = find_input_info(inputs, r.input_id);
        RouteStripData {
            input_id: r.input_id as i32,
            output_id: r.output_id as i32,
            name: SharedString::from(name),
            color,
            volume: r.volume as i32,
            muted: r.muted,
        }
    }).collect();
    state.set_routes(ModelRc::new(VecModel::from(route_data)));
}

fn update_single_output(win: &PopupWindow, out: &OutputInfo, selected_id: u32) {
    let state = win.global::<AppletState>();
    let model = state.get_outputs();
    let count = model.row_count();
    for i in 0..count {
        if let Some(row) = model.row_data(i) {
            if row.id == out.id as i32 {
                let updated = OutputStripData {
                    id: out.id as i32,
                    name: SharedString::from(out.name.as_str()),
                    color: parse_color(&out.color),
                    volume: out.volume as i32,
                    muted: out.muted,
                    selected: out.id == selected_id,
                };
                model.set_row_data(i, updated);
                break;
            }
        }
    }
}

fn update_single_route(win: &PopupWindow, route: &RouteInfo, inputs: &[InputInfo]) {
    let state = win.global::<AppletState>();
    let model = state.get_routes();
    let count = model.row_count();
    for i in 0..count {
        if let Some(row) = model.row_data(i) {
            if row.input_id == route.input_id as i32 && row.output_id == route.output_id as i32 {
                let (name, color) = find_input_info(inputs, route.input_id);
                let updated = RouteStripData {
                    input_id: route.input_id as i32,
                    output_id: route.output_id as i32,
                    name: SharedString::from(name),
                    color,
                    volume: route.volume as i32,
                    muted: route.muted,
                };
                model.set_row_data(i, updated);
                break;
            }
        }
    }
}

fn find_input_info(inputs: &[InputInfo], input_id: u32) -> (&str, slint::Color) {
    inputs.iter()
        .find(|i| i.id == input_id)
        .map(|i| (i.name.as_str(), parse_color(&i.color)))
        .unwrap_or(("?", slint::Color::from_rgb_u8(136, 136, 136)))
}

fn parse_color(hex: &str) -> slint::Color {
    match mixctl_core::parse_hex_color(hex) {
        Some((r, g, b)) => slint::Color::from_rgb_u8(r, g, b),
        None => slint::Color::from_rgb_u8(136, 136, 136),
    }
}
