use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use tracing::warn;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    pub version: u32,
    pub inputs: Vec<ChannelConfig>,
    pub outputs: Vec<ChannelConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    #[serde(default)]
    pub id: Option<u32>,
    pub name: String,
    pub color: String,
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
                ChannelConfig { id: Some(1), name: "System".into(), color: "#4A90D9".into() },
                ChannelConfig { id: Some(2), name: "Game".into(), color: "#E74C3C".into() },
                ChannelConfig { id: Some(3), name: "Music".into(), color: "#2ECC71".into() },
                ChannelConfig { id: Some(4), name: "Chat".into(), color: "#F39C12".into() },
            ],
            outputs: vec![
                ChannelConfig { id: Some(5), name: "Personal Mix".into(), color: "#8E44AD".into() },
                ChannelConfig { id: Some(6), name: "Voice Chat Mix".into(), color: "#3498DB".into() },
                ChannelConfig { id: Some(7), name: "Audience Mix".into(), color: "#E67E22".into() },
                ChannelConfig { id: Some(8), name: "VOD Track".into(), color: "#1ABC9C".into() },
            ],
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
    /// Shared ID space across both inputs and outputs.
    fn fixup_ids(&mut self) {
        let mut seen = HashSet::new();
        for entry in self.inputs.iter_mut().chain(self.outputs.iter_mut()) {
            match entry.id {
                Some(id) if id > 0 && seen.insert(id) => {
                    // valid unique id, keep it
                }
                Some(id) => {
                    // duplicate or zero
                    let new_id = next_unused_id(&seen);
                    warn!(
                        "'{}' has duplicate/invalid id {}, reassigning to {}",
                        entry.name, id, new_id
                    );
                    seen.insert(new_id);
                    entry.id = Some(new_id);
                }
                None => {
                    let new_id = next_unused_id(&seen);
                    warn!(
                        "'{}' missing id, assigning {}",
                        entry.name, new_id
                    );
                    seen.insert(new_id);
                    entry.id = Some(new_id);
                }
            }
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
        let used: HashSet<u32> = self.inputs.iter()
            .chain(self.outputs.iter())
            .map(|c| c.id())
            .collect();
        next_unused_id(&used)
    }

    pub fn max_page(&self) -> u32 {
        let n = self.inputs.len() as u32;
        if n == 0 { 0 } else { (n - 1) / 4 }
    }
}

/// Find the smallest positive integer not in `used`.
fn next_unused_id(used: &HashSet<u32>) -> u32 {
    (1..).find(|id| !used.contains(id)).unwrap()
}
