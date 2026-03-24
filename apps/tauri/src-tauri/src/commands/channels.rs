use crate::error::Error;
use crate::state::AppState;
use tauri::State;
use tokio::sync::Mutex;

#[tauri::command]
pub async fn add_input(
    state: State<'_, Mutex<AppState>>,
    name: String,
    color: String,
) -> Result<u32, Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.add_input(&name, &color).await?)
}

#[tauri::command]
pub async fn remove_input(state: State<'_, Mutex<AppState>>, id: u32) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.remove_input(id).await?)
}

#[tauri::command]
pub async fn move_input(
    state: State<'_, Mutex<AppState>>,
    id: u32,
    position: u32,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.move_input(id, position).await?)
}

#[tauri::command]
pub async fn set_input_name(
    state: State<'_, Mutex<AppState>>,
    id: u32,
    name: String,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.set_input_name(id, &name).await?)
}

#[tauri::command]
pub async fn set_input_color(
    state: State<'_, Mutex<AppState>>,
    id: u32,
    color: String,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.set_input_color(id, &color).await?)
}

#[tauri::command]
pub async fn add_output(
    state: State<'_, Mutex<AppState>>,
    name: String,
    color: String,
    source_output_id: u32,
) -> Result<u32, Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.add_output(&name, &color, source_output_id).await?)
}

#[tauri::command]
pub async fn remove_output(state: State<'_, Mutex<AppState>>, id: u32) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.remove_output(id).await?)
}

#[tauri::command]
pub async fn move_output(
    state: State<'_, Mutex<AppState>>,
    id: u32,
    position: u32,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.move_output(id, position).await?)
}

#[tauri::command]
pub async fn set_output_name(
    state: State<'_, Mutex<AppState>>,
    id: u32,
    name: String,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.set_output_name(id, &name).await?)
}

#[tauri::command]
pub async fn set_output_color(
    state: State<'_, Mutex<AppState>>,
    id: u32,
    color: String,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.set_output_color(id, &color).await?)
}
