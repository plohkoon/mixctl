use mixctl_core::State;
use zbus::interface;

use crate::service::Service;

#[interface(name = "dev.greghuber.MixCtl1")]
impl Service {
    async fn ping(&self) -> zbus::fdo::Result<String> {
        Ok("pong".to_string())
    }

    async fn get_state(&self) -> zbus::fdo::Result<State> {
        Ok(self.state.lock().await.clone())
    }

    async fn set_profile(&self, name: &str) -> zbus::fdo::Result<()> {
        self.state.lock().await.active_profile = name.to_string();
        Ok(())
    }
}
