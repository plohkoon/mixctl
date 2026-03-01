use tokio::sync::Mutex;

use mixctl_core::State;

pub struct Service {
    pub(crate) state: Mutex<State>,
}

impl Service {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                connected: false,
                active_profile: "default".to_string(),
            }),
        }
    }
}
