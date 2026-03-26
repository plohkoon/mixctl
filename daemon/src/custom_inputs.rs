use anyhow::{Result, bail};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn};

/// Trait for custom input handlers that map a 0-100 value to some external control.
pub trait CustomInputHandler: Send + Sync {
    /// Apply the given value (0-100) to the underlying control.
    fn apply(&self, value: u8) -> Result<()>;

    /// Read the current value (0-100) from the underlying control.
    fn read_current(&self) -> Result<u8>;

    /// Whether this handler supports reading back a current value.
    fn supports_read(&self) -> bool {
        true
    }

    /// If this is a DspParameterHandler, return its details.
    fn as_dsp_parameter(&self) -> Option<(u32, &str, f64, f64)> {
        None
    }
}

/// Create a handler for the given type name and parameters.
pub fn create_handler(
    custom_type: &str,
    params: &HashMap<String, toml::Value>,
) -> Result<Box<dyn CustomInputHandler>> {
    match custom_type {
        "display_brightness" => Ok(Box::new(DisplayBrightnessHandler::new()?)),
        "keyboard_brightness" => Ok(Box::new(KeyboardBrightnessHandler::new()?)),
        "dsp_parameter" => Ok(Box::new(DspParameterHandler::new(params)?)),
        "command" => Ok(Box::new(CommandHandler::new(params)?)),
        "http" => Ok(Box::new(HttpHandler::new(params)?)),
        "dbus" => Ok(Box::new(DbusHandler::new(params)?)),
        _ => bail!("unknown custom input type '{}'", custom_type),
    }
}

// ---------------------------------------------------------------------------
// Display brightness handler
// ---------------------------------------------------------------------------

pub struct DisplayBrightnessHandler {
    brightness_path: PathBuf,
    max_brightness: u32,
}

impl DisplayBrightnessHandler {
    fn new() -> Result<Self> {
        let backlight_dir = PathBuf::from("/sys/class/backlight");
        let entry = fs::read_dir(&backlight_dir)?
            .filter_map(|e| e.ok())
            .next()
            .ok_or_else(|| anyhow::anyhow!("no backlight device found in /sys/class/backlight"))?;
        let base = entry.path();
        let max_str = fs::read_to_string(base.join("max_brightness"))?.trim().to_string();
        let max_brightness: u32 = max_str.parse()?;
        if max_brightness == 0 {
            bail!("max_brightness is 0");
        }
        info!("display brightness handler: {:?}, max={}", base, max_brightness);
        Ok(Self {
            brightness_path: base.join("brightness"),
            max_brightness,
        })
    }

    fn scale_to_device(&self, value: u8) -> u32 {
        ((value as u32) * self.max_brightness + 50) / 100
    }

    fn scale_from_device(&self, device_val: u32) -> u8 {
        ((device_val * 100 + self.max_brightness / 2) / self.max_brightness).min(100) as u8
    }
}

impl CustomInputHandler for DisplayBrightnessHandler {
    fn apply(&self, value: u8) -> Result<()> {
        let device_val = self.scale_to_device(value);
        fs::write(&self.brightness_path, device_val.to_string())?;
        Ok(())
    }

    fn read_current(&self) -> Result<u8> {
        let raw = fs::read_to_string(&self.brightness_path)?.trim().to_string();
        let device_val: u32 = raw.parse()?;
        Ok(self.scale_from_device(device_val))
    }
}

// ---------------------------------------------------------------------------
// Keyboard brightness handler
// ---------------------------------------------------------------------------

pub struct KeyboardBrightnessHandler {
    brightness_path: PathBuf,
    max_brightness: u32,
}

