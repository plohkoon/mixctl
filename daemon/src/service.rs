use std::sync::Arc;

use mixctl_core::{InputInfo, OutputInfo, RouteInfo};
use tokio::sync::Mutex;

use crate::config::{ChannelConfig, ConfigFile};
use crate::state::{OutputState, RouteState, StateFile};

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

    pub fn build_input_info(cfg: &ChannelConfig) -> InputInfo {
        InputInfo {
            id: cfg.id(),
            name: cfg.name.clone(),
            color: cfg.color.clone(),
        }
    }

    pub fn build_output_info(cfg: &ChannelConfig, state: &OutputState) -> OutputInfo {
        OutputInfo {
            id: cfg.id(),
            name: cfg.name.clone(),
            color: cfg.color.clone(),
            volume: state.volume,
            muted: state.muted,
        }
    }

    pub fn build_route_info(input_id: u32, output_id: u32, state: &RouteState) -> RouteInfo {
        RouteInfo {
            input_id,
            output_id,
            volume: state.volume,
            muted: state.muted,
        }
    }
}
