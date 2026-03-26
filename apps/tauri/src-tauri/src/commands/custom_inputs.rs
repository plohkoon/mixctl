use crate::error::Error;
use crate::state::AppState;
use mixctl_core::CustomInputInfo;
use tauri::State;
use tokio::sync::Mutex;

#[tauri::command]
pub async fn list_custom_inputs(
    state: State<'_, Mutex<AppState>>,
) -> Result<Vec<CustomInputInfo>, Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.list_custom_inputs().await?)
}

#[tauri::command]
pub async fn add_custom_input(
    state: State<'_, Mutex<AppState>>,
    name: String,
    color: String,
    custom_type: String,
    params_json: String,
) -> Result<u32, Error> {
    let s = state.lock().await;
    Ok(s.proxy()?
        .add_custom_input(&name, &color, &custom_type, &params_json)
        .await?)
}

#[tauri::command]
pub async fn remove_custom_input(
    state: State<'_, Mutex<AppState>>,
    id: u32,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.remove_custom_input(id).await?)
}

#[tauri::command]
pub async fn set_custom_input_value(
    state: State<'_, Mutex<AppState>>,
    id: u32,
    value: u8,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.set_custom_input_value(id, value).await?)
}
