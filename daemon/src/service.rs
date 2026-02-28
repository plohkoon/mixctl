use std::sync::Mutex;

use mixctl_core::{api::MixCtlApi, State};

pub struct Service {
    state: Mutex<State>,
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

impl MixCtlApi for Service {
    fn ping(&self) -> String {
        "pong".to_string()
    }

    fn get_state(&self) -> State {
        self.state.lock().unwrap().clone()
    }

    fn set_profile(&self, name: String) {
        self.state.lock().unwrap().active_profile = name;
    }
}
