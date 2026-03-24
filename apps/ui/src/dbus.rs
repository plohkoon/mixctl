use futures_lite::StreamExt;
use mixctl_core::dbus::MixCtlProxy;
use mixctl_core::{InputInfo, OutputInfo, PlaybackDeviceInfo, RouteInfo, StreamInfo};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::{
    AppRuleData, BeacnDialog, CaptureDeviceData, CaptureDialog, ChannelItemData, ChannelsDialog,
    CompressorData, DeesserData, DspDialog, EqBandData, GateData, LimiterData, MainWindow,
    MixerState, RouteData, RulesDialog, SidebarOutput, StreamData, UserAction,
};

pub(crate) async fn run_background(
    window: slint::Weak<MainWindow>,
    mut action_rx: mpsc::UnboundedReceiver<UserAction>,
) {
    // Reconnection loop
    loop {
        // Set disconnected state
        let w = window.clone();
        slint::invoke_from_event_loop(move || {
            if let Some(win) = w.upgrade() {
                win.global::<MixerState>().set_daemon_connected(false);
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
                        win.global::<MixerState>().set_daemon_connected(false);
                    }
                }).ok();
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
    }
}

async fn try_connect_and_run(
    window: &slint::Weak<MainWindow>,
    action_rx: &mut mpsc::UnboundedReceiver<UserAction>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let conn = zbus::Connection::session().await?;
    let proxy = MixCtlProxy::new(&conn).await?;

    // Register with daemon
    proxy.register_component("ui").await.ok();

    // Mark connected
    let w = window.clone();
    slint::invoke_from_event_loop(move || {
        if let Some(win) = w.upgrade() {
            win.global::<MixerState>().set_daemon_connected(true);
        }
    }).ok();

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

    push_full_state(window, &outputs, &inputs, &routes, &streams, &playback_devices, selected_id, default_input_id);

    // Initial component check for beacn
    let components = proxy.list_components().await.unwrap_or_default();
    let beacn_connected = components.iter().any(|c| c.component_type == "beacn");
    {
        let w = window.clone();
        slint::invoke_from_event_loop(move || {
            if let Some(win) = w.upgrade() {
                win.global::<MixerState>().set_beacn_connected(beacn_connected);
            }
        }).ok();
    }

    // Track selected output on the tokio side
    let selected = Arc::new(std::sync::atomic::AtomicU32::new(selected_id));

    // Channel for disconnect sentinel
    let (disconnect_tx, mut disconnect_rx) = mpsc::unbounded_channel::<()>();

    // Spawn signal listeners, collecting handles
    let handles = spawn_signal_listeners(&proxy, window, &selected, disconnect_tx);

    // Process user actions until disconnect or channel close
    let inputs_ref = inputs;
    loop {
        tokio::select! {
            action = action_rx.recv() => {
                match action {
                    Some(action) => {
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
                                refresh_for_output(&proxy, window, &inputs_ref, output_id).await;
                            }
                            UserAction::SetOutputTarget { id, device_index } => {
                                let devices = proxy.list_playback_devices().await.unwrap_or_default();
                                let device_name = if device_index == 0 {
                                    String::new() // "None" -- unbind
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
                            UserAction::OpenDspDialog => {
                                open_dsp_dialog(&proxy).await;
                            }
                            UserAction::OpenChannelsDialog => {
                                open_channels_dialog(&proxy).await;
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
                // A signal stream ended -- daemon disconnected
                for h in handles {
                    h.abort();
                }
                return Err("signal stream ended (daemon disconnected)".into());
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

async fn open_channels_dialog(proxy: &MixCtlProxy<'static>) {
    let inputs = proxy.list_inputs().await.unwrap_or_default();
    let outputs = proxy.list_outputs().await.unwrap_or_default();

    let input_data: Vec<ChannelItemData> = inputs.iter().map(|i| ChannelItemData {
        id: i.id as i32,
        name: SharedString::from(i.name.as_str()),
        color: SharedString::from(i.color.as_str()),
    }).collect();
    let output_data: Vec<ChannelItemData> = outputs.iter().map(|o| ChannelItemData {
        id: o.id as i32,
        name: SharedString::from(o.name.as_str()),
        color: SharedString::from(o.color.as_str()),
    }).collect();

    let p = proxy.clone();
    slint::invoke_from_event_loop(move || {
        let dialog = ChannelsDialog::new().unwrap();
        dialog.set_inputs(ModelRc::new(VecModel::from(input_data)));
        dialog.set_outputs(ModelRc::new(VecModel::from(output_data)));

        // --- Add input ---
        let p2 = p.clone();
        let d_weak = dialog.as_weak();
        dialog.on_add_input(move |name, color| {
            let p3 = p2.clone();
            let d = d_weak.clone();
            let name = name.to_string();
            let color = color.to_string();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    p3.add_input(&name, &color).await.ok();
                    refresh_channels_dialog(&p3, &d).await;
                });
            });
        });

        // --- Remove input ---
        let p2 = p.clone();
        let d_weak = dialog.as_weak();
        dialog.on_remove_input(move |id| {
            let p3 = p2.clone();
            let d = d_weak.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    p3.remove_input(id as u32).await.ok();
                    refresh_channels_dialog(&p3, &d).await;
                });
            });
        });

        // --- Move input ---
        let p2 = p.clone();
        let d_weak = dialog.as_weak();
        dialog.on_move_input(move |id, position| {
            let p3 = p2.clone();
            let d = d_weak.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    p3.move_input(id as u32, position as u32).await.ok();
                    refresh_channels_dialog(&p3, &d).await;
                });
            });
        });

        // --- Rename input ---
        let p2 = p.clone();
        let d_weak = dialog.as_weak();
        dialog.on_rename_input(move |id, new_name| {
            let p3 = p2.clone();
            let d = d_weak.clone();
            let name = new_name.to_string();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    p3.set_input_name(id as u32, &name).await.ok();
                    refresh_channels_dialog(&p3, &d).await;
                });
            });
        });

        // --- Add output ---
        let p2 = p.clone();
        let d_weak = dialog.as_weak();
        dialog.on_add_output(move |name, color, source_output_id| {
            let p3 = p2.clone();
            let d = d_weak.clone();
            let name = name.to_string();
            let color = color.to_string();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    p3.add_output(&name, &color, source_output_id as u32).await.ok();
                    refresh_channels_dialog(&p3, &d).await;
                });
            });
        });

        // --- Remove output ---
        let p2 = p.clone();
        let d_weak = dialog.as_weak();
        dialog.on_remove_output(move |id| {
            let p3 = p2.clone();
            let d = d_weak.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    p3.remove_output(id as u32).await.ok();
                    refresh_channels_dialog(&p3, &d).await;
                });
            });
        });

        // --- Move output ---
        let p2 = p.clone();
        let d_weak = dialog.as_weak();
        dialog.on_move_output(move |id, position| {
            let p3 = p2.clone();
            let d = d_weak.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    p3.move_output(id as u32, position as u32).await.ok();
                    refresh_channels_dialog(&p3, &d).await;
                });
            });
        });

        // --- Rename output ---
        let p2 = p.clone();
        let d_weak = dialog.as_weak();
        dialog.on_rename_output(move |id, new_name| {
            let p3 = p2.clone();
            let d = d_weak.clone();
            let name = new_name.to_string();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    p3.set_output_name(id as u32, &name).await.ok();
                    refresh_channels_dialog(&p3, &d).await;
                });
            });
        });

        dialog.show().unwrap();
    }).ok();
}

