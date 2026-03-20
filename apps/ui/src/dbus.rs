use futures_lite::StreamExt;
use mixctl_core::dbus::MixCtlProxy;
use mixctl_core::{InputInfo, OutputInfo, PlaybackDeviceInfo, RouteInfo, StreamInfo};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::{
    AppRuleData, BeacnDialog, CaptureDeviceData, CaptureDialog, MainWindow, MixerState,
    RouteData, RulesDialog, SidebarOutput, StreamData, UserAction,
};

pub(crate) async fn run_background(
    window: slint::Weak<MainWindow>,
    mut action_rx: mpsc::UnboundedReceiver<UserAction>,
) {
    let conn = match zbus::Connection::session().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to connect to D-Bus: {e}");
            return;
        }
    };
    let proxy = MixCtlProxy::new(&conn).await.unwrap();

    // Initial state
    if let Ok(status) = proxy.get_audio_status().await {
        let connected = status == "connected";
        let w = window.clone();
        slint::invoke_from_event_loop(move || {
            if let Some(win) = w.upgrade() {
                win.global::<MixerState>().set_audio_connected(connected);
            }
        }).ok();
    }

    let outputs = proxy.list_outputs().await.unwrap_or_default();
    let inputs = proxy.list_inputs().await.unwrap_or_default();
    let default_input_id = proxy.get_default_input().await.unwrap_or(0);

    let selected_id = outputs.first().map(|o| o.id).unwrap_or(0);
    let routes = if selected_id > 0 {
        proxy.list_routes_for_output(selected_id).await.unwrap_or_default()
    } else {
        Vec::new()
    };
    let streams = proxy.list_streams().await.unwrap_or_default();
    let playback_devices = proxy.list_playback_devices().await.unwrap_or_default();

    push_full_state(&window, &outputs, &inputs, &routes, &streams, &playback_devices, selected_id, default_input_id);

    // Track selected output on the tokio side
    let selected = Arc::new(std::sync::atomic::AtomicU32::new(selected_id));

    // Spawn signal listeners
    spawn_signal_listeners(&proxy, &window, &selected);

    // Process user actions
    let inputs_ref = inputs;
    while let Some(action) = action_rx.recv().await {
        match action {
            UserAction::SetRouteVolume { input_id, output_id, volume } => {
                proxy.set_route_volume(input_id, output_id, volume).await.ok();
            }
            UserAction::SetRouteMute { input_id, output_id, muted } => {
                proxy.set_route_mute(input_id, output_id, muted).await.ok();
            }
            UserAction::SetOutputVolume { id, volume } => {
                proxy.set_output_volume(id, volume).await.ok();
            }
            UserAction::SetOutputMute { id, muted } => {
                proxy.set_output_mute(id, muted).await.ok();
            }
            UserAction::SelectOutput { output_id } => {
                selected.store(output_id, std::sync::atomic::Ordering::Relaxed);
                refresh_for_output(&proxy, &window, &inputs_ref, output_id).await;
            }
            UserAction::SetOutputTarget { id, device_index } => {
                let devices = proxy.list_playback_devices().await.unwrap_or_default();
                let device_name = if device_index == 0 {
                    String::new() // "None" — unbind
                } else {
                    devices.get(device_index - 1)
                        .map(|d| d.device_name.clone())
                        .unwrap_or_default()
                };
                proxy.set_output_target(id, &device_name).await.ok();
            }
            UserAction::SetDefaultInput { index } => {
                let inputs = proxy.list_inputs().await.unwrap_or_default();
                if let Some(input) = inputs.get(index) {
                    proxy.set_default_input(input.id).await.ok();
                }
            }
            UserAction::AssignStream { pw_node_id, input_id, remember } => {
                proxy.assign_stream(pw_node_id, input_id, remember).await.ok();
            }
            UserAction::AddRule { app_name, input_index } => {
                let inputs = proxy.list_inputs().await.unwrap_or_default();
                if let Some(input) = inputs.get(input_index) {
                    proxy.set_app_rule(&app_name, input.id).await.ok();
                }
            }
            UserAction::RemoveRule { app_name } => {
                proxy.remove_app_rule(&app_name).await.ok();
            }
            UserAction::AddCapture { pw_node_id, name, color } => {
                proxy.add_capture_input(pw_node_id, &name, &color).await.ok();
            }
            UserAction::ApplyBeacn { layout, dial_sensitivity, level_decay } => {
                let config = serde_json::json!({
                    "layout": layout,
                    "dial_sensitivity": dial_sensitivity,
                    "level_decay": level_decay,
                });
                proxy.set_config_section("beacn", &config.to_string()).await.ok();
            }
            UserAction::OpenRulesDialog => {
                open_rules_dialog(&proxy).await;
            }
            UserAction::OpenCaptureDialog => {
                open_capture_dialog(&proxy).await;
            }
            UserAction::OpenBeacnDialog => {
                open_beacn_dialog(&proxy).await;
            }
        }
    }
}

