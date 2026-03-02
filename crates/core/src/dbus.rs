use crate::{InputInfo, OutputInfo, RouteInfo};

pub const BUS_NAME: &str = "dev.greghuber.MixCtl";
pub const OBJ_PATH: &str = "/dev/greghuber/MixCtl1";

#[zbus::proxy(
    interface = "dev.greghuber.MixCtl1",
    default_service = "dev.greghuber.MixCtl",
    default_path = "/dev/greghuber/MixCtl1"
)]
pub trait MixCtl {
    fn ping(&self) -> zbus::Result<String>;

    // Inputs (config-only — no volume/mute)
    fn list_inputs(&self) -> zbus::Result<Vec<InputInfo>>;
    fn get_input(&self, id: u32) -> zbus::Result<InputInfo>;
    fn add_input(&self, name: &str, color: &str) -> zbus::Result<u32>;
    fn remove_input(&self, id: u32) -> zbus::Result<()>;
    fn move_input(&self, id: u32, position: u32) -> zbus::Result<()>;
    fn set_input_name(&self, id: u32, name: &str) -> zbus::Result<()>;
    fn set_input_color(&self, id: u32, color: &str) -> zbus::Result<()>;

    // Outputs (have master volume + mute)
    fn list_outputs(&self) -> zbus::Result<Vec<OutputInfo>>;
    fn get_output(&self, id: u32) -> zbus::Result<OutputInfo>;
    fn add_output(&self, name: &str, color: &str, source_output_id: u32) -> zbus::Result<u32>;
    fn remove_output(&self, id: u32) -> zbus::Result<()>;
    fn move_output(&self, id: u32, position: u32) -> zbus::Result<()>;
    fn set_output_name(&self, id: u32, name: &str) -> zbus::Result<()>;
    fn set_output_color(&self, id: u32, color: &str) -> zbus::Result<()>;
    fn set_output_volume(&self, id: u32, volume: u8) -> zbus::Result<()>;
    fn set_output_mute(&self, id: u32, muted: bool) -> zbus::Result<()>;

    // Routing (per input→output cell)
    fn get_route(&self, input_id: u32, output_id: u32) -> zbus::Result<RouteInfo>;
    fn list_routes_for_output(&self, output_id: u32) -> zbus::Result<Vec<RouteInfo>>;
    fn set_route_volume(&self, input_id: u32, output_id: u32, volume: u8) -> zbus::Result<()>;
    fn set_route_mute(&self, input_id: u32, output_id: u32, muted: bool) -> zbus::Result<()>;

    // Page
    fn get_current_page(&self) -> zbus::Result<u32>;
    fn set_current_page(&self, page: u32) -> zbus::Result<()>;

    // Signals
    #[zbus(signal)]
    fn inputs_config_changed(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn outputs_config_changed(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn output_state_changed(&self, id: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    fn route_changed(&self, input_id: u32, output_id: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    fn page_changed(&self, page: u32) -> zbus::Result<()>;
}
