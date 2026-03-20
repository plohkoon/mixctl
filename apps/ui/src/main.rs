mod dbus;

slint::include_modules!();

use std::sync::Arc;
use tokio::sync::mpsc;

/// Actions from UI -> tokio background thread
#[allow(dead_code)]
pub(crate) enum UserAction {
    SetRouteVolume { input_id: u32, output_id: u32, volume: u8 },
    SetRouteMute { input_id: u32, output_id: u32, muted: bool },
    SetOutputVolume { id: u32, volume: u8 },
    SetOutputMute { id: u32, muted: bool },
    SelectOutput { output_id: u32 },
    SetOutputTarget { id: u32, device_index: usize },
    SetDefaultInput { index: usize },
    AssignStream { pw_node_id: u32, input_id: u32, remember: bool },
    AddRule { app_name: String, input_index: usize },
    RemoveRule { app_name: String },
    AddCapture { pw_node_id: u32, name: String, color: String },
    ApplyBeacn { layout: String, dial_sensitivity: u32, level_decay: f64 },
    OpenRulesDialog,
    OpenCaptureDialog,
    OpenBeacnDialog,
}

fn main() {
    let window = MainWindow::new().unwrap();
    let window_weak = window.as_weak();

    let (action_tx, action_rx) = mpsc::unbounded_channel::<UserAction>();
    let action_tx = Arc::new(action_tx);

    // Wire UI callbacks -> action channel
    wire_callbacks(&window, &action_tx);

    // Spawn tokio runtime on background thread
    let bg_window = window_weak.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            dbus::run_background(bg_window, action_rx).await;
        });
    });

    window.run().unwrap();
}

fn wire_callbacks(window: &MainWindow, action_tx: &Arc<mpsc::UnboundedSender<UserAction>>) {
    let mixer = window.global::<MixerState>();

    let tx = action_tx.clone();
    mixer.on_set_route_volume(move |input_id, output_id, volume| {
        tx.send(UserAction::SetRouteVolume {
            input_id: input_id as u32,
            output_id: output_id as u32,
            volume: volume as u8,
        }).ok();
    });

    let tx = action_tx.clone();
    mixer.on_set_route_mute(move |input_id, output_id, muted| {
        tx.send(UserAction::SetRouteMute {
            input_id: input_id as u32,
            output_id: output_id as u32,
            muted,
        }).ok();
    });

    let tx = action_tx.clone();
    mixer.on_select_output(move |id| {
        tx.send(UserAction::SelectOutput {
            output_id: id as u32,
        }).ok();
    });

    let tx = action_tx.clone();
    mixer.on_set_output_target(move |id, device_index| {
        tx.send(UserAction::SetOutputTarget {
            id: id as u32,
            device_index: device_index as usize,
        }).ok();
    });

    let tx = action_tx.clone();
    mixer.on_set_default_input(move |index| {
        tx.send(UserAction::SetDefaultInput {
            index: index as usize,
        }).ok();
    });

    let tx = action_tx.clone();
    mixer.on_assign_stream(move |pw_node_id, input_id, remember| {
        tx.send(UserAction::AssignStream {
            pw_node_id: pw_node_id as u32,
            input_id: input_id as u32,
            remember,
        }).ok();
    });

    let tx = action_tx.clone();
    mixer.on_open_rules_dialog(move || {
        tx.send(UserAction::OpenRulesDialog).ok();
    });

    let tx = action_tx.clone();
    mixer.on_open_capture_dialog(move || {
        tx.send(UserAction::OpenCaptureDialog).ok();
    });

    let tx = action_tx.clone();
    mixer.on_open_beacn_dialog(move || {
        tx.send(UserAction::OpenBeacnDialog).ok();
    });
}