async fn open_rules_dialog(proxy: &MixCtlProxy<'static>) {
    let rules = proxy.list_app_rules().await.unwrap_or_default();
    let inputs = proxy.list_inputs().await.unwrap_or_default();

    let input_names: Vec<SharedString> = inputs.iter().map(|i| SharedString::from(i.name.as_str())).collect();
    let rule_data: Vec<AppRuleData> = rules.iter()
        .filter(|r| !r.app_name.contains("mixctl.") && !r.app_name.starts_with("output."))
        .map(|r| {
        let input_name = inputs.iter()
            .find(|i| i.id == r.input_id)
            .map(|i| i.name.as_str())
            .unwrap_or("?");
        AppRuleData {
            app_name: SharedString::from(r.app_name.as_str()),
            input_id: r.input_id as i32,
            input_name: SharedString::from(input_name),
        }
    }).collect();

    let p = proxy.clone();
    slint::invoke_from_event_loop(move || {
        let dialog = RulesDialog::new().unwrap();
        dialog.set_rules(ModelRc::new(VecModel::from(rule_data)));
        dialog.set_input_names(ModelRc::new(VecModel::from(input_names.clone())));

        let p2 = p.clone();
        let inputs2 = input_names.clone();
        let d_weak = dialog.as_weak();
        dialog.on_add_rule(move |app_name, input_index| {
            let app = app_name.to_string();
            let idx = input_index as usize;
            let p3 = p2.clone();
            let d = d_weak.clone();
            let inputs3 = inputs2.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    let inputs = p3.list_inputs().await.unwrap_or_default();
                    if let Some(input) = inputs.get(idx) {
                        p3.set_app_rule(&app, input.id).await.ok();
                    }
                    // Refresh dialog
                    let rules = p3.list_app_rules().await.unwrap_or_default();
                    let rule_data: Vec<AppRuleData> = rules.iter().map(|r| {
                        let iname = inputs.iter()
                            .find(|i| i.id == r.input_id)
                            .map(|i| i.name.as_str())
                            .unwrap_or("?");
                        AppRuleData {
                            app_name: SharedString::from(r.app_name.as_str()),
                            input_id: r.input_id as i32,
                            input_name: SharedString::from(iname),
                        }
                    }).collect();
                    slint::invoke_from_event_loop(move || {
                        if let Some(d) = d.upgrade() {
                            d.set_rules(ModelRc::new(VecModel::from(rule_data)));
                            d.set_input_names(ModelRc::new(VecModel::from(inputs3)));
                        }
                    }).ok();
                });
            });
        });

        let p2 = p.clone();
        let d_weak = dialog.as_weak();
        dialog.on_remove_rule(move |app_name| {
            let app = app_name.to_string();
            let p3 = p2.clone();
            let d = d_weak.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    p3.remove_app_rule(&app).await.ok();
                    let rules = p3.list_app_rules().await.unwrap_or_default();
                    let inputs = p3.list_inputs().await.unwrap_or_default();
                    let rule_data: Vec<AppRuleData> = rules.iter().map(|r| {
                        let iname = inputs.iter()
                            .find(|i| i.id == r.input_id)
                            .map(|i| i.name.as_str())
                            .unwrap_or("?");
                        AppRuleData {
                            app_name: SharedString::from(r.app_name.as_str()),
                            input_id: r.input_id as i32,
                            input_name: SharedString::from(iname),
                        }
                    }).collect();
                    slint::invoke_from_event_loop(move || {
                        if let Some(d) = d.upgrade() {
                            d.set_rules(ModelRc::new(VecModel::from(rule_data)));
                        }
                    }).ok();
                });
            });
        });

        dialog.show().unwrap();
    }).ok();
}