async fn refresh_channels_dialog(
    proxy: &MixCtlProxy<'static>,
    d_weak: &slint::Weak<ChannelsDialog>,
) {
    let inputs = proxy.list_inputs().await.unwrap_or_default();
    let outputs = proxy.list_outputs().await.unwrap_or_default();
    let input_data: Vec<ChannelItemData> = inputs.iter().map(|i| ChannelItemData {
        id: i.id as i32,
        name: SharedString::from(i.name.as_str()),
        color: SharedString::from(i.color.as_str()),
    }).collect();
    let output_data: Vec<ChannelItemData> = outputs.iter().map(|o| ChannelItemData {
        id: o.id as i32,
        name: SharedString::from(o.name.as_str()),
        color: SharedString::from(o.color.as_str()),
    }).collect();
    let d = d_weak.clone();
    slint::invoke_from_event_loop(move || {
        if let Some(dlg) = d.upgrade() {
            dlg.set_inputs(ModelRc::new(VecModel::from(input_data)));
            dlg.set_outputs(ModelRc::new(VecModel::from(output_data)));
        }
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

async fn open_dsp_dialog(proxy: &MixCtlProxy<'static>) {
    let inputs = proxy.list_inputs().await.unwrap_or_default();
    let outputs = proxy.list_outputs().await.unwrap_or_default();

    let input_names: Vec<SharedString> = inputs.iter().map(|i| SharedString::from(i.name.as_str())).collect();
    let output_names: Vec<SharedString> = outputs.iter().map(|o| SharedString::from(o.name.as_str())).collect();

    // Fetch initial DSP state for first input/output
    let first_input_id = inputs.first().map(|i| i.id).unwrap_or(0);
    let first_output_id = outputs.first().map(|o| o.id).unwrap_or(0);

    let eq_enabled = if first_input_id > 0 {
        proxy.get_input_eq_enabled(first_input_id).await.unwrap_or(false)
    } else {
        false
    };
    let eq_bands_raw = if first_input_id > 0 {
        proxy.get_input_eq(first_input_id).await.unwrap_or_default()
    } else {
        Vec::new()
    };
    let gate_info = if first_input_id > 0 {
        proxy.get_input_gate(first_input_id).await.ok()
    } else {
        None
    };
    let deesser_info = if first_input_id > 0 {
        proxy.get_input_deesser(first_input_id).await.ok()
    } else {
        None
    };
    let compressor_info = if first_output_id > 0 {
        proxy.get_output_compressor(first_output_id).await.ok()
    } else {
        None
    };
    let limiter_info = if first_output_id > 0 {
        proxy.get_output_limiter(first_output_id).await.ok()
    } else {
        None
    };

    let input_ids: Vec<u32> = inputs.iter().map(|i| i.id).collect();
    let output_ids: Vec<u32> = outputs.iter().map(|o| o.id).collect();

    let p = proxy.clone();
    slint::invoke_from_event_loop(move || {
        let dialog = DspDialog::new().unwrap();
        dialog.set_input_names(ModelRc::new(VecModel::from(input_names)));
        dialog.set_output_names(ModelRc::new(VecModel::from(output_names)));

        // Set EQ state
        dialog.set_eq_enabled(eq_enabled);
        let eq_band_data: Vec<EqBandData> = eq_bands_raw.iter().map(|b| {
            let type_idx = match b.band_type.as_str() {
                "low_shelf" => 0,
                "peaking" => 1,
                "high_shelf" => 2,
                _ => 1,
            };
            EqBandData {
                band_type_index: type_idx,
                frequency: b.frequency as f32,
                gain_db: b.gain_db as f32,
                q: b.q as f32,
            }
        }).collect();
        // Ensure we always have 8 bands
        let mut bands = eq_band_data;
        while bands.len() < 8 {
            bands.push(EqBandData {
                band_type_index: 1,
                frequency: 1000.0,
                gain_db: 0.0,
                q: 1.0,
            });
        }
        dialog.set_eq_bands(ModelRc::new(VecModel::from(bands)));

        // Render initial EQ curve
        crate::eq_curve::update_eq_curve_image(&dialog);

        // Set Gate state
        if let Some(g) = &gate_info {
            dialog.set_gate(GateData {
                enabled: g.enabled,
                threshold_db: g.threshold_db as f32,
                attack_ms: g.attack_ms as f32,
                release_ms: g.release_ms as f32,
                hold_ms: g.hold_ms as f32,
            });
        } else {
            dialog.set_gate(GateData {
                enabled: false,
                threshold_db: -40.0,
                attack_ms: 1.0,
                release_ms: 100.0,
                hold_ms: 50.0,
            });
        }

        // Set De-esser state
        if let Some(d) = &deesser_info {
            dialog.set_deesser(DeesserData {
                enabled: d.enabled,
                frequency: d.frequency as f32,
                threshold_db: d.threshold_db as f32,
                ratio: d.ratio as f32,
            });
        } else {
            dialog.set_deesser(DeesserData {
                enabled: false,
                frequency: 6000.0,
                threshold_db: -20.0,
                ratio: 4.0,
            });
        }

        // Set Compressor state
        if let Some(c) = &compressor_info {
            dialog.set_compressor(CompressorData {
                enabled: c.enabled,
                threshold_db: c.threshold_db as f32,
                ratio: c.ratio as f32,
                attack_ms: c.attack_ms as f32,
                release_ms: c.release_ms as f32,
                makeup_gain_db: c.makeup_gain_db as f32,
                knee_db: c.knee_db as f32,
            });
        } else {
            dialog.set_compressor(CompressorData {
                enabled: false,
                threshold_db: -20.0,
                ratio: 4.0,
                attack_ms: 10.0,
                release_ms: 100.0,
                makeup_gain_db: 0.0,
                knee_db: 6.0,
            });
        }

        // Set Limiter state
        if let Some(l) = &limiter_info {
            dialog.set_limiter(LimiterData {
                enabled: l.enabled,
                ceiling_db: l.ceiling_db as f32,
                release_ms: l.release_ms as f32,
            });
        } else {
            dialog.set_limiter(LimiterData {
                enabled: false,
                ceiling_db: -1.0,
                release_ms: 50.0,
            });
        }

        // --- Wire input selection ---
        let p2 = p.clone();
        let input_ids2 = input_ids.clone();
        let d_weak = dialog.as_weak();
        dialog.on_input_selected(move |idx| {
            let input_id = input_ids2.get(idx as usize).copied().unwrap_or(0);
            if input_id == 0 { return; }
            let p3 = p2.clone();
            let d = d_weak.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    let eq_enabled = p3.get_input_eq_enabled(input_id).await.unwrap_or(false);
                    let eq_bands = p3.get_input_eq(input_id).await.unwrap_or_default();
                    let gate = p3.get_input_gate(input_id).await.ok();
                    let deesser = p3.get_input_deesser(input_id).await.ok();

                    slint::invoke_from_event_loop(move || {
                        if let Some(dlg) = d.upgrade() {
                            dlg.set_eq_enabled(eq_enabled);
                            let mut bands: Vec<EqBandData> = eq_bands.iter().map(|b| {
                                let type_idx = match b.band_type.as_str() {
                                    "low_shelf" => 0,
                                    "peaking" => 1,
                                    "high_shelf" => 2,
                                    _ => 1,
                                };
                                EqBandData {
                                    band_type_index: type_idx,
                                    frequency: b.frequency as f32,
                                    gain_db: b.gain_db as f32,
                                    q: b.q as f32,
                                }
                            }).collect();
                            while bands.len() < 8 {
                                bands.push(EqBandData {
                                    band_type_index: 1,
                                    frequency: 1000.0,
                                    gain_db: 0.0,
                                    q: 1.0,
                                });
                            }
                            dlg.set_eq_bands(ModelRc::new(VecModel::from(bands)));
                            crate::eq_curve::update_eq_curve_image(&dlg);

                            if let Some(g) = gate {
                                dlg.set_gate(GateData {
                                    enabled: g.enabled,
                                    threshold_db: g.threshold_db as f32,
                                    attack_ms: g.attack_ms as f32,
                                    release_ms: g.release_ms as f32,
                                    hold_ms: g.hold_ms as f32,
                                });
                            }
                            if let Some(ds) = deesser {
                                dlg.set_deesser(DeesserData {
                                    enabled: ds.enabled,
                                    frequency: ds.frequency as f32,
                                    threshold_db: ds.threshold_db as f32,
                                    ratio: ds.ratio as f32,
                                });
                            }
                        }
                    }).ok();
                });
            });
        });

        // --- Wire output selection ---
        let p2 = p.clone();
        let output_ids2 = output_ids.clone();
        let d_weak = dialog.as_weak();
        dialog.on_output_selected(move |idx| {
            let output_id = output_ids2.get(idx as usize).copied().unwrap_or(0);
            if output_id == 0 { return; }
            let p3 = p2.clone();
            let d = d_weak.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    let compressor = p3.get_output_compressor(output_id).await.ok();
                    let limiter = p3.get_output_limiter(output_id).await.ok();

                    slint::invoke_from_event_loop(move || {
                        if let Some(dlg) = d.upgrade() {
                            if let Some(c) = compressor {
                                dlg.set_compressor(CompressorData {
                                    enabled: c.enabled,
                                    threshold_db: c.threshold_db as f32,
                                    ratio: c.ratio as f32,
                                    attack_ms: c.attack_ms as f32,
                                    release_ms: c.release_ms as f32,
                                    makeup_gain_db: c.makeup_gain_db as f32,
                                    knee_db: c.knee_db as f32,
                                });
                            }
                            if let Some(l) = limiter {
                                dlg.set_limiter(LimiterData {
                                    enabled: l.enabled,
                                    ceiling_db: l.ceiling_db as f32,
                                    release_ms: l.release_ms as f32,
                                });
                            }
                        }
                    }).ok();
                });
            });
        });

        // --- Wire EQ callbacks ---
        let p2 = p.clone();
        let input_ids2 = input_ids.clone();
        let d_weak = dialog.as_weak();
        dialog.on_eq_enabled_changed(move |enabled| {
            let d = d_weak.clone();
            let idx = if let Some(dlg) = d.upgrade() { dlg.get_selected_input_index() } else { return; };
            let input_id = input_ids2.get(idx as usize).copied().unwrap_or(0);
            if input_id == 0 { return; }
            let p3 = p2.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    p3.set_input_eq_enabled(input_id, enabled).await.ok();
                });
            });
        });

        let p2 = p.clone();
        let input_ids2 = input_ids.clone();
        let d_weak = dialog.as_weak();
        dialog.on_eq_band_changed(move |band_idx, type_idx, freq, gain, q| {
            let d = d_weak.clone();
            let sel_idx = if let Some(dlg) = d.upgrade() { dlg.get_selected_input_index() } else { return; };
            let input_id = input_ids2.get(sel_idx as usize).copied().unwrap_or(0);
            if input_id == 0 { return; }
            let band_type = match type_idx {
                0 => "low_shelf",
                2 => "high_shelf",
                _ => "peaking",
            }.to_string();

            // Update the EQ curve image immediately
            if let Some(dlg) = d.upgrade() {
                let mut bands = crate::eq_curve::bands_from_dialog(&dlg);
                if let Some(b) = bands.get_mut(band_idx as usize) {
                    b.band_type = band_type.clone();
                    b.frequency = freq as f64;
                    b.gain_db = gain as f64;
                    b.q = q as f64;
                }
                let image = crate::eq_curve::render_eq_curve(&bands, 600, 200);
                dlg.set_eq_curve_image(image);
            }

            let p3 = p2.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    p3.set_input_eq_band(input_id, band_idx as u8, &band_type, freq as f64, gain as f64, q as f64).await.ok();
                });
            });
        });

        let p2 = p.clone();
        let input_ids2 = input_ids.clone();
        let d_weak = dialog.as_weak();
        dialog.on_reset_eq(move || {
            let d = d_weak.clone();
            let sel_idx = if let Some(dlg) = d.upgrade() { dlg.get_selected_input_index() } else { return; };
            let input_id = input_ids2.get(sel_idx as usize).copied().unwrap_or(0);
            if input_id == 0 { return; }
            let p3 = p2.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    p3.reset_input_eq(input_id).await.ok();
                    // Refresh EQ bands
                    let eq_enabled = p3.get_input_eq_enabled(input_id).await.unwrap_or(false);
                    let eq_bands = p3.get_input_eq(input_id).await.unwrap_or_default();
                    slint::invoke_from_event_loop(move || {
                        if let Some(dlg) = d.upgrade() {
                            dlg.set_eq_enabled(eq_enabled);
                            let mut bands: Vec<EqBandData> = eq_bands.iter().map(|b| {
                                let type_idx = match b.band_type.as_str() {
                                    "low_shelf" => 0,
                                    "peaking" => 1,
                                    "high_shelf" => 2,
                                    _ => 1,
                                };
                                EqBandData {
                                    band_type_index: type_idx,
                                    frequency: b.frequency as f32,
                                    gain_db: b.gain_db as f32,
                                    q: b.q as f32,
                                }
                            }).collect();
                            while bands.len() < 8 {
                                bands.push(EqBandData {
                                    band_type_index: 1,
                                    frequency: 1000.0,
                                    gain_db: 0.0,
                                    q: 1.0,
                                });
                            }
                            dlg.set_eq_bands(ModelRc::new(VecModel::from(bands)));
                            crate::eq_curve::update_eq_curve_image(&dlg);
                        }
                    }).ok();
                });
            });
        });

        // --- Wire Gate callbacks ---
        let p2 = p.clone();
        let input_ids2 = input_ids.clone();
        let d_weak = dialog.as_weak();
        dialog.on_gate_enabled_changed(move |enabled| {
            let d = d_weak.clone();
            let idx = if let Some(dlg) = d.upgrade() { dlg.get_selected_input_index() } else { return; };
            let input_id = input_ids2.get(idx as usize).copied().unwrap_or(0);
            if input_id == 0 { return; }
            let p3 = p2.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    p3.set_input_gate_enabled(input_id, enabled).await.ok();
                });
            });
        });

        let p2 = p.clone();
        let input_ids2 = input_ids.clone();
        let d_weak = dialog.as_weak();
        dialog.on_gate_changed(move |threshold, attack, release, hold| {
            let d = d_weak.clone();
            let idx = if let Some(dlg) = d.upgrade() { dlg.get_selected_input_index() } else { return; };
            let input_id = input_ids2.get(idx as usize).copied().unwrap_or(0);
            if input_id == 0 { return; }
            let p3 = p2.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    p3.set_input_gate(input_id, threshold as f64, attack as f64, release as f64, hold as f64).await.ok();
                });
            });
        });

        // --- Wire De-esser callbacks ---
        let p2 = p.clone();
        let input_ids2 = input_ids.clone();
        let d_weak = dialog.as_weak();
        dialog.on_deesser_enabled_changed(move |enabled| {
            let d = d_weak.clone();
            let idx = if let Some(dlg) = d.upgrade() { dlg.get_selected_input_index() } else { return; };
            let input_id = input_ids2.get(idx as usize).copied().unwrap_or(0);
            if input_id == 0 { return; }
            let p3 = p2.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    p3.set_input_deesser_enabled(input_id, enabled).await.ok();
                });
            });
        });

        let p2 = p.clone();
        let input_ids2 = input_ids.clone();
        let d_weak = dialog.as_weak();
        dialog.on_deesser_changed(move |freq, threshold, ratio| {
            let d = d_weak.clone();
            let idx = if let Some(dlg) = d.upgrade() { dlg.get_selected_input_index() } else { return; };
            let input_id = input_ids2.get(idx as usize).copied().unwrap_or(0);
            if input_id == 0 { return; }
            let p3 = p2.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    p3.set_input_deesser(input_id, freq as f64, threshold as f64, ratio as f64).await.ok();
                });
            });
        });

        // --- Wire Compressor callbacks ---
        let p2 = p.clone();
        let output_ids2 = output_ids.clone();
        let d_weak = dialog.as_weak();
        dialog.on_compressor_enabled_changed(move |enabled| {
            let d = d_weak.clone();
            let idx = if let Some(dlg) = d.upgrade() { dlg.get_selected_output_index() } else { return; };
            let output_id = output_ids2.get(idx as usize).copied().unwrap_or(0);
            if output_id == 0 { return; }
            let p3 = p2.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    p3.set_output_compressor_enabled(output_id, enabled).await.ok();
                });
            });
        });

        let p2 = p.clone();
        let output_ids2 = output_ids.clone();
        let d_weak = dialog.as_weak();
        dialog.on_compressor_changed(move |threshold, ratio, attack, release, makeup, knee| {
            let d = d_weak.clone();
            let idx = if let Some(dlg) = d.upgrade() { dlg.get_selected_output_index() } else { return; };
            let output_id = output_ids2.get(idx as usize).copied().unwrap_or(0);
            if output_id == 0 { return; }
            let p3 = p2.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    p3.set_output_compressor(output_id, threshold as f64, ratio as f64, attack as f64, release as f64, makeup as f64, knee as f64).await.ok();
                });
            });
        });

        // --- Wire Limiter callbacks ---
        let p2 = p.clone();
        let output_ids2 = output_ids.clone();
        let d_weak = dialog.as_weak();
        dialog.on_limiter_enabled_changed(move |enabled| {
            let d = d_weak.clone();
            let idx = if let Some(dlg) = d.upgrade() { dlg.get_selected_output_index() } else { return; };
            let output_id = output_ids2.get(idx as usize).copied().unwrap_or(0);
            if output_id == 0 { return; }
            let p3 = p2.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    p3.set_output_limiter_enabled(output_id, enabled).await.ok();
                });
            });
        });

        let p2 = p.clone();
        let output_ids2 = output_ids.clone();
        let d_weak = dialog.as_weak();
        dialog.on_limiter_changed(move |ceiling, release| {
            let d = d_weak.clone();
            let idx = if let Some(dlg) = d.upgrade() { dlg.get_selected_output_index() } else { return; };
            let output_id = output_ids2.get(idx as usize).copied().unwrap_or(0);
            if output_id == 0 { return; }
            let p3 = p2.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    p3.set_output_limiter(output_id, ceiling as f64, release as f64).await.ok();
                });
            });
        });

        dialog.show().unwrap();
    }).ok();
}

