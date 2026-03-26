use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, bail};
use mixctl_core::config_sections::{AppletConfig, BeacnConfig, CliConfig, TuiConfig, UiConfig};
use serde::{Deserialize, Serialize};
use tracing::warn;

fn is_default<T: Default + PartialEq>(val: &T) -> bool {
    *val == T::default()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    pub version: u32,
    pub inputs: Vec<ChannelConfig>,
    pub outputs: Vec<ChannelConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_input: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_output: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub app_rules: Vec<AppRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_inputs: Vec<CustomInputConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub broadcast_levels: Option<bool>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub beacn: BeacnConfig,
    #[serde(default, skip_serializing_if = "is_default")]
    pub ui: UiConfig,
    #[serde(default, skip_serializing_if = "is_default")]
    pub applet: AppletConfig,
    #[serde(default, skip_serializing_if = "is_default")]
    pub cli: CliConfig,
    #[serde(default, skip_serializing_if = "is_default")]
    pub tui: TuiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    #[serde(default)]
    pub id: Option<u32>,
    pub name: String,
    pub color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_device: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_device: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppRule {
    pub app_name: String,
    pub input_id: u32,
}

fn default_true() -> bool {
    true
}

fn is_true(val: &bool) -> bool {
    *val
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomInputConfig {
    #[serde(default)]
    pub id: Option<u32>,
    pub name: String,
    pub color: String,
    pub custom_type: String,
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub restore_on_exit: bool,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub params: HashMap<String, toml::Value>,
}

impl CustomInputConfig {
    /// Returns the id, panicking if it hasn't been assigned yet.
    /// Only call after `fixup_ids()` has run.
    pub fn id(&self) -> u32 {
        self.id.expect("custom input id not assigned; fixup_ids() must run first")
    }
}

impl ChannelConfig {
    /// Returns the id, panicking if it hasn't been assigned yet.
    /// Only call after `fixup_ids()` has run.
    pub fn id(&self) -> u32 {
        self.id.expect("channel id not assigned; fixup_ids() must run first")
    }
}

impl Default for ConfigFile {
    fn default() -> Self {
        Self {
            version: 1,
            inputs: vec![
                ChannelConfig { id: Some(1), name: "System".into(), color: "#4A90D9".into(), target_device: None, capture_device: None },
                ChannelConfig { id: Some(2), name: "Game".into(), color: "#E74C3C".into(), target_device: None, capture_device: None },
                ChannelConfig { id: Some(3), name: "Music".into(), color: "#2ECC71".into(), target_device: None, capture_device: None },
                ChannelConfig { id: Some(4), name: "Chat".into(), color: "#F39C12".into(), target_device: None, capture_device: None },
            ],
            outputs: vec![
                ChannelConfig { id: Some(5), name: "Personal Mix".into(), color: "#8E44AD".into(), target_device: None, capture_device: None },
                ChannelConfig { id: Some(6), name: "Voice Chat Mix".into(), color: "#3498DB".into(), target_device: None, capture_device: None },
                ChannelConfig { id: Some(7), name: "Audience Mix".into(), color: "#E67E22".into(), target_device: None, capture_device: None },
                ChannelConfig { id: Some(8), name: "VOD Track".into(), color: "#1ABC9C".into(), target_device: None, capture_device: None },
            ],
            default_input: Some(1),
            default_output: Some(6),
            app_rules: Vec::new(),
            custom_inputs: Vec::new(),
            broadcast_levels: None,
            beacn: BeacnConfig::default(),
            ui: UiConfig::default(),
            applet: AppletConfig::default(),
            cli: CliConfig::default(),
            tui: TuiConfig::default(),
        }
    }
}

impl ConfigFile {
    pub fn path() -> PathBuf {
        dirs::home_dir()
            .expect("no home directory")
            .join(".config/mixctl.toml")
    }

    pub fn load_or_create() -> anyhow::Result<Self> {
        let path = Self::path();
        if path.exists() {
            let text = fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            let mut config: Self = toml::from_str(&text)
                .with_context(|| format!("parsing {}", path.display()))?;
            if config.version != 1 {
                bail!(
                    "unsupported config version {}, expected 1 ({})",
                    config.version,
                    path.display()
                );
            }
            config.fixup_ids();
            Ok(config)
        } else {
            let config = Self::default();
            config.save()?;
            Ok(config)
        }
    }

    /// Assign IDs to any entries missing one, and deduplicate.
    /// Shared ID space across inputs, outputs, and custom inputs.
    fn fixup_ids(&mut self) {
        let mut seen = HashSet::new();
        for entry in self.inputs.iter_mut().chain(self.outputs.iter_mut()) {
            fixup_entry_id(&mut seen, &mut entry.id, &entry.name);
        }
        for entry in &mut self.custom_inputs {
            fixup_entry_id(&mut seen, &mut entry.id, &entry.name);
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let text = toml::to_string_pretty(self)?;
        fs::write(&path, text)
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    pub fn find_input(&self, id: u32) -> Option<&ChannelConfig> {
        self.inputs.iter().find(|c| c.id() == id)
    }

    pub fn find_input_mut(&mut self, id: u32) -> Option<&mut ChannelConfig> {
        self.inputs.iter_mut().find(|c| c.id() == id)
    }

    pub fn find_output(&self, id: u32) -> Option<&ChannelConfig> {
        self.outputs.iter().find(|c| c.id() == id)
    }

    pub fn find_output_mut(&mut self, id: u32) -> Option<&mut ChannelConfig> {
        self.outputs.iter_mut().find(|c| c.id() == id)
    }

    pub fn next_unused_id(&self) -> u32 {
        let mut used: HashSet<u32> = self.inputs.iter()
            .chain(self.outputs.iter())
            .map(|c| c.id())
            .collect();
        for ci in &self.custom_inputs {
            used.insert(ci.id());
        }
        next_unused_id(&used)
    }

    #[allow(dead_code)]
    pub fn find_custom_input(&self, id: u32) -> Option<&CustomInputConfig> {
        self.custom_inputs.iter().find(|c| c.id() == id)
    }

    #[allow(dead_code)]
    pub fn find_custom_input_mut(&mut self, id: u32) -> Option<&mut CustomInputConfig> {
        self.custom_inputs.iter_mut().find(|c| c.id() == id)
    }

    pub fn is_custom_input(&self, id: u32) -> bool {
        self.custom_inputs.iter().any(|c| c.id() == id)
    }
}

/// Find the smallest positive integer not in `used`.
fn next_unused_id(used: &HashSet<u32>) -> u32 {
    (1..).find(|id| !used.contains(id)).unwrap()
}

/// Helper: fix up a single ID entry (assign new if missing/duplicate/zero).
fn fixup_entry_id(seen: &mut HashSet<u32>, id: &mut Option<u32>, name: &str) {
    match *id {
        Some(v) if v > 0 && seen.insert(v) => {
            // valid unique id, keep it
        }
        Some(v) => {
            let new_id = next_unused_id(seen);
            warn!(
                "'{}' has duplicate/invalid id {}, reassigning to {}",
                name, v, new_id
            );
            seen.insert(new_id);
            *id = Some(new_id);
        }
        None => {
            let new_id = next_unused_id(seen);
            warn!("'{}' missing id, assigning {}", name, new_id);
            seen.insert(new_id);
            *id = Some(new_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_channel(id: Option<u32>, name: &str) -> ChannelConfig {
        ChannelConfig {
            id,
            name: name.into(),
            color: "#000000".into(),
            target_device: None,
            capture_device: None,
        }
    }

    #[test]
    fn fixup_ids_duplicate_input_ids_reassigned() {
        let mut config = ConfigFile {
            version: 1,
            inputs: vec![
                make_channel(Some(1), "A"),
                make_channel(Some(1), "B"), // duplicate
            ],
            outputs: vec![],
            default_input: None,
            default_output: None,
            app_rules: Vec::new(),
            custom_inputs: vec![],
            broadcast_levels: None,
            beacn: Default::default(),
            ui: Default::default(),
            applet: Default::default(),
            cli: Default::default(),
            tui: Default::default(),
        };
        config.fixup_ids();

        let id_a = config.inputs[0].id();
        let id_b = config.inputs[1].id();
        assert_eq!(id_a, 1);
        assert_eq!(id_b, 2); // reassigned to next available
        assert_ne!(id_a, id_b);
    }

    #[test]
    fn fixup_ids_zero_ids_assigned_from_one() {
        let mut config = ConfigFile {
            version: 1,
            inputs: vec![
                make_channel(Some(0), "A"),
                make_channel(Some(0), "B"),
            ],
            outputs: vec![
                make_channel(Some(0), "C"),
            ],
            default_input: None,
            default_output: None,
            app_rules: Vec::new(),
            custom_inputs: vec![],
            broadcast_levels: None,
            beacn: Default::default(),
            ui: Default::default(),
            applet: Default::default(),
            cli: Default::default(),
            tui: Default::default(),
        };
        config.fixup_ids();

        assert_eq!(config.inputs[0].id(), 1);
        assert_eq!(config.inputs[1].id(), 2);
        assert_eq!(config.outputs[0].id(), 3);
    }

    #[test]
    fn fixup_ids_preserves_valid_unique_ids_with_gaps() {
        let mut config = ConfigFile {
            version: 1,
            inputs: vec![
                make_channel(Some(3), "A"),
                make_channel(Some(7), "B"),
            ],
            outputs: vec![
                make_channel(Some(10), "C"),
            ],
            default_input: None,
            default_output: None,
            app_rules: Vec::new(),
            custom_inputs: vec![],
            broadcast_levels: None,
            beacn: Default::default(),
            ui: Default::default(),
            applet: Default::default(),
            cli: Default::default(),
            tui: Default::default(),
        };
        config.fixup_ids();

        assert_eq!(config.inputs[0].id(), 3);
        assert_eq!(config.inputs[1].id(), 7);
        assert_eq!(config.outputs[0].id(), 10);
    }

    #[test]
    fn fixup_ids_none_ids_assigned() {
        let mut config = ConfigFile {
            version: 1,
            inputs: vec![
                make_channel(None, "A"),
                make_channel(None, "B"),
            ],
            outputs: vec![],
            default_input: None,
            default_output: None,
            app_rules: Vec::new(),
            custom_inputs: vec![],
            broadcast_levels: None,
            beacn: Default::default(),
            ui: Default::default(),
            applet: Default::default(),
            cli: Default::default(),
            tui: Default::default(),
        };
        config.fixup_ids();

        assert_eq!(config.inputs[0].id(), 1);
        assert_eq!(config.inputs[1].id(), 2);
    }

    #[test]
    fn next_unused_id_correctness() {
        let mut used = HashSet::new();
        assert_eq!(next_unused_id(&used), 1);

        used.insert(1);
        assert_eq!(next_unused_id(&used), 2);

        used.insert(2);
        used.insert(3);
        assert_eq!(next_unused_id(&used), 4);

        // gap: 1,2,3,5 -> next is 4
        used.remove(&3);
        used.insert(5);
        assert_eq!(next_unused_id(&used), 3);
    }

    #[test]
    fn config_next_unused_id_skips_used() {
        let config = ConfigFile {
            version: 1,
            inputs: vec![
                make_channel(Some(1), "A"),
                make_channel(Some(2), "B"),
            ],
            outputs: vec![
                make_channel(Some(3), "C"),
            ],
            default_input: None,
            default_output: None,
            app_rules: Vec::new(),
            custom_inputs: vec![],
            broadcast_levels: None,
            beacn: Default::default(),
            ui: Default::default(),
            applet: Default::default(),
            cli: Default::default(),
            tui: Default::default(),
        };
        assert_eq!(config.next_unused_id(), 4);
    }
}
