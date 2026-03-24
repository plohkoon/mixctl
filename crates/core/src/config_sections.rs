use serde::{Deserialize, Deserializer, Serialize};

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
    #[serde(default = "default_hold_threshold_ms")]
    pub hold_threshold_ms: u64,
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
            hold_threshold_ms: default_hold_threshold_ms(),
            button_mappings: ButtonMappings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ButtonAction {
    ToggleRouteMute,
    ToggleGlobalMute,
    MuteOutput { output_id: u32 },
    MuteAllOutputs,
    ToggleEq,
    ToggleGate,
    ToggleDeesser,
    ToggleCompressor,
    ToggleLimiter,
    LoadProfile { name: String },
    PushToMute,
    PushToTalk,
    NextOutput,
    PrevOutput,
    PageLeft,
    PageRight,
    None,
}

impl ButtonAction {
    pub const ALL_SIMPLE: &[ButtonAction] = &[
        ButtonAction::ToggleRouteMute,
        ButtonAction::ToggleGlobalMute,
        ButtonAction::MuteAllOutputs,
        ButtonAction::ToggleEq,
        ButtonAction::ToggleGate,
        ButtonAction::ToggleDeesser,
        ButtonAction::ToggleCompressor,
        ButtonAction::ToggleLimiter,
        ButtonAction::PushToMute,
        ButtonAction::PushToTalk,
        ButtonAction::NextOutput,
        ButtonAction::PrevOutput,
        ButtonAction::PageLeft,
        ButtonAction::PageRight,
        ButtonAction::None,
    ];

