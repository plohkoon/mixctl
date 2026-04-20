use crate::error::Error;
use crate::state::AppState;
use mixctl_core::{
    InputInfo, OutputInfo, PlaybackDeviceInfo, RouteInfo, StreamInfo,
};
use serde::Serialize;
use std::sync::atomic::Ordering;
use tauri::State;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FullState {
    pub inputs: Vec<InputInfo>,
    pub outputs: Vec<OutputInfo>,
    pub routes: Vec<RouteInfo>,
    pub streams: Vec<StreamInfo>,
    pub playback_devices: Vec<PlaybackDeviceInfo>,
    pub selected_output_id: u32,
    pub default_input_id: u32,
    pub default_output_id: u32,
    pub audio_connected: bool,
    pub beacn_connected: bool,
}

#[tauri::command]
pub async fn get_full_state(state: State<'_, Mutex<AppState>>) -> Result<FullState, Error> {
    let s = state.lock().await;
    let proxy = s.proxy()?;

    let inputs = proxy.list_inputs().await.unwrap_or_default();
    let outputs = proxy.list_outputs().await.unwrap_or_default();
    let streams = proxy.list_streams().await.unwrap_or_default();
    let playback_devices = proxy.list_playback_devices().await.unwrap_or_default();
    let default_input_id = proxy.get_default_input().await.unwrap_or(0);
    let default_output_id = proxy.get_default_output().await.unwrap_or(0);
    let audio_status = proxy.get_audio_status().await.unwrap_or_default();
    let components = proxy.list_components().await.unwrap_or_default();

    let mut selected_id = s.selected_output_id.load(Ordering::Relaxed);
    if !outputs.iter().any(|o| o.id == selected_id) {
        selected_id = outputs.first().map(|o| o.id).unwrap_or(0);
        s.selected_output_id.store(selected_id, Ordering::Relaxed);
    }

    // Fetch routes for ALL outputs (matrix view needs the complete routing state)
    let mut routes = Vec::new();
    for output in &outputs {
        if let Ok(output_routes) = proxy.list_routes_for_output(output.id).await {
            routes.extend(output_routes);
        }
    }

    Ok(FullState {
        inputs,
        outputs,
        routes,
        streams,
        playback_devices,
        selected_output_id: selected_id,
        default_input_id,
        default_output_id,
        audio_connected: audio_status == "connected",
        beacn_connected: components.iter().any(|c| c.component_type.starts_with("beacn")),
    })
}

#[tauri::command]
pub async fn list_inputs(state: State<'_, Mutex<AppState>>) -> Result<Vec<InputInfo>, Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.list_inputs().await?)
}

#[tauri::command]
pub async fn list_outputs(state: State<'_, Mutex<AppState>>) -> Result<Vec<OutputInfo>, Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.list_outputs().await?)
}

#[tauri::command]
pub async fn get_default_input(state: State<'_, Mutex<AppState>>) -> Result<u32, Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.get_default_input().await?)
}

#[tauri::command]
pub async fn get_default_output(state: State<'_, Mutex<AppState>>) -> Result<u32, Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.get_default_output().await?)
}

#[tauri::command]
pub async fn set_default_output(state: State<'_, Mutex<AppState>>, id: u32) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.set_default_output(id).await?)
}

#[tauri::command]
pub async fn set_default_input(state: State<'_, Mutex<AppState>>, id: u32) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.set_default_input(id).await?)
}

#[tauri::command]
pub async fn select_output(
    state: State<'_, Mutex<AppState>>,
    output_id: u32,
) -> Result<Vec<RouteInfo>, Error> {
    let s = state.lock().await;
    s.selected_output_id.store(output_id, Ordering::Relaxed);
    Ok(s.proxy()?.list_routes_for_output(output_id).await?)
}

#[tauri::command]
pub async fn list_routes_for_output(
    state: State<'_, Mutex<AppState>>,
    output_id: u32,
) -> Result<Vec<RouteInfo>, Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.list_routes_for_output(output_id).await?)
}

#[tauri::command]
pub async fn set_route_volume(
    state: State<'_, Mutex<AppState>>,
    input_id: u32,
    output_id: u32,
    volume: u8,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.set_route_volume(input_id, output_id, volume).await?)
}

#[tauri::command]
pub async fn set_route_mute(
    state: State<'_, Mutex<AppState>>,
    input_id: u32,
    output_id: u32,
    muted: bool,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.set_route_mute(input_id, output_id, muted).await?)
}

#[tauri::command]
pub async fn set_output_volume(
    state: State<'_, Mutex<AppState>>,
    id: u32,
    volume: u8,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.set_output_volume(id, volume).await?)
}

#[tauri::command]
pub async fn set_output_mute(
    state: State<'_, Mutex<AppState>>,
    id: u32,
    muted: bool,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.set_output_mute(id, muted).await?)
}

#[tauri::command]
pub async fn set_output_target(
    state: State<'_, Mutex<AppState>>,
    id: u32,
    device_name: String,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.set_output_target(id, &device_name).await?)
}

#[tauri::command]
pub async fn list_streams(state: State<'_, Mutex<AppState>>) -> Result<Vec<StreamInfo>, Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.list_streams().await?)
}

#[tauri::command]
pub async fn assign_stream(
    state: State<'_, Mutex<AppState>>,
    pw_node_id: u32,
    input_id: u32,
    remember: bool,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.assign_stream(pw_node_id, input_id, remember).await?)
}

#[tauri::command]
pub async fn list_playback_devices(
    state: State<'_, Mutex<AppState>>,
) -> Result<Vec<PlaybackDeviceInfo>, Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.list_playback_devices().await?)
}

#[tauri::command]
pub async fn get_audio_status(state: State<'_, Mutex<AppState>>) -> Result<String, Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.get_audio_status().await?)
}