async fn open_capture_dialog(proxy: &MixCtlProxy<'static>) {
    let devices = proxy.list_capture_devices().await.unwrap_or_default();

    let device_data: Vec<CaptureDeviceData> = devices.iter().map(|d| {
        CaptureDeviceData {
            pw_node_id: d.pw_node_id as i32,
            name: SharedString::from(d.name.as_str()),
            device_name: SharedString::from(d.device_name.as_str()),
            is_added: d.is_added,
            input_id: d.input_id as i32,
        }
    }).collect();

    let p = proxy.clone();
    slint::invoke_from_event_loop(move || {
        let dialog = CaptureDialog::new().unwrap();
        dialog.set_devices(ModelRc::new(VecModel::from(device_data)));

        let p2 = p.clone();
        let d_weak = dialog.as_weak();
        dialog.on_add_capture(move |pw_node_id, name, color| {
            let p3 = p2.clone();
            let d = d_weak.clone();
            let name = name.to_string();
            let color = color.to_string();
            let pw_id = pw_node_id as u32;
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    p3.add_capture_input(pw_id, &name, &color).await.ok();
                    let devices = p3.list_capture_devices().await.unwrap_or_default();
                    let data: Vec<CaptureDeviceData> = devices.iter().map(|d| {
                        CaptureDeviceData {
                            pw_node_id: d.pw_node_id as i32,
                            name: SharedString::from(d.name.as_str()),
                            device_name: SharedString::from(d.device_name.as_str()),
                            is_added: d.is_added,
                            input_id: d.input_id as i32,
                        }
                    }).collect();
                    slint::invoke_from_event_loop(move || {
                        if let Some(d) = d.upgrade() {
                            d.set_devices(ModelRc::new(VecModel::from(data)));
                        }
                    }).ok();
                });
            });
        });

        dialog.show().unwrap();
    }).ok();
}

async fn open_beacn_dialog(proxy: &MixCtlProxy<'static>) {
    let beacn_json = proxy.get_config_section("beacn").await.unwrap_or_default();
    let config: mixctl_core::config_sections::BeacnConfig =
        serde_json::from_str(&beacn_json).unwrap_or_default();

    let layout = config.layout.clone();
    let sensitivity = config.dial_sensitivity;
    let decay_pct = (config.level_decay * 100.0) as i32;

    let p = proxy.clone();
    slint::invoke_from_event_loop(move || {
        let dialog = BeacnDialog::new().unwrap();
        dialog.set_layout(SharedString::from(layout.as_str()));
        dialog.set_dial_sensitivity(sensitivity as i32);
        dialog.set_level_decay_pct(decay_pct);

        let p2 = p.clone();
        let d_weak = dialog.as_weak();
        dialog.on_apply(move || {
            let d = d_weak.clone();
            let p3 = p2.clone();
            if let Some(dlg) = d.upgrade() {
                let layout = dlg.get_layout().to_string();
                let dial_sensitivity = dlg.get_dial_sensitivity() as u32;
                let level_decay = dlg.get_level_decay_pct() as f64 / 100.0;
                std::thread::spawn(move || {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(async move {
                        let config = serde_json::json!({
                            "layout": layout,
                            "dial_sensitivity": dial_sensitivity,
                            "level_decay": level_decay,
                        });
                        p3.set_config_section("beacn", &config.to_string()).await.ok();
                    });
                });
            }
        });

        dialog.show().unwrap();
    }).ok();
}

