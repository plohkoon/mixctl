use crate::ChannelInfo;

pub const BUS_NAME: &str = "dev.greghuber.MixCtl";
pub const OBJ_PATH: &str = "/dev/greghuber/MixCtl1";

#[zbus::proxy(
    interface = "dev.greghuber.MixCtl1",
    default_service = "dev.greghuber.MixCtl",
    default_path = "/dev/greghuber/MixCtl1"
)]
pub trait MixCtl {
    fn ping(&self) -> zbus::Result<String>;

    // Channel queries
    fn list_channels(&self) -> zbus::Result<Vec<ChannelInfo>>;
    fn get_channel(&self, id: u32) -> zbus::Result<ChannelInfo>;

    // Channel config mutations
    fn add_channel(&self, name: &str, color: &str) -> zbus::Result<u32>;
    fn remove_channel(&self, id: u32) -> zbus::Result<()>;
    fn set_channel_name(&self, id: u32, name: &str) -> zbus::Result<()>;
    fn move_channel(&self, id: u32, position: u32) -> zbus::Result<()>;
    fn set_channel_color(&self, id: u32, color: &str) -> zbus::Result<()>;

    // Channel state mutations
    fn set_channel_mute(&self, id: u32, muted: bool) -> zbus::Result<()>;
    fn set_channel_volume(&self, id: u32, volume: u8) -> zbus::Result<()>;

    // Page
    fn get_current_page(&self) -> zbus::Result<u32>;
    fn set_current_page(&self, page: u32) -> zbus::Result<()>;
}
