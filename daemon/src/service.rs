use std::sync::Arc;

use mixctl_core::ChannelInfo;
use tokio::sync::Mutex;

use crate::config::{ChannelConfig, ConfigFile};
use crate::state::{ChannelState, StateFile};

#[derive(Clone)]
pub struct Service {
    pub(crate) inner: Arc<Mutex<Shared>>,
}

pub struct Shared {
    pub config: ConfigFile,
    pub state: StateFile,
    pub config_dirty: bool,
    pub state_dirty: bool,
}

impl Service {
    pub fn new(config: ConfigFile, state: StateFile) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Shared {
                config,
                state,
                config_dirty: false,
                state_dirty: false,
            })),
        }
    }

    pub fn build_channel_info(cfg: &ChannelConfig, state: &ChannelState) -> ChannelInfo {
        ChannelInfo {
            id: cfg.id(),
            name: cfg.name.clone(),
            color: cfg.color.clone(),
            muted: state.muted,
            volume: state.volume,
        }
    }
}