fn spawn_signal_listeners(
    proxy: &MixCtlProxy<'static>,
    window: &slint::Weak<MainWindow>,
    selected: &Arc<std::sync::atomic::AtomicU32>,
) {
    // Audio status changed
    let p = proxy.clone();
    let w = window.clone();
    tokio::spawn(async move {
        let mut stream = p.receive_audio_status_changed().await.unwrap();
        while let Some(_) = stream.next().await {
            if let Ok(status) = p.get_audio_status().await {
                let connected = status == "connected";
                let w2 = w.clone();
                slint::invoke_from_event_loop(move || {
                    if let Some(win) = w2.upgrade() {
                        win.global::<MixerState>().set_audio_connected(connected);
                    }
                }).ok();
            }
        }
    });

    // Route changed
    let p = proxy.clone();
    let w = window.clone();
    let sel = selected.clone();
    tokio::spawn(async move {
        let mut stream = p.receive_route_changed().await.unwrap();
        while let Some(signal) = stream.next().await {
            let args = signal.args().unwrap();
            let sel_id = sel.load(std::sync::atomic::Ordering::Relaxed);
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
    });

    // Output state changed
    let p = proxy.clone();
    let w = window.clone();
    let sel = selected.clone();
    tokio::spawn(async move {
        let mut stream = p.receive_output_state_changed().await.unwrap();
        while let Some(_) = stream.next().await {
            do_full_refresh(&p, &w, &sel).await;
        }
    });

    // Inputs config changed
    let p = proxy.clone();
    let w = window.clone();
    let sel = selected.clone();
    tokio::spawn(async move {
        let mut stream = p.receive_inputs_config_changed().await.unwrap();
        while let Some(_) = stream.next().await {
            do_full_refresh(&p, &w, &sel).await;
        }
    });

    // Outputs config changed
    let p = proxy.clone();
    let w = window.clone();
    let sel = selected.clone();
    tokio::spawn(async move {
        let mut stream = p.receive_outputs_config_changed().await.unwrap();
        while let Some(_) = stream.next().await {
            do_full_refresh(&p, &w, &sel).await;
        }
    });

    // Streams changed
    let p = proxy.clone();
    let w = window.clone();
    tokio::spawn(async move {
        let mut stream = p.receive_streams_changed().await.unwrap();
        while let Some(_) = stream.next().await {
            let inputs = p.list_inputs().await.unwrap_or_default();
            let streams = p.list_streams().await.unwrap_or_default();
            let w2 = w.clone();
            slint::invoke_from_event_loop(move || {
                if let Some(win) = w2.upgrade() {
                    set_streams(&win, &streams, &inputs);
                }
            }).ok();
        }
    });

    // Playback devices changed
    let p = proxy.clone();
    let w = window.clone();
    let sel = selected.clone();
    tokio::spawn(async move {
        let mut stream = p.receive_playback_devices_changed().await.unwrap();
        while let Some(_) = stream.next().await {
            do_full_refresh(&p, &w, &sel).await;
        }
    });
}

async fn do_full_refresh(
    proxy: &MixCtlProxy<'static>,
    window: &slint::Weak<MainWindow>,
    selected: &Arc<std::sync::atomic::AtomicU32>,
) {
    let outputs = proxy.list_outputs().await.unwrap_or_default();
    let inputs = proxy.list_inputs().await.unwrap_or_default();
    let default_input_id = proxy.get_default_input().await.unwrap_or(0);
    let streams = proxy.list_streams().await.unwrap_or_default();
    let playback_devices = proxy.list_playback_devices().await.unwrap_or_default();

    let mut sel_id = selected.load(std::sync::atomic::Ordering::Relaxed);
    // If selected output was removed, pick first
    if !outputs.iter().any(|o| o.id == sel_id) {
        sel_id = outputs.first().map(|o| o.id).unwrap_or(0);
        selected.store(sel_id, std::sync::atomic::Ordering::Relaxed);
    }

    let routes = if sel_id > 0 {
        proxy.list_routes_for_output(sel_id).await.unwrap_or_default()
    } else {
        Vec::new()
    };

    let w = window.clone();
    slint::invoke_from_event_loop(move || {
        if let Some(win) = w.upgrade() {
            set_all_state(&win, &outputs, &inputs, &routes, &streams, &playback_devices, sel_id, default_input_id);
        }
    }).ok();
}

async fn refresh_for_output(
    proxy: &MixCtlProxy<'static>,
    window: &slint::Weak<MainWindow>,
    _inputs: &[InputInfo],
    output_id: u32,
) {
    let inputs = proxy.list_inputs().await.unwrap_or_default();
    let routes = if output_id > 0 {
        proxy.list_routes_for_output(output_id).await.unwrap_or_default()
    } else {
        Vec::new()
    };
    let w = window.clone();
    slint::invoke_from_event_loop(move || {
        if let Some(win) = w.upgrade() {
            let mixer = win.global::<MixerState>();
            mixer.set_selected_output_id(output_id as i32);
            set_routes_model(&mixer, &routes, &inputs);
        }
    }).ok();
}

fn push_full_state(
    window: &slint::Weak<MainWindow>,
    outputs: &[OutputInfo],
    inputs: &[InputInfo],
    routes: &[RouteInfo],
    streams: &[StreamInfo],
    playback_devices: &[PlaybackDeviceInfo],
    selected_id: u32,
    default_input_id: u32,
) {
    let outputs = outputs.to_vec();
    let inputs = inputs.to_vec();
    let routes = routes.to_vec();
    let streams = streams.to_vec();
    let playback_devices = playback_devices.to_vec();
    let w = window.clone();
    slint::invoke_from_event_loop(move || {
        if let Some(win) = w.upgrade() {
            set_all_state(&win, &outputs, &inputs, &routes, &streams, &playback_devices, selected_id, default_input_id);
        }
    }).ok();
}

fn set_all_state(
    win: &MainWindow,
    outputs: &[OutputInfo],
    inputs: &[InputInfo],
    routes: &[RouteInfo],
    streams: &[StreamInfo],
    playback_devices: &[PlaybackDeviceInfo],
    selected_id: u32,
    default_input_id: u32,
) {
    let mixer = win.global::<MixerState>();

    // Playback device names: ["None", "Speaker", "Headphones", ...]
    let mut pb_names: Vec<SharedString> = vec![SharedString::from("None")];
    for d in playback_devices {
        pb_names.push(SharedString::from(d.name.as_str()));
    }
    mixer.set_playback_device_names(ModelRc::new(VecModel::from(pb_names)));

    // Outputs — use SidebarOutput structs with target device index
    let output_data: Vec<SidebarOutput> = outputs.iter().map(|o| {
        let target_idx = if o.target_device.is_empty() {
            0 // "None"
        } else {
            playback_devices.iter()
                .position(|d| d.device_name == o.target_device)
                .map(|i| i + 1) // +1 for "None" at index 0
                .unwrap_or(0)
        };
        SidebarOutput {
            id: o.id as i32,
            name: SharedString::from(o.name.as_str()),
            color: parse_color(&o.color),
            target_device_index: target_idx as i32,
        }
    }).collect();
    mixer.set_outputs(ModelRc::new(VecModel::from(output_data)));
    mixer.set_selected_output_id(selected_id as i32);

    // Inputs
    let input_names: Vec<SharedString> = inputs.iter().map(|i| SharedString::from(i.name.as_str())).collect();
    mixer.set_input_names(ModelRc::new(VecModel::from(input_names)));

    // Default input index
    let default_idx = inputs.iter().position(|i| i.id == default_input_id).unwrap_or(0);
    mixer.set_default_input_index(default_idx as i32);

    // Routes
    set_routes_model(&mixer, routes, inputs);

    // Streams
    set_streams(win, streams, inputs);
}

fn set_routes_model(mixer: &MixerState, routes: &[RouteInfo], inputs: &[InputInfo]) {
    let route_data: Vec<RouteData> = routes.iter().map(|r| {
        let (name, color) = find_input_info(inputs, r.input_id);
        RouteData {
            input_id: r.input_id as i32,
            output_id: r.output_id as i32,
            volume: r.volume as i32,
            muted: r.muted,
            input_name: name.into(),
            input_color: color,
        }
    }).collect();
    mixer.set_routes(ModelRc::new(VecModel::from(route_data)));
}

fn set_streams(win: &MainWindow, streams: &[StreamInfo], inputs: &[InputInfo]) {
    let mixer = win.global::<MixerState>();
    let stream_data: Vec<StreamData> = streams.iter()
        .filter(|s| !s.app_name.contains("mixctl.") && !s.app_name.starts_with("output."))
        .map(|s| {
            let (name, color) = find_input_info(inputs, s.input_id);
            StreamData {
                pw_node_id: s.pw_node_id as i32,
                app_name: s.app_name.as_str().into(),
                input_name: name.into(),
                input_color: color,
                input_id: s.input_id as i32,
            }
        }).collect();
    mixer.set_streams(ModelRc::new(VecModel::from(stream_data)));
}

fn update_single_route(win: &MainWindow, route: &RouteInfo, inputs: &[InputInfo]) {
    let mixer = win.global::<MixerState>();
    let routes_model = mixer.get_routes();
    let count = routes_model.row_count();
    for i in 0..count {
        if let Some(r) = routes_model.row_data(i) {
            if r.input_id == route.input_id as i32 && r.output_id == route.output_id as i32 {
                let (name, color) = find_input_info(inputs, route.input_id);
                let updated = RouteData {
                    input_id: route.input_id as i32,
                    output_id: route.output_id as i32,
                    volume: route.volume as i32,
                    muted: route.muted,
                    input_name: name.into(),
                    input_color: color,
                };
                routes_model.set_row_data(i, updated);
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