impl KeyboardBrightnessHandler {
    fn new() -> Result<Self> {
        let leds_dir = PathBuf::from("/sys/class/leds");
        let entry = fs::read_dir(&leds_dir)?
            .filter_map(|e| e.ok())
            .find(|e| {
                e.file_name()
                    .to_str()
                    .map(|n| n.contains("kbd_backlight"))
                    .unwrap_or(false)
            })
            .ok_or_else(|| {
                anyhow::anyhow!("no keyboard backlight found in /sys/class/leds/*kbd_backlight*")
            })?;
        let base = entry.path();
        let max_str = fs::read_to_string(base.join("max_brightness"))?.trim().to_string();
        let max_brightness: u32 = max_str.parse()?;
        if max_brightness == 0 {
            bail!("max_brightness is 0");
        }
        info!("keyboard brightness handler: {:?}, max={}", base, max_brightness);
        Ok(Self {
            brightness_path: base.join("brightness"),
            max_brightness,
        })
    }

    fn scale_to_device(&self, value: u8) -> u32 {
        ((value as u32) * self.max_brightness + 50) / 100
    }

    fn scale_from_device(&self, device_val: u32) -> u8 {
        ((device_val * 100 + self.max_brightness / 2) / self.max_brightness).min(100) as u8
    }
}

impl CustomInputHandler for KeyboardBrightnessHandler {
    fn apply(&self, value: u8) -> Result<()> {
        let device_val = self.scale_to_device(value);
        fs::write(&self.brightness_path, device_val.to_string())?;
        Ok(())
    }

    fn read_current(&self) -> Result<u8> {
        let raw = fs::read_to_string(&self.brightness_path)?.trim().to_string();
        let device_val: u32 = raw.parse()?;
        Ok(self.scale_from_device(device_val))
    }
}

// ---------------------------------------------------------------------------
// DSP parameter handler (special case: handled by D-Bus adapter)
// ---------------------------------------------------------------------------

pub struct DspParameterHandler {
    channel_id: u32,
    param: String,
    min_val: f64,
    max_val: f64,
}

impl DspParameterHandler {
    fn new(params: &HashMap<String, toml::Value>) -> Result<Self> {
        let channel_id = params
            .get("channel_id")
            .and_then(|v| v.as_integer())
            .ok_or_else(|| anyhow::anyhow!("dsp_parameter requires 'channel_id' (integer)"))?
            as u32;
        let param = params
            .get("param")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("dsp_parameter requires 'param' (string)"))?
            .to_string();
        let min_val = params
            .get("min")
            .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
            .unwrap_or(0.0);
        let max_val = params
            .get("max")
            .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
            .unwrap_or(100.0);
        Ok(Self {
            channel_id,
            param,
            min_val,
            max_val,
        })
    }

    #[allow(dead_code)]
    pub fn channel_id(&self) -> u32 {
        self.channel_id
    }

    #[allow(dead_code)]
    pub fn param_name(&self) -> &str {
        &self.param
    }

    /// Map a 0-100 value to the parameter's actual range.
    #[allow(dead_code)]
    pub fn map_value(&self, v: u8) -> f64 {
        let t = v as f64 / 100.0;
        self.min_val + t * (self.max_val - self.min_val)
    }
}

impl CustomInputHandler for DspParameterHandler {
    fn apply(&self, _value: u8) -> Result<()> {
        // Actual DSP application is handled by the D-Bus adapter which has
        // access to Shared and can issue PW commands directly.
        Ok(())
    }

    fn read_current(&self) -> Result<u8> {
        bail!("dsp_parameter does not support reading current value")
    }

    fn supports_read(&self) -> bool {
        false
    }

    fn as_dsp_parameter(&self) -> Option<(u32, &str, f64, f64)> {
        Some((self.channel_id, &self.param, self.min_val, self.max_val))
    }
}

// ---------------------------------------------------------------------------
// Command handler
// ---------------------------------------------------------------------------

pub struct CommandHandler {
    command: String,
    read_command: Option<String>,
}

impl CommandHandler {
    fn new(params: &HashMap<String, toml::Value>) -> Result<Self> {
        let command = params
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("command handler requires 'command' (string)"))?
            .to_string();
        let read_command = params
            .get("read_command")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        Ok(Self {
            command,
            read_command,
        })
    }
}

