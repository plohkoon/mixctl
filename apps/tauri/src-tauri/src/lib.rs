mod commands;
mod error;
mod signals;
mod state;
mod tray;

use error::Error;
use state::AppState;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

use tokio::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.set_focus();
            }
        }))
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            // mixer
            commands::mixer::get_full_state,
            commands::mixer::list_inputs,
            commands::mixer::list_outputs,
            commands::mixer::get_default_input,
            commands::mixer::set_default_input,
            commands::mixer::get_default_output,
            commands::mixer::set_default_output,
            commands::mixer::select_output,
            commands::mixer::list_routes_for_output,
            commands::mixer::set_route_volume,
            commands::mixer::set_route_mute,
            commands::mixer::set_output_volume,
            commands::mixer::set_output_mute,
            commands::mixer::set_output_target,
            commands::mixer::list_streams,
            commands::mixer::assign_stream,
            commands::mixer::list_playback_devices,
            commands::mixer::get_audio_status,
            // channels
            commands::channels::add_input,
            commands::channels::remove_input,
            commands::channels::move_input,
            commands::channels::set_input_name,
            commands::channels::set_input_color,
            commands::channels::add_output,
            commands::channels::remove_output,
            commands::channels::move_output,
            commands::channels::set_output_name,
            commands::channels::set_output_color,
            // dsp
            commands::dsp::get_input_dsp,
            commands::dsp::get_output_dsp,
            commands::dsp::set_input_eq_enabled,
            commands::dsp::set_input_eq_band,
            commands::dsp::get_input_eq,
            commands::dsp::reset_input_eq,
            commands::dsp::set_input_gate_enabled,
            commands::dsp::set_input_gate,
            commands::dsp::set_input_deesser_enabled,
            commands::dsp::set_input_deesser,
            commands::dsp::set_output_compressor_enabled,
            commands::dsp::set_output_compressor,
            commands::dsp::set_output_limiter_enabled,
            commands::dsp::set_output_limiter,
            commands::dsp::compute_eq_curve,
            // system
            commands::system::list_app_rules,
            commands::system::set_app_rule,
            commands::system::remove_app_rule,
            commands::system::list_capture_devices,
            commands::system::add_capture_input,
            commands::system::get_beacn_config,
            commands::system::set_beacn_config,
            commands::system::list_components,
            commands::system::register_component,
            commands::system::bind_capture_to_input,
            commands::system::remove_capture_input,
            commands::system::set_capture_volume,
            commands::system::set_capture_mute,
            // profiles
            commands::system::list_profiles,
            commands::system::save_profile,
            commands::system::load_profile,
            commands::system::delete_profile,
            // dialogs
            open_dialog,
            open_channel_editor,
        ])
        .setup(|app| {
            // Manage state synchronously — starts disconnected
            app.manage(Mutex::new(AppState::new()));

            // Connect to D-Bus daemon in background
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match connect_to_daemon().await {
                    Ok(proxy) => {
                        {
                            let state = app_handle.state::<Mutex<AppState>>();
                            let mut s = state.lock().await;
                            s.proxy = Some(proxy);
                        }
                        // Spawn signal listeners
                        signals::spawn_signal_listeners(app_handle.clone());
                        // Register as UI component
                        let state = app_handle.state::<Mutex<AppState>>();
                        let s = state.lock().await;
                        if let Ok(proxy) = s.proxy() {
                            proxy.register_component("ui").await.ok();
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to connect to daemon: {e}");
                    }
                }
            });

            // Ensure applet starts hidden
            if let Some(applet) = app.get_webview_window("applet") {
                let _ = applet.hide();
            }

            // Set up ksni system tray (bypasses libappindicator for proper left-click)
            let app_handle = app.handle().clone();
            setup_ksni_tray(app_handle);

            Ok(())
        })
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            eprintln!("Tauri application error: {e:?}");
            std::process::exit(1);
        });
}

async fn connect_to_daemon(
) -> Result<mixctl_core::dbus::MixCtlProxy<'static>, Box<dyn std::error::Error + Send + Sync>> {
    let conn = zbus::Connection::session().await?;
    let proxy = mixctl_core::dbus::MixCtlProxy::new(&conn).await?;
    proxy.ping().await?;
    Ok(proxy)
}

