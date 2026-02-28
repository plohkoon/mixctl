use mixctl_core::api::MixCtlApi;
use zbus::interface;

use crate::service::Service;

#[interface(name = "dev.greghuber.MixCtl1")]
impl Service {
    fn ping(&self) -> String {
        <Self as MixCtlApi>::ping(self)
    }

    fn get_state_json(&self) -> String {
        let state = <Self as MixCtlApi>::get_state(self);
        serde_json::to_string(&state).unwrap()
    }

    fn set_profile(&self, name: &str) {
        <Self as MixCtlApi>::set_profile(self, name.to_string())
    }
}
