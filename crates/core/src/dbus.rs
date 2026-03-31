use crate::{
    AppRuleInfo, CaptureDeviceInfo, CompressorInfo, ComponentInfo, CustomInputInfo,
    DeesserInfo, DeviceInfo, EqBandInfo, GateInfo, InputInfo, LimiterInfo, OutputInfo,
    PlaybackDeviceInfo, RouteInfo, StreamInfo,
};

pub const BUS_NAME: &str = "dev.greghuber.MixCtl";
pub const OBJ_PATH: &str = "/dev/greghuber/MixCtl1";

#[zbus::proxy(
    interface = "dev.greghuber.MixCtl1",
    default_service = "dev.greghuber.MixCtl",
    default_path = "/dev/greghuber/MixCtl1"
)]
pub trait MixCtl {
    fn ping(&self) -> zbus::Result<String>;

    // Audio status
    fn get_audio_status(&self) -> zbus::Result<String>;

    // Default input/output
    fn get_default_input(&self) -> zbus::Result<u32>;
    fn set_default_input(&self, id: u32) -> zbus::Result<()>;
    fn get_default_output(&self) -> zbus::Result<u32>;
    fn set_default_output(&self, id: u32) -> zbus::Result<()>;

    // Inputs (config-only — no volume/mute)
    fn list_inputs(&self) -> zbus::Result<Vec<InputInfo>>;
    fn get_input(&self, id: u32) -> zbus::Result<InputInfo>;
    fn add_input(&self, name: &str, color: &str) -> zbus::Result<u32>;
    fn remove_input(&self, id: u32) -> zbus::Result<()>;
    fn move_input(&self, id: u32, position: u32) -> zbus::Result<()>;
    fn set_input_name(&self, id: u32, name: &str) -> zbus::Result<()>;
    fn set_input_color(&self, id: u32, color: &str) -> zbus::Result<()>;

    // Outputs (have master volume + mute)
    fn list_outputs(&self) -> zbus::Result<Vec<OutputInfo>>;
    fn get_output(&self, id: u32) -> zbus::Result<OutputInfo>;
    fn add_output(&self, name: &str, color: &str, source_output_id: u32) -> zbus::Result<u32>;
    fn remove_output(&self, id: u32) -> zbus::Result<()>;
    fn move_output(&self, id: u32, position: u32) -> zbus::Result<()>;
    fn set_output_name(&self, id: u32, name: &str) -> zbus::Result<()>;
    fn set_output_color(&self, id: u32, color: &str) -> zbus::Result<()>;
    fn set_output_volume(&self, id: u32, volume: u8) -> zbus::Result<()>;
    fn set_output_mute(&self, id: u32, muted: bool) -> zbus::Result<()>;
    fn set_output_target(&self, id: u32, device_name: &str) -> zbus::Result<()>;

    // Routing (per input→output cell)
    fn get_route(&self, input_id: u32, output_id: u32) -> zbus::Result<RouteInfo>;
    fn list_routes_for_output(&self, output_id: u32) -> zbus::Result<Vec<RouteInfo>>;
    fn set_route_volume(&self, input_id: u32, output_id: u32, volume: u8) -> zbus::Result<()>;
    fn set_route_mute(&self, input_id: u32, output_id: u32, muted: bool) -> zbus::Result<()>;

    // Streams (Phase 3)
    fn list_streams(&self) -> zbus::Result<Vec<StreamInfo>>;
    fn assign_stream(&self, pw_node_id: u32, input_id: u32, remember: bool) -> zbus::Result<()>;

    // App Rules (Phase 3)
    fn list_app_rules(&self) -> zbus::Result<Vec<AppRuleInfo>>;
    fn set_app_rule(&self, app_name: &str, input_id: u32) -> zbus::Result<()>;
    fn remove_app_rule(&self, app_name: &str) -> zbus::Result<()>;

    // Capture Devices (Phase 4)
    fn list_capture_devices(&self) -> zbus::Result<Vec<CaptureDeviceInfo>>;
    fn add_capture_input(&self, pw_node_id: u32, name: &str, color: &str) -> zbus::Result<u32>;
    fn bind_capture_to_input(&self, input_id: u32, device_name: &str) -> zbus::Result<()>;
    fn remove_capture_input(&self, id: u32) -> zbus::Result<()>;
    fn set_capture_volume(&self, id: u32, volume: f32) -> zbus::Result<()>;
    fn set_capture_mute(&self, id: u32, muted: bool) -> zbus::Result<()>;

    // Playback Devices
    fn list_playback_devices(&self) -> zbus::Result<Vec<PlaybackDeviceInfo>>;

    // Config sections
    fn get_config_section(&self, section: &str) -> zbus::Result<String>;
    fn set_config_section(&self, section: &str, json: &str) -> zbus::Result<()>;

