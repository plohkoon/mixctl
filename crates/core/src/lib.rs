use serde::{Deserialize, Serialize};
use zvariant::Type;
pub mod dbus;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct State {
    pub connected: bool,
    pub active_profile: String,
}