fn spawn_signal_listeners(
    proxy: &MixCtlProxy<'static>,
    window: &slint::Weak<MainWindow>,
    selected: &Arc<std::sync::atomic::AtomicU32>,
    disconnect_tx: mpsc::UnboundedSender<()>,
) -> Vec<JoinHandle<()>> {
    let mut handles = Vec::new();

    // Audio status changed
    let p = proxy.clone();
    let w = window.clone();
    let disc = disconnect_tx.clone();
    handles.push(tokio::spawn(async move {
        let Ok(mut stream) = p.receive_audio_status_changed().await else {
            disc.send(()).ok();
            return;
        };
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
        disc.send(()).ok();
    }));

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
        while let Some(_) = stream.next().await {
            do_full_refresh(&p, &w, &sel).await;
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
            do_full_refresh(&p, &w, &sel).await;
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
            do_full_refresh(&p, &w, &sel).await;
        }
        disc.send(()).ok();
    }));

    // Streams changed
    let p = proxy.clone();
    let w = window.clone();
    let disc = disconnect_tx.clone();
    handles.push(tokio::spawn(async move {
        let Ok(mut stream) = p.receive_streams_changed().await else {
            disc.send(()).ok();
            return;
        };
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
        disc.send(()).ok();
    }));

    // Playback devices changed
    let p = proxy.clone();
    let w = window.clone();
    let sel = selected.clone();
    let disc = disconnect_tx.clone();
    handles.push(tokio::spawn(async move {
        let Ok(mut stream) = p.receive_playback_devices_changed().await else {
            disc.send(()).ok();
            return;
        };
        while let Some(_) = stream.next().await {
            do_full_refresh(&p, &w, &sel).await;
        }
        disc.send(()).ok();
    }));

    // Component changed — update beacn-connected
    let p = proxy.clone();
    let w = window.clone();
    let disc = disconnect_tx.clone();
    handles.push(tokio::spawn(async move {
        let Ok(mut stream) = p.receive_component_changed().await else {
            disc.send(()).ok();
            return;
        };
        while let Some(_) = stream.next().await {
            let components = p.list_components().await.unwrap_or_default();
            let beacn_connected = components.iter().any(|c| c.component_type == "beacn");
            let w2 = w.clone();
            slint::invoke_from_event_loop(move || {
                if let Some(win) = w2.upgrade() {
                    win.global::<MixerState>().set_beacn_connected(beacn_connected);
                }
            }).ok();
        }
        disc.send(()).ok();
    }));

    handles
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
