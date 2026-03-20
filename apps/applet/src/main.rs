mod dbus;
mod tray;

slint::include_modules!();

use std::sync::Arc;
use tokio::sync::mpsc;

/// Actions from UI -> tokio background thread
pub(crate) enum UserAction {
    SetOutputVolume { id: u32, volume: u8 },
    SetOutputMute { id: u32, muted: bool },
    SetRouteVolume { input_id: u32, output_id: u32, volume: u8 },
    SetRouteMute { input_id: u32, output_id: u32, muted: bool },
    SelectOutput { output_id: u32 },
}

fn main() {
    let window = PopupWindow::new().unwrap();
    let window_weak = window.as_weak();

    let (action_tx, action_rx) = mpsc::unbounded_channel::<UserAction>();
    let action_tx = Arc::new(action_tx);

    // Wire UI callbacks
    wire_callbacks(&window, &action_tx);

    // Spawn tokio runtime on background thread
    let bg_window = window_weak.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            dbus::run_background(bg_window, action_rx).await;
        });
    });

    // Don't show yet -- popup appears on tray click
    // Run the Slint event loop
    slint::run_event_loop().unwrap();
}

fn wire_callbacks(window: &PopupWindow, action_tx: &Arc<mpsc::UnboundedSender<UserAction>>) {
    let state = window.global::<AppletState>();

    let tx = action_tx.clone();
    state.on_set_output_volume(move |id, volume| {
        tx.send(UserAction::SetOutputVolume {
            id: id as u32,
            volume: volume as u8,
        }).ok();
    });

    let tx = action_tx.clone();
    state.on_set_output_mute(move |id, muted| {
        tx.send(UserAction::SetOutputMute {
            id: id as u32,
            muted,
        }).ok();
    });

    let tx = action_tx.clone();
    state.on_set_route_volume(move |input_id, output_id, volume| {
        tx.send(UserAction::SetRouteVolume {
            input_id: input_id as u32,
            output_id: output_id as u32,
            volume: volume as u8,
        }).ok();
    });

    let tx = action_tx.clone();
    state.on_set_route_mute(move |input_id, output_id, muted| {
        tx.send(UserAction::SetRouteMute {
            input_id: input_id as u32,
            output_id: output_id as u32,
            muted,
        }).ok();
    });

    let tx = action_tx.clone();
    state.on_select_output(move |id| {
        tx.send(UserAction::SelectOutput {
            output_id: id as u32,
        }).ok();
    });

    state.on_open_mixer(move || {
        std::process::Command::new("mixctl-ui").spawn().ok();
    });
}
