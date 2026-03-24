use crate::error::Error;
use crate::state::AppState;
use mixctl_core::{AppRuleInfo, CaptureDeviceInfo, ComponentInfo};
use tauri::State;
use tokio::sync::Mutex;

#[tauri::command]
pub async fn list_app_rules(
    state: State<'_, Mutex<AppState>>,
) -> Result<Vec<AppRuleInfo>, Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.list_app_rules().await?)
}

#[tauri::command]
pub async fn set_app_rule(
    state: State<'_, Mutex<AppState>>,
    app_name: String,
    input_id: u32,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.set_app_rule(&app_name, input_id).await?)
}

#[tauri::command]
pub async fn remove_app_rule(
    state: State<'_, Mutex<AppState>>,
    app_name: String,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.remove_app_rule(&app_name).await?)
}

#[tauri::command]
pub async fn list_capture_devices(
    state: State<'_, Mutex<AppState>>,
) -> Result<Vec<CaptureDeviceInfo>, Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.list_capture_devices().await?)
}

#[tauri::command]
pub async fn add_capture_input(
    state: State<'_, Mutex<AppState>>,
    pw_node_id: u32,
    name: String,
    color: String,
) -> Result<u32, Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.add_capture_input(pw_node_id, &name, &color).await?)
}

#[tauri::command]
pub async fn get_beacn_config(
    state: State<'_, Mutex<AppState>>,
) -> Result<String, Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.get_config_section("beacn").await?)
}

#[tauri::command]
pub async fn set_beacn_config(
    state: State<'_, Mutex<AppState>>,
    layout: String,
    dial_sensitivity: u32,
    level_decay: f64,
) -> Result<(), Error> {
    let s = state.lock().await;
    let config = serde_json::json!({
        "layout": layout,
        "dial_sensitivity": dial_sensitivity,
        "level_decay": level_decay,
    });
    Ok(s.proxy()?
        .set_config_section("beacn", &config.to_string())
        .await?)
}

#[tauri::command]
pub async fn list_components(
    state: State<'_, Mutex<AppState>>,
) -> Result<Vec<ComponentInfo>, Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.list_components().await?)
}

#[tauri::command]
pub async fn register_component(
    state: State<'_, Mutex<AppState>>,
    component_type: String,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.register_component(&component_type).await?)
}

#[tauri::command]
pub async fn bind_capture_to_input(
    state: State<'_, Mutex<AppState>>,
    input_id: u32,
    device_name: String,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.bind_capture_to_input(input_id, &device_name).await?)
}

#[tauri::command]
pub async fn remove_capture_input(
    state: State<'_, Mutex<AppState>>,
    id: u32,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.remove_capture_input(id).await?)
}

#[tauri::command]
pub async fn set_capture_volume(
    state: State<'_, Mutex<AppState>>,
    id: u32,
    volume: f32,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.set_capture_volume(id, volume).await?)
}

#[tauri::command]
pub async fn set_capture_mute(
    state: State<'_, Mutex<AppState>>,
    id: u32,
    muted: bool,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.set_capture_mute(id, muted).await?)
}

// -- Profiles --

#[tauri::command]
pub async fn list_profiles(
    state: State<'_, Mutex<AppState>>,
) -> Result<Vec<String>, Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.list_profiles().await?)
}

#[tauri::command]
pub async fn save_profile(
    state: State<'_, Mutex<AppState>>,
    name: String,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.save_profile(&name).await?)
}

#[tauri::command]
pub async fn load_profile(
    state: State<'_, Mutex<AppState>>,
    name: String,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.load_profile(&name).await?)
}

#[tauri::command]
pub async fn delete_profile(
    state: State<'_, Mutex<AppState>>,
    name: String,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.delete_profile(&name).await?)
}