impl CustomInputHandler for CommandHandler {
    fn apply(&self, value: u8) -> Result<()> {
        let cmd = self.command.replace("{value}", &value.to_string());
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(&cmd)
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("command handler: command failed: {}", stderr.trim());
        }
        Ok(())
    }

    fn read_current(&self) -> Result<u8> {
        match &self.read_command {
            Some(cmd) => {
                let output = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(cmd)
                    .output()?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    bail!("read_command failed: {}", stderr.trim());
                }
                let stdout = String::from_utf8_lossy(&output.stdout);
                let val: u8 = stdout
                    .trim()
                    .parse()
                    .map_err(|e| anyhow::anyhow!("failed to parse read_command output as u8: {}", e))?;
                Ok(val)
            }
            None => bail!("no read_command configured"),
        }
    }

    fn supports_read(&self) -> bool {
        self.read_command.is_some()
    }
}

// ---------------------------------------------------------------------------
// HTTP handler
// ---------------------------------------------------------------------------

pub struct HttpHandler {
    url: String,
    method: String,
    body_template: Option<String>,
    headers: HashMap<String, String>,
    client: reqwest::blocking::Client,
}

impl HttpHandler {
    fn new(params: &HashMap<String, toml::Value>) -> Result<Self> {
        let url = params
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("http handler requires 'url' (string)"))?
            .to_string();
        let method = params
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("POST")
            .to_uppercase();
        let body_template = params
            .get("body_template")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let headers = params
            .get("headers")
            .and_then(|v| v.as_table())
            .map(|t| {
                t.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()?;
        Ok(Self {
            url,
            method,
            body_template,
            headers,
            client,
        })
    }
}

impl CustomInputHandler for HttpHandler {
    fn apply(&self, value: u8) -> Result<()> {
        let url = self.url.replace("{value}", &value.to_string());
        let mut req = match self.method.as_str() {
            "GET" => self.client.get(&url),
            "PUT" => self.client.put(&url),
            "PATCH" => self.client.patch(&url),
            "DELETE" => self.client.delete(&url),
            _ => self.client.post(&url),
        };
        for (k, v) in &self.headers {
            req = req.header(k, v);
        }
        if let Some(template) = &self.body_template {
            let body = template.replace("{value}", &value.to_string());
            req = req.body(body);
        }
        let resp = req.send()?;
        if !resp.status().is_success() {
            warn!("http handler: request returned status {}", resp.status());
        }
        Ok(())
    }

    fn read_current(&self) -> Result<u8> {
        bail!("http handler does not support reading current value")
    }

    fn supports_read(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// D-Bus handler (stub)
// ---------------------------------------------------------------------------

pub struct DbusHandler {
    dest: String,
    path: String,
    interface: String,
    method: String,
}

impl DbusHandler {
    fn new(params: &HashMap<String, toml::Value>) -> Result<Self> {
        let dest = params
            .get("dest")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("dbus handler requires 'dest' (string)"))?
            .to_string();
        let path = params
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("dbus handler requires 'path' (string)"))?
            .to_string();
        let interface = params
            .get("interface")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("dbus handler requires 'interface' (string)"))?
            .to_string();
        let method = params
            .get("method")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("dbus handler requires 'method' (string)"))?
            .to_string();
        Ok(Self {
            dest,
            path,
            interface,
            method,
        })
    }
}

impl CustomInputHandler for DbusHandler {
    fn apply(&self, value: u8) -> Result<()> {
        info!(
            "dbus handler: would call {}.{} on {} {} with value {}",
            self.interface, self.method, self.dest, self.path, value
        );
        Ok(())
    }

    fn read_current(&self) -> Result<u8> {
        bail!("dbus handler does not support reading current value")
    }

    fn supports_read(&self) -> bool {
        false
    }
}

