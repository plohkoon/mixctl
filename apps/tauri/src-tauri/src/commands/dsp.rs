use crate::error::Error;
use crate::state::AppState;
use mixctl_core::{CompressorInfo, DeesserInfo, EqBandInfo, GateInfo, LimiterInfo};
use serde::Serialize;
use tauri::State;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputDspState {
    pub eq_enabled: bool,
    pub eq_bands: Vec<EqBandInfo>,
    pub gate: GateInfo,
    pub deesser: DeesserInfo,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputDspState {
    pub compressor: CompressorInfo,
    pub limiter: LimiterInfo,
}

#[tauri::command]
pub async fn get_input_dsp(
    state: State<'_, Mutex<AppState>>,
    input_id: u32,
) -> Result<InputDspState, Error> {
    let s = state.lock().await;
    let proxy = s.proxy()?;

    let eq_enabled = proxy.get_input_eq_enabled(input_id).await.unwrap_or(false);
    let eq_bands = proxy.get_input_eq(input_id).await.unwrap_or_default();
    let gate = proxy.get_input_gate(input_id).await.unwrap_or(GateInfo {
        enabled: false,
        threshold_db: -40.0,
        attack_ms: 1.0,
        release_ms: 50.0,
        hold_ms: 5.0,
    });
    let deesser = proxy.get_input_deesser(input_id).await.unwrap_or(DeesserInfo {
        enabled: false,
        frequency: 6000.0,
        threshold_db: -20.0,
        ratio: 4.0,
    });

    Ok(InputDspState {
        eq_enabled,
        eq_bands,
        gate,
        deesser,
    })
}

#[tauri::command]
pub async fn get_output_dsp(
    state: State<'_, Mutex<AppState>>,
    output_id: u32,
) -> Result<OutputDspState, Error> {
    let s = state.lock().await;
    let proxy = s.proxy()?;

    let compressor = proxy
        .get_output_compressor(output_id)
        .await
        .unwrap_or(CompressorInfo {
            enabled: false,
            threshold_db: -18.0,
            ratio: 4.0,
            attack_ms: 10.0,
            release_ms: 100.0,
            makeup_gain_db: 0.0,
            knee_db: 0.0,
        });
    let limiter = proxy
        .get_output_limiter(output_id)
        .await
        .unwrap_or(LimiterInfo {
            enabled: false,
            ceiling_db: -0.5,
            release_ms: 50.0,
        });

    Ok(OutputDspState {
        compressor,
        limiter,
    })
}

#[tauri::command]
pub async fn set_input_eq_enabled(
    state: State<'_, Mutex<AppState>>,
    input_id: u32,
    enabled: bool,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.set_input_eq_enabled(input_id, enabled).await?)
}

#[tauri::command]
pub async fn set_input_eq_band(
    state: State<'_, Mutex<AppState>>,
    input_id: u32,
    band: u8,
    band_type: String,
    freq: f64,
    gain_db: f64,
    q: f64,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?
        .set_input_eq_band(input_id, band, &band_type, freq, gain_db, q)
        .await?)
}

#[tauri::command]
pub async fn get_input_eq(
    state: State<'_, Mutex<AppState>>,
    input_id: u32,
) -> Result<Vec<EqBandInfo>, Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.get_input_eq(input_id).await?)
}

#[tauri::command]
pub async fn reset_input_eq(
    state: State<'_, Mutex<AppState>>,
    input_id: u32,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.reset_input_eq(input_id).await?)
}

#[tauri::command]
pub async fn set_input_gate_enabled(
    state: State<'_, Mutex<AppState>>,
    input_id: u32,
    enabled: bool,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?.set_input_gate_enabled(input_id, enabled).await?)
}

#[tauri::command]
pub async fn set_input_gate(
    state: State<'_, Mutex<AppState>>,
    input_id: u32,
    threshold_db: f64,
    attack_ms: f64,
    release_ms: f64,
    hold_ms: f64,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?
        .set_input_gate(input_id, threshold_db, attack_ms, release_ms, hold_ms)
        .await?)
}

#[tauri::command]
pub async fn set_input_deesser_enabled(
    state: State<'_, Mutex<AppState>>,
    input_id: u32,
    enabled: bool,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?
        .set_input_deesser_enabled(input_id, enabled)
        .await?)
}

#[tauri::command]
pub async fn set_input_deesser(
    state: State<'_, Mutex<AppState>>,
    input_id: u32,
    frequency: f64,
    threshold_db: f64,
    ratio: f64,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?
        .set_input_deesser(input_id, frequency, threshold_db, ratio)
        .await?)
}

#[tauri::command]
pub async fn set_output_compressor_enabled(
    state: State<'_, Mutex<AppState>>,
    output_id: u32,
    enabled: bool,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?
        .set_output_compressor_enabled(output_id, enabled)
        .await?)
}

#[tauri::command]
pub async fn set_output_compressor(
    state: State<'_, Mutex<AppState>>,
    output_id: u32,
    threshold_db: f64,
    ratio: f64,
    attack_ms: f64,
    release_ms: f64,
    makeup_gain_db: f64,
    knee_db: f64,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?
        .set_output_compressor(
            output_id,
            threshold_db,
            ratio,
            attack_ms,
            release_ms,
            makeup_gain_db,
            knee_db,
        )
        .await?)
}

#[tauri::command]
pub async fn set_output_limiter_enabled(
    state: State<'_, Mutex<AppState>>,
    output_id: u32,
    enabled: bool,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?
        .set_output_limiter_enabled(output_id, enabled)
        .await?)
}

#[tauri::command]
pub async fn set_output_limiter(
    state: State<'_, Mutex<AppState>>,
    output_id: u32,
    ceiling_db: f64,
    release_ms: f64,
) -> Result<(), Error> {
    let s = state.lock().await;
    Ok(s.proxy()?
        .set_output_limiter(output_id, ceiling_db, release_ms)
        .await?)
}

#[tauri::command]
pub async fn compute_eq_curve(bands: Vec<EqBandInfo>) -> Result<Vec<(f64, f64)>, Error> {
    Ok(mixctl_core::compute_eq_curve(&bands))
}