    // Level monitoring
    fn get_broadcast_levels(&self) -> zbus::Result<bool>;
    fn set_broadcast_levels(&self, enabled: bool) -> zbus::Result<()>;
    fn get_input_levels(&self) -> zbus::Result<Vec<(u32, f64)>>;

    // Component tracking
    fn register_component(&self, component_type: &str) -> zbus::Result<()>;
    fn list_components(&self) -> zbus::Result<Vec<ComponentInfo>>;

    // Device adapter registration
    fn register_device(&self, device_name: &str, capabilities_json: &str) -> zbus::Result<()>;
    fn list_devices(&self) -> zbus::Result<Vec<DeviceInfo>>;

    // DSP: EQ (per input, 8 bands)
    fn set_input_eq_enabled(&self, input_id: u32, enabled: bool) -> zbus::Result<()>;
    fn get_input_eq_enabled(&self, input_id: u32) -> zbus::Result<bool>;
    fn set_input_eq_band(&self, input_id: u32, band: u8, band_type: &str, freq: f64, gain_db: f64, q: f64) -> zbus::Result<()>;
    fn get_input_eq(&self, input_id: u32) -> zbus::Result<Vec<EqBandInfo>>;
    fn reset_input_eq(&self, input_id: u32) -> zbus::Result<()>;

    // DSP: Gate (per input)
    fn set_input_gate_enabled(&self, input_id: u32, enabled: bool) -> zbus::Result<()>;
    fn set_input_gate(&self, input_id: u32, threshold_db: f64, attack_ms: f64, release_ms: f64, hold_ms: f64) -> zbus::Result<()>;
    fn get_input_gate(&self, input_id: u32) -> zbus::Result<GateInfo>;

    // DSP: De-esser (per input)
    fn set_input_deesser_enabled(&self, input_id: u32, enabled: bool) -> zbus::Result<()>;
    fn set_input_deesser(&self, input_id: u32, frequency: f64, threshold_db: f64, ratio: f64) -> zbus::Result<()>;
    fn get_input_deesser(&self, input_id: u32) -> zbus::Result<DeesserInfo>;

    // DSP: Compressor (per output)
    fn set_output_compressor_enabled(&self, output_id: u32, enabled: bool) -> zbus::Result<()>;
    fn set_output_compressor(&self, output_id: u32, threshold_db: f64, ratio: f64, attack_ms: f64, release_ms: f64, makeup_gain_db: f64, knee_db: f64) -> zbus::Result<()>;
    fn get_output_compressor(&self, output_id: u32) -> zbus::Result<CompressorInfo>;

    // DSP: Limiter (per output)
    fn set_output_limiter_enabled(&self, output_id: u32, enabled: bool) -> zbus::Result<()>;
    fn set_output_limiter(&self, output_id: u32, ceiling_db: f64, release_ms: f64) -> zbus::Result<()>;
    fn get_output_limiter(&self, output_id: u32) -> zbus::Result<LimiterInfo>;

    // DSP: Noise suppression (per capture input)
    fn set_capture_noise_suppression(&self, input_id: u32, enabled: bool) -> zbus::Result<()>;
    fn get_capture_noise_suppression(&self, input_id: u32) -> zbus::Result<bool>;

    // Custom inputs (non-audio dial controls)
    fn list_custom_inputs(&self) -> zbus::Result<Vec<CustomInputInfo>>;
    fn add_custom_input(&self, name: &str, color: &str, custom_type: &str, params_json: &str) -> zbus::Result<u32>;
    fn remove_custom_input(&self, id: u32) -> zbus::Result<()>;
    fn get_custom_input_value(&self, id: u32) -> zbus::Result<u8>;
    fn set_custom_input_value(&self, id: u32, value: u8) -> zbus::Result<()>;

    // Profiles
    fn list_profiles(&self) -> zbus::Result<Vec<String>>;
    fn save_profile(&self, name: &str) -> zbus::Result<()>;
    fn load_profile(&self, name: &str) -> zbus::Result<()>;
    fn delete_profile(&self, name: &str) -> zbus::Result<()>;

    // Signals
    #[zbus(signal)]
    fn inputs_config_changed(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn outputs_config_changed(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn output_state_changed(&self, id: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    fn route_changed(&self, input_id: u32, output_id: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    fn audio_status_changed(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn streams_changed(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn app_rules_changed(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn capture_devices_changed(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn playback_devices_changed(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn input_levels_changed(&self, levels: Vec<(u32, f64)>) -> zbus::Result<()>;

    #[zbus(signal)]
    fn broadcast_levels_changed(&self, enabled: bool) -> zbus::Result<()>;

    #[zbus(signal)]
    fn config_section_changed(&self, section: String) -> zbus::Result<()>;

    #[zbus(signal)]
    fn component_changed(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn input_dsp_changed(&self, input_id: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    fn output_dsp_changed(&self, output_id: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    fn profile_changed(&self, name: String) -> zbus::Result<()>;

    #[zbus(signal)]
    fn custom_input_changed(&self, id: u32) -> zbus::Result<()>;
}
