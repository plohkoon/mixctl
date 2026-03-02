use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::config::ConfigFile;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateFile {
    pub version: u32,
    pub current_page: u32,
    pub channels: HashMap<String, ChannelState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelState {
    pub muted: bool,
    pub volume: u8,
}

impl Default for ChannelState {
    fn default() -> Self {
        Self {
            muted: false,
            volume: 100,
        }
    }
}

impl Default for StateFile {
    fn default() -> Self {
        Self {
            version: 1,
            current_page: 0,
            channels: HashMap::new(),
        }
    }
}

impl StateFile {
    pub fn path() -> PathBuf {
        dirs::home_dir()
            .expect("no home directory")
            .join(".local/state/mixctl.toml")
    }

    pub fn load() -> anyhow::Result<Self> {
        let path = Self::path();
        if path.exists() {
            let text = fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            let state: Self = toml::from_str(&text)
                .with_context(|| format!("parsing {}", path.display()))?;
            if state.version != 1 {
                bail!(
                    "unsupported state version {}, expected 1 ({})",
                    state.version,
                    path.display()
                );
            }
            Ok(state)
        } else {
            Ok(Self::default())
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

    /// Reconcile state with config: drop non-u32 keys, add defaults for new
    /// channel IDs, remove stale entries, clamp page. Returns true if anything
    /// changed.
    pub fn reconcile(&mut self, config: &ConfigFile) -> bool {
        let mut changed = false;

        // Drop entries whose key doesn't parse as u32
        let bad_keys: Vec<String> = self.channels.keys()
            .filter(|k| k.parse::<u32>().is_err())
            .cloned()
            .collect();
        for key in bad_keys {
            warn!("dropping state entry with non-numeric key '{}'", key);
            self.channels.remove(&key);
            changed = true;
        }

        // Add defaults for channels in config but not in state
        for ch in &config.channels {
            let key = ch.id().to_string();
            if !self.channels.contains_key(&key) {
                self.channels.insert(key, ChannelState::default());
                changed = true;
            }
        }

        // Remove channels in state but not in config
        let config_ids: Vec<String> = config.channels.iter().map(|c| c.id().to_string()).collect();
        self.channels.retain(|key, _| {
            let keep = config_ids.contains(key);
            if !keep {
                changed = true;
            }
            keep
        });

        // Clamp page
        let max = config.max_page();
        if self.current_page > max {
            self.current_page = max;
            changed = true;
        }

        changed
    }

    pub fn channel_state(&self, id: u32) -> Option<&ChannelState> {
        self.channels.get(&id.to_string())
    }

    pub fn ensure_channel(&mut self, id: u32) -> &mut ChannelState {
        self.channels
            .entry(id.to_string())
            .or_insert_with(ChannelState::default)
    }

    pub fn remove_channel(&mut self, id: u32) {
        self.channels.remove(&id.to_string());
    }

    pub fn set_muted(&mut self, id: u32, muted: bool) {
        if let Some(ch) = self.channels.get_mut(&id.to_string()) {
            ch.muted = muted;
        }
    }

    pub fn set_volume(&mut self, id: u32, volume: u8) {
        if let Some(ch) = self.channels.get_mut(&id.to_string()) {
            ch.volume = volume;
        }
    }
}