fn setup_ksni_tray(app_handle: tauri::AppHandle) {
    let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel::<tray::TrayAction>();

    // Spawn ksni tray service (async in ksni 0.3)
    tauri::async_runtime::spawn(async move {
        let tray = tray::MixCtlTray {
            action_tx: std::sync::Mutex::new(action_tx),
        };
        match tray::spawn_tray(tray).await {
            Ok(_handle) => {
                // Keep handle alive — dropping it removes the tray icon
                // Block forever (the action_rx loop below handles messages)
                std::future::pending::<()>().await;
            }
            Err(e) => {
                log::error!("Failed to spawn ksni tray: {e}");
            }
        }
    });

    // Process tray actions on a separate task
    let handle = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(action) = action_rx.recv().await {
            match action {
                tray::TrayAction::ToggleApplet { x, y } => {
                    if let Some(window) = handle.get_webview_window("applet") {
                        if window.is_visible().unwrap_or(false) {
                            let _ = window.hide();
                        } else {
                            let w = 360;
                            let h = 600;
                            let monitor_height = window.current_monitor()
                                .ok().flatten()
                                .map(|m| m.size().height as i32)
                                .unwrap_or(1080);
                            let pos_x = (x - w / 2).max(0);
                            let pos_y = if y < monitor_height / 2 {
                                (y + 36).max(40) // Below top panel
                            } else {
                                (y - h - 8).max(0) // Above bottom panel
                            };

                            // GTK WMs ignore set_position on unmapped windows.
                            // Workaround: show off-screen first so the window is "mapped",
                            // then reposition — WMs honor moves on already-shown windows.
                            let _ = window.set_position(tauri::LogicalPosition::new(pos_x, -10000));
                            let _ = window.show();
                            let _ = window.set_position(tauri::LogicalPosition::new(pos_x, pos_y));
                            let _ = window.set_focus();
                        }
                    }
                }
                tray::TrayAction::ShowMixer => {
                    if let Some(win) = handle.get_webview_window("main") {
                        let _ = win.show();
                        let _ = win.set_focus();
                    } else if let Ok(win) = tauri::WebviewWindowBuilder::new(&handle, "main", tauri::WebviewUrl::App("index.html".into()))
                        .title("MixCtl")
                        .inner_size(750.0, 450.0)
                        .build()
                    {
                        let _ = win.set_focus();
                    }
                }
                tray::TrayAction::Quit => {
                    handle.exit(0);
                }
            }
        }
    });
}

#[tauri::command]
async fn open_channel_editor(
    app: tauri::AppHandle,
    mode: String,
    id: Option<u32>,
) -> Result<(), Error> {
    let label = match id {
        Some(i) => format!("channel-editor-{mode}-{i}"),
        None => format!("channel-editor-new-{mode}"),
    };
    let url = match id {
        Some(i) => format!("/dialogs/channel-editor?mode={mode}&id={i}"),
        None => format!("/dialogs/channel-editor?mode={mode}"),
    };
    let title = match id {
        Some(_) => "Edit Channel",
        None => "New Channel",
    };

    if let Some(win) = app.get_webview_window(&label) {
        win.set_focus()?;
    } else {
        WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(url.into()))
            .title(title)
            .inner_size(340.0, 400.0)
            .resizable(false)
            .build()?;
    }
    Ok(())
}

#[tauri::command]
async fn open_dialog(app: tauri::AppHandle, dialog: String) -> Result<(), Error> {
    // TODO: Read dialog dimensions from UiConfig via proxy.get_config_section("ui")
    // For now, using sensible defaults that can be overridden later
    let (label, title, width, height, url) = match dialog.as_str() {
        "dsp" => ("dsp-dialog", "DSP", 900.0, 600.0, "/dialogs/dsp"),
        "beacn" => ("beacn-dialog", "Beacn", 520.0, 700.0, "/dialogs/beacn"),
        _ => return Err(Error::InvalidParam(format!("unknown dialog: {dialog}"))),
    };

    if let Some(win) = app.get_webview_window(label) {
        win.set_focus()?;
    } else {
        WebviewWindowBuilder::new(&app, label, WebviewUrl::App(url.into()))
            .title(title)
            .inner_size(width, height)
            .resizable(false)
            .build()?;
    }

    Ok(())
}