    pub fn display_name(&self) -> String {
        match self {
            Self::ToggleRouteMute => "Mute Route".into(),
            Self::ToggleGlobalMute => "Mute Global".into(),
            Self::MuteOutput { output_id } => format!("Mute Output {output_id}"),
            Self::MuteAllOutputs => "Mute All Outputs".into(),
            Self::ToggleEq => "Toggle EQ".into(),
            Self::ToggleGate => "Toggle Gate".into(),
            Self::ToggleDeesser => "Toggle De-esser".into(),
            Self::ToggleCompressor => "Toggle Compressor".into(),
            Self::ToggleLimiter => "Toggle Limiter".into(),
            Self::LoadProfile { name } => format!("Load Profile: {name}"),
            Self::PushToMute => "Push to Mute".into(),
            Self::PushToTalk => "Push to Talk".into(),
            Self::NextOutput => "Next Output".into(),
            Self::PrevOutput => "Prev Output".into(),
            Self::PageLeft => "Page Left".into(),
            Self::PageRight => "Page Right".into(),
            Self::None => "None".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ButtonMapping {
    pub press: ButtonAction,
    pub hold: ButtonAction,
}

/// Custom deserializer: accepts either a plain string (old format → press only)
/// or a struct with press/hold fields (new format).
impl<'de> Deserialize<'de> for ButtonMapping {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            /// New format: { press: ..., hold: ... }
            Struct { #[serde(default = "default_action_none")] press: ButtonAction, #[serde(default = "default_action_none")] hold: ButtonAction },
            /// Old format: plain action string like "toggle_route_mute"
            Legacy(ButtonAction),
        }
        match Raw::deserialize(deserializer)? {
            Raw::Struct { press, hold } => Ok(ButtonMapping { press, hold }),
            Raw::Legacy(action) => Ok(ButtonMapping::press_only(action)),
        }
    }
}

impl ButtonMapping {
    pub fn press_only(action: ButtonAction) -> Self {
        Self { press: action, hold: ButtonAction::None }
    }
}

fn default_action_none() -> ButtonAction { ButtonAction::None }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ButtonMappings {
    #[serde(default = "default_dial", alias = "dial1_press")]
    pub dial1: ButtonMapping,
    #[serde(default = "default_dial", alias = "dial2_press")]
    pub dial2: ButtonMapping,
    #[serde(default = "default_dial", alias = "dial3_press")]
    pub dial3: ButtonMapping,
    #[serde(default = "default_dial", alias = "dial4_press")]
    pub dial4: ButtonMapping,
    #[serde(default = "default_audience")]
    pub audience1: ButtonMapping,
    #[serde(default = "default_audience")]
    pub audience2: ButtonMapping,
    #[serde(default = "default_audience")]
    pub audience3: ButtonMapping,
    #[serde(default = "default_audience")]
    pub audience4: ButtonMapping,
    #[serde(default = "default_mix")]
    pub mix: ButtonMapping,
    #[serde(default = "default_page_left_mapping")]
    pub page_left: ButtonMapping,
    #[serde(default = "default_page_right_mapping")]
    pub page_right: ButtonMapping,
}

impl Default for ButtonMappings {
    fn default() -> Self {
        Self {
            dial1: default_dial(),
            dial2: default_dial(),
            dial3: default_dial(),
            dial4: default_dial(),
            audience1: default_audience(),
            audience2: default_audience(),
            audience3: default_audience(),
            audience4: default_audience(),
            mix: default_mix(),
            page_left: default_page_left_mapping(),
            page_right: default_page_right_mapping(),
        }
    }
}

impl ButtonMappings {
    pub const BUTTON_NAMES: &[&str] = &[
        "dial1", "dial2", "dial3", "dial4",
        "audience1", "audience2", "audience3", "audience4",
        "mix", "page_left", "page_right",
    ];

    pub fn get(&self, name: &str) -> Option<&ButtonMapping> {
        match name {
            "dial1" => Some(&self.dial1),
            "dial2" => Some(&self.dial2),
            "dial3" => Some(&self.dial3),
            "dial4" => Some(&self.dial4),
            "audience1" => Some(&self.audience1),
            "audience2" => Some(&self.audience2),
            "audience3" => Some(&self.audience3),
            "audience4" => Some(&self.audience4),
            "mix" => Some(&self.mix),
            "page_left" | "page-left" => Some(&self.page_left),
            "page_right" | "page-right" => Some(&self.page_right),
            _ => Option::None,
        }
    }

    pub fn set(&mut self, name: &str, mapping: ButtonMapping) -> bool {
        match name {
            "dial1" => self.dial1 = mapping,
            "dial2" => self.dial2 = mapping,
            "dial3" => self.dial3 = mapping,
            "dial4" => self.dial4 = mapping,
            "audience1" => self.audience1 = mapping,
            "audience2" => self.audience2 = mapping,
            "audience3" => self.audience3 = mapping,
            "audience4" => self.audience4 = mapping,
            "mix" => self.mix = mapping,
            "page_left" | "page-left" => self.page_left = mapping,
            "page_right" | "page-right" => self.page_right = mapping,
            _ => return false,
        }
        true
    }
}

fn default_dial() -> ButtonMapping { ButtonMapping::press_only(ButtonAction::ToggleRouteMute) }
fn default_audience() -> ButtonMapping { ButtonMapping::press_only(ButtonAction::ToggleGlobalMute) }
fn default_mix() -> ButtonMapping {
    ButtonMapping { press: ButtonAction::NextOutput, hold: ButtonAction::PrevOutput }
}
fn default_page_left_mapping() -> ButtonMapping { ButtonMapping::press_only(ButtonAction::PageLeft) }
fn default_page_right_mapping() -> ButtonMapping { ButtonMapping::press_only(ButtonAction::PageRight) }

fn default_layout() -> String { "column".into() }
fn default_dial_sensitivity() -> u32 { 2 }
fn default_level_decay() -> f64 { 0.8 }
fn default_display_brightness() -> u8 { 40 }
fn default_led_brightness() -> u8 { 255 }
fn default_hold_threshold_ms() -> u64 { 200 }

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
}

impl Default for AppletConfig {
    fn default() -> Self {
        Self {
            window_width: default_applet_window_width(),
            poll_interval_ms: default_poll_interval_ms(),
        }
    }
}

fn default_applet_window_width() -> i32 {
    380
}
fn default_poll_interval_ms() -> u64 {
    30
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
        assert_eq!(config.hold_threshold_ms, 200);
    }

    #[test]
    fn button_mappings_default() {
        let m = ButtonMappings::default();
        assert_eq!(m.dial1.press, ButtonAction::ToggleRouteMute);
        assert_eq!(m.dial1.hold, ButtonAction::None);
        assert_eq!(m.audience1.press, ButtonAction::ToggleGlobalMute);
        assert_eq!(m.audience1.hold, ButtonAction::None);
        assert_eq!(m.mix.press, ButtonAction::NextOutput);
        assert_eq!(m.mix.hold, ButtonAction::PrevOutput);
        assert_eq!(m.page_left.press, ButtonAction::PageLeft);
        assert_eq!(m.page_right.press, ButtonAction::PageRight);
    }

