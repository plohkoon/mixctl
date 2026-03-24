use mixctl_core::dbus::MixCtlProxy;
use std::sync::atomic::AtomicU32;

pub struct AppState {
    pub proxy: Option<MixCtlProxy<'static>>,
    pub selected_output_id: AtomicU32,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            proxy: None,
            selected_output_id: AtomicU32::new(0),
        }
    }

    pub fn proxy(&self) -> Result<&MixCtlProxy<'static>, crate::error::Error> {
        self.proxy
            .as_ref()
            .ok_or_else(|| crate::error::Error::Other("not connected to daemon".into()))
    }
}
