use crate::State;

pub const BUS_NAME: &str = "dev.greghuber.MixCtl";
pub const OBJ_PATH: &str = "/dev/greghuber/MixCtl1";

#[zbus::proxy(
    interface = "dev.greghuber.MixCtl1",
    default_service = "dev.greghuber.MixCtl",
    default_path = "/dev/greghuber/MixCtl1"
)]
pub trait MixCtl {
    fn ping(&self) -> zbus::Result<String>;

    fn get_state(&self) -> zbus::Result<State>;

    fn set_profile(&self, name: &str) -> zbus::Result<()>;
}
