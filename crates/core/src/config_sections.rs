use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BeacnConfig {
    #[serde(default = "default_layout")]
    pub layout: String,
    #[serde(default = "default_dial_sensitivity")]
    pub dial_sensitivity: u32,
    #[serde(default = "default_level_decay")]
    pub level_decay: f64,
}

impl Default for BeacnConfig {
    fn default() -> Self {
        Self {
            layout: default_layout(),
            dial_sensitivity: default_dial_sensitivity(),
            level_decay: default_level_decay(),
        }
    }
}

fn default_layout() -> String {
    "column".into()
}
fn default_dial_sensitivity() -> u32 {
    2
}
fn default_level_decay() -> f64 {
    0.8
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
