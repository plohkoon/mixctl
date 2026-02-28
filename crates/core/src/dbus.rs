pub const BUS_NAME: &str = "dev.greghuber.MixCtl";
pub const OBJ_PATH: &str = "/dev/greghuber/MixCtl";
pub const IFACE: &str = "dev.greghuber.MixCtl1";

#[zbus::proxy(
    interface = "dev.greghuber.MixCtl1",
    default_service = "dev.greghuber.MixCtl",
    default_path = "/dev/greghuber/MixCtl"
)]
pub trait MixCtl {
    fn ping(&self) -> zbus::Result<String>;

    // start simple: JSON string is fine for now, can migrate to typed later
    fn get_state_json(&self) -> zbus::Result<String>;

    fn set_profile(&self, name: &str) -> zbus::Result<()>;
}
