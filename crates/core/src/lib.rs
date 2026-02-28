use serde::{Deserialize, Serialize};
pub mod dbus;
pub mod api;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub connected: bool,
    pub active_profile: String
}