    #[test]
    fn button_action_serde_roundtrip() {
        let simple_variants = vec![
            ButtonAction::ToggleRouteMute,
            ButtonAction::ToggleGlobalMute,
            ButtonAction::MuteAllOutputs,
            ButtonAction::ToggleEq,
            ButtonAction::ToggleGate,
            ButtonAction::ToggleDeesser,
            ButtonAction::ToggleCompressor,
            ButtonAction::ToggleLimiter,
            ButtonAction::PushToMute,
            ButtonAction::PushToTalk,
            ButtonAction::NextOutput,
            ButtonAction::PrevOutput,
            ButtonAction::PageLeft,
            ButtonAction::PageRight,
            ButtonAction::None,
        ];
        for variant in &simple_variants {
            let json = serde_json::to_string(variant).unwrap();
            let deserialized: ButtonAction = serde_json::from_str(&json).unwrap();
            assert_eq!(*variant, deserialized);
        }

        // Parameterized variants
        let mute_output = ButtonAction::MuteOutput { output_id: 5 };
        let json = serde_json::to_string(&mute_output).unwrap();
        let deserialized: ButtonAction = serde_json::from_str(&json).unwrap();
        assert_eq!(mute_output, deserialized);

        let load_profile = ButtonAction::LoadProfile { name: "gaming".into() };
        let json = serde_json::to_string(&load_profile).unwrap();
        let deserialized: ButtonAction = serde_json::from_str(&json).unwrap();
        assert_eq!(load_profile, deserialized);
    }

    #[test]
    fn button_mapping_serde_roundtrip() {
        let mapping = ButtonMapping {
            press: ButtonAction::ToggleRouteMute,
            hold: ButtonAction::ToggleGlobalMute,
        };
        let json = serde_json::to_string(&mapping).unwrap();
        let deserialized: ButtonMapping = serde_json::from_str(&json).unwrap();
        assert_eq!(mapping, deserialized);
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
        assert_eq!(config.dial_sensitivity, 2);
        assert_eq!(config.hold_threshold_ms, 200);
        assert_eq!(config.button_mappings, ButtonMappings::default());
    }

    #[test]
    fn button_mappings_get_set() {
        let mut m = ButtonMappings::default();
        assert_eq!(m.get("dial1").unwrap().press, ButtonAction::ToggleRouteMute);
        m.set("dial1", ButtonMapping { press: ButtonAction::ToggleEq, hold: ButtonAction::PushToMute });
        assert_eq!(m.dial1.press, ButtonAction::ToggleEq);
        assert_eq!(m.dial1.hold, ButtonAction::PushToMute);
    }

    #[test]
    fn legacy_button_mapping_string_format() {
        // Old config format: plain string instead of { press, hold } struct
        let json = r#""toggle_route_mute""#;
        let mapping: ButtonMapping = serde_json::from_str(json).unwrap();
        assert_eq!(mapping.press, ButtonAction::ToggleRouteMute);
        assert_eq!(mapping.hold, ButtonAction::None);
    }

    #[test]
    fn legacy_button_mappings_with_old_field_names() {
        // Old config used dial1_press, dial2_press, etc.
        let json = r#"{"dial1_press": "toggle_global_mute", "audience1": "toggle_route_mute"}"#;
        let mappings: ButtonMappings = serde_json::from_str(json).unwrap();
        assert_eq!(mappings.dial1.press, ButtonAction::ToggleGlobalMute);
        assert_eq!(mappings.dial1.hold, ButtonAction::None);
        assert_eq!(mappings.audience1.press, ButtonAction::ToggleRouteMute);
    }

    #[test]
    fn legacy_beacn_config_full_migration() {
        // Simulate an old config file with flat button mappings
        let json = r#"{
            "layout": "column",
            "dial_sensitivity": 2,
            "level_decay": 0.8,
            "button_mappings": {
                "dial1_press": "toggle_route_mute",
                "dial2_press": "toggle_route_mute",
                "dial3_press": "toggle_route_mute",
                "dial4_press": "toggle_route_mute",
                "audience1": "toggle_global_mute",
                "audience2": "toggle_global_mute",
                "audience3": "toggle_global_mute",
                "audience4": "toggle_global_mute",
                "mix": "next_output",
                "page_left": "page_left",
                "page_right": "page_right"
            }
        }"#;
        let config: BeacnConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.dial_sensitivity, 2);
        assert_eq!(config.hold_threshold_ms, 200); // new field defaults
        assert_eq!(config.button_mappings.dial1.press, ButtonAction::ToggleRouteMute);
        assert_eq!(config.button_mappings.dial1.hold, ButtonAction::None);
        assert_eq!(config.button_mappings.audience1.press, ButtonAction::ToggleGlobalMute);
        assert_eq!(config.button_mappings.mix.press, ButtonAction::NextOutput);
    }
}
