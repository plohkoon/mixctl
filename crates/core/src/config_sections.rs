use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BeacnConfig {
    #[serde(default = "default_layout")]
    pub layout: String,
    #[serde(default = "default_dial_sensitivity")]
    pub dial_sensitivity: u32,
    #[serde(default = "default_level_decay")]
    pub level_decay: f64,
    #[serde(default = "default_display_brightness")]
    pub display_brightness: u8,
    #[serde(default = "default_led_brightness")]
    pub led_brightness: u8,
    #[serde(default)]
    pub button_mappings: ButtonMappings,
}

impl Default for BeacnConfig {
    fn default() -> Self {
        Self {
            layout: default_layout(),
            dial_sensitivity: default_dial_sensitivity(),
            level_decay: default_level_decay(),
            display_brightness: default_display_brightness(),
            led_brightness: default_led_brightness(),
            button_mappings: ButtonMappings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ButtonMappings {
    #[serde(default = "default_toggle_route_mute")]
    pub dial1_press: ButtonAction,
    #[serde(default = "default_toggle_route_mute")]
    pub dial2_press: ButtonAction,
    #[serde(default = "default_toggle_route_mute")]
    pub dial3_press: ButtonAction,
    #[serde(default = "default_toggle_route_mute")]
    pub dial4_press: ButtonAction,
    #[serde(default = "default_toggle_global_mute")]
    pub audience1: ButtonAction,
    #[serde(default = "default_toggle_global_mute")]
    pub audience2: ButtonAction,
    #[serde(default = "default_toggle_global_mute")]
    pub audience3: ButtonAction,
    #[serde(default = "default_toggle_global_mute")]
    pub audience4: ButtonAction,
    #[serde(default = "default_next_output")]
    pub mix: ButtonAction,
    #[serde(default = "default_page_left")]
    pub page_left: ButtonAction,
    #[serde(default = "default_page_right")]
    pub page_right: ButtonAction,
}

impl Default for ButtonMappings {
    fn default() -> Self {
        Self {
            dial1_press: ButtonAction::ToggleRouteMute,
            dial2_press: ButtonAction::ToggleRouteMute,
            dial3_press: ButtonAction::ToggleRouteMute,
            dial4_press: ButtonAction::ToggleRouteMute,
            audience1: ButtonAction::ToggleGlobalMute,
            audience2: ButtonAction::ToggleGlobalMute,
            audience3: ButtonAction::ToggleGlobalMute,
            audience4: ButtonAction::ToggleGlobalMute,
            mix: ButtonAction::NextOutput,
            page_left: ButtonAction::PageLeft,
            page_right: ButtonAction::PageRight,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ButtonAction {
    ToggleRouteMute,
    ToggleGlobalMute,
    NextOutput,
    PrevOutput,
    PageLeft,
    PageRight,
    None,
}

fn default_toggle_route_mute() -> ButtonAction { ButtonAction::ToggleRouteMute }
fn default_toggle_global_mute() -> ButtonAction { ButtonAction::ToggleGlobalMute }
fn default_next_output() -> ButtonAction { ButtonAction::NextOutput }
fn default_page_left() -> ButtonAction { ButtonAction::PageLeft }
fn default_page_right() -> ButtonAction { ButtonAction::PageRight }

fn default_layout() -> String {
    "column".into()
}
fn default_dial_sensitivity() -> u32 {
    2
}
fn default_level_decay() -> f64 {
    0.8
}
fn default_display_brightness() -> u8 {
    40
}
fn default_led_brightness() -> u8 {
    255
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UiConfig {
    #[serde(default = "default_window_width")]
    pub window_width: i32,
    #[serde(default = "default_window_height")]
    pub window_height: i32,
    #[serde(default = "default_margin")]
    pub margin: i32,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            window_width: default_window_width(),
            window_height: default_window_height(),
            margin: default_margin(),
        }
    }
}

fn default_window_width() -> i32 {
    750
}
fn default_window_height() -> i32 {
    450
}
fn default_margin() -> i32 {
    12
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppletConfig {
    #[serde(default = "default_applet_window_width")]
    pub window_width: i32,
    #[serde(default = "default_poll_interval_ms")]
    pub poll_interval_ms: u64,
    #[serde(default = "default_open_ui_command")]
    pub open_ui_command: String,
}

impl Default for AppletConfig {
    fn default() -> Self {
        Self {
            window_width: default_applet_window_width(),
            poll_interval_ms: default_poll_interval_ms(),
            open_ui_command: default_open_ui_command(),
        }
    }
}

fn default_applet_window_width() -> i32 {
    380
}
fn default_poll_interval_ms() -> u64 {
    30
}
fn default_open_ui_command() -> String {
    "mixctl-ui".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CliConfig {
    #[serde(default = "default_color_output")]
    pub color_output: bool,
    #[serde(default = "default_output_format")]
    pub output_format: String,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            color_output: default_color_output(),
            output_format: default_output_format(),
        }
    }
}

fn default_color_output() -> bool {
    true
}
fn default_output_format() -> String {
    "text".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TuiConfig {
    #[serde(default = "default_volume_step")]
    pub volume_step: u8,
    #[serde(default = "default_volume_fine_step")]
    pub volume_fine_step: u8,
    #[serde(default = "default_initial_panel")]
    pub initial_panel: String,
    #[serde(default = "default_ping_interval_secs")]
    pub ping_interval_secs: u64,
    #[serde(default = "default_reconnect_delay_secs")]
    pub reconnect_delay_secs: u64,
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            volume_step: default_volume_step(),
            volume_fine_step: default_volume_fine_step(),
            initial_panel: default_initial_panel(),
            ping_interval_secs: default_ping_interval_secs(),
            reconnect_delay_secs: default_reconnect_delay_secs(),
        }
    }
}

fn default_volume_step() -> u8 {
    5
}
fn default_volume_fine_step() -> u8 {
    1
}
fn default_initial_panel() -> String {
    "routes".into()
}
fn default_ping_interval_secs() -> u64 {
    3
}
fn default_reconnect_delay_secs() -> u64 {
    2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beacn_config_defaults() {
        let config = BeacnConfig::default();
        assert_eq!(config.layout, "column");
        assert_eq!(config.dial_sensitivity, 2);
        assert_eq!(config.level_decay, 0.8);
        assert_eq!(config.display_brightness, 40);
        assert_eq!(config.led_brightness, 255);
    }

    #[test]
    fn button_mappings_default() {
        let m = ButtonMappings::default();
        assert_eq!(m.dial1_press, ButtonAction::ToggleRouteMute);
        assert_eq!(m.dial2_press, ButtonAction::ToggleRouteMute);
        assert_eq!(m.dial3_press, ButtonAction::ToggleRouteMute);
        assert_eq!(m.dial4_press, ButtonAction::ToggleRouteMute);
        assert_eq!(m.audience1, ButtonAction::ToggleGlobalMute);
        assert_eq!(m.audience2, ButtonAction::ToggleGlobalMute);
        assert_eq!(m.audience3, ButtonAction::ToggleGlobalMute);
        assert_eq!(m.audience4, ButtonAction::ToggleGlobalMute);
        assert_eq!(m.mix, ButtonAction::NextOutput);
        assert_eq!(m.page_left, ButtonAction::PageLeft);
        assert_eq!(m.page_right, ButtonAction::PageRight);
    }

    #[test]
    fn button_action_serde_roundtrip() {
        let action = ButtonAction::ToggleRouteMute;
        let json = serde_json::to_string(&action).unwrap();
        let deserialized: ButtonAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, deserialized);

        // Test other variants
        for variant in &[
            ButtonAction::ToggleGlobalMute,
            ButtonAction::NextOutput,
            ButtonAction::PrevOutput,
            ButtonAction::PageLeft,
            ButtonAction::PageRight,
            ButtonAction::None,
        ] {
            let json = serde_json::to_string(variant).unwrap();
            let deserialized: ButtonAction = serde_json::from_str(&json).unwrap();
            assert_eq!(*variant, deserialized);
        }
    }

    #[test]
    fn beacn_config_serde_roundtrip() {
        let config = BeacnConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: BeacnConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, deserialized);
    }

    #[test]
    fn partial_beacn_config_deserialize() {
        let json = r#"{"layout": "row"}"#;
        let config: BeacnConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.layout, "row");
        // Missing fields get defaults
        assert_eq!(config.dial_sensitivity, 2);
        assert_eq!(config.level_decay, 0.8);
        assert_eq!(config.display_brightness, 40);
        assert_eq!(config.led_brightness, 255);
        assert_eq!(config.button_mappings, ButtonMappings::default());
    }
}
