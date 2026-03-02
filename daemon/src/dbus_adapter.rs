use mixctl_core::{InputInfo, OutputInfo, RouteInfo, parse_hex_color};
use zbus::interface;
use zbus::object_server::SignalEmitter;

use crate::service::Service;

#[interface(name = "dev.greghuber.MixCtl1")]
impl Service {
    async fn ping(&self) -> zbus::fdo::Result<String> {
        Ok("pong".to_string())
    }

    // -- Inputs (config-only) --

    async fn list_inputs(&self) -> zbus::fdo::Result<Vec<InputInfo>> {
        let shared = self.inner.lock().await;
        let out: Vec<InputInfo> = shared.config.inputs.iter()
            .map(|cfg| Service::build_input_info(cfg))
            .collect();
        Ok(out)
    }

    async fn get_input(&self, id: u32) -> zbus::fdo::Result<InputInfo> {
        let shared = self.inner.lock().await;
        let cfg = shared.config.find_input(id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("input id {} not found", id)))?;
        Ok(Service::build_input_info(cfg))
    }

    async fn add_input(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        name: &str,
        color: &str,
    ) -> zbus::fdo::Result<u32> {
        if parse_hex_color(color).is_none() {
            return Err(zbus::fdo::Error::InvalidArgs(
                format!("invalid hex color '{}'", color),
            ));
        }
        let mut shared = self.inner.lock().await;
        let id = shared.config.next_unused_id();
        shared.config.inputs.push(crate::config::ChannelConfig {
            id: Some(id),
            name: name.to_string(),
            color: color.to_string(),
        });
        let output_ids: Vec<u32> = shared.config.outputs.iter().map(|o| o.id()).collect();
        shared.state.ensure_routes_for_input(id, &output_ids);
        shared.config_dirty = true;
        shared.state_dirty = true;
        drop(shared);
        Self::inputs_config_changed(&emitter).await.ok();
        Ok(id)
    }

    async fn remove_input(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        id: u32,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        let idx = shared.config.inputs.iter().position(|c| c.id() == id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("input id {} not found", id)))?;
        shared.config.inputs.remove(idx);
        shared.state.remove_routes_for_input(id);
        let max = shared.config.max_page();
        if shared.state.current_page > max {
            shared.state.current_page = max;
        }
        shared.config_dirty = true;
        shared.state_dirty = true;
        drop(shared);
        Self::inputs_config_changed(&emitter).await.ok();
        Ok(())
    }

    async fn move_input(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        id: u32,
        position: u32,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        let from = shared.config.inputs.iter().position(|c| c.id() == id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("input id {} not found", id)))?;
        let len = shared.config.inputs.len();
        let to = position as usize;
        if to >= len {
            return Err(zbus::fdo::Error::InvalidArgs(
                format!("position {} out of range (0..{})", position, len - 1),
            ));
        }
        let entry = shared.config.inputs.remove(from);
        shared.config.inputs.insert(to, entry);
        shared.config_dirty = true;
        drop(shared);
        Self::inputs_config_changed(&emitter).await.ok();
        Ok(())
    }

    async fn set_input_name(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        id: u32,
        name: &str,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        let cfg = shared.config.find_input_mut(id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("input id {} not found", id)))?;
        cfg.name = name.to_string();
        shared.config_dirty = true;
        drop(shared);
        Self::inputs_config_changed(&emitter).await.ok();
        Ok(())
    }

    async fn set_input_color(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        id: u32,
        color: &str,
    ) -> zbus::fdo::Result<()> {
        if parse_hex_color(color).is_none() {
            return Err(zbus::fdo::Error::InvalidArgs(
                format!("invalid hex color '{}'", color),
            ));
        }
        let mut shared = self.inner.lock().await;
        let cfg = shared.config.find_input_mut(id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("input id {} not found", id)))?;
        cfg.color = color.to_string();
        shared.config_dirty = true;
        drop(shared);
        Self::inputs_config_changed(&emitter).await.ok();
        Ok(())
    }

    // -- Outputs (config + state) --

    async fn list_outputs(&self) -> zbus::fdo::Result<Vec<OutputInfo>> {
        let shared = self.inner.lock().await;
        let mut out = Vec::with_capacity(shared.config.outputs.len());
        for cfg in &shared.config.outputs {
            let st = shared.state.output_state(cfg.id())
                .cloned()
                .unwrap_or_default();
            out.push(Service::build_output_info(cfg, &st));
        }
        Ok(out)
    }

    async fn get_output(&self, id: u32) -> zbus::fdo::Result<OutputInfo> {
        let shared = self.inner.lock().await;
        let cfg = shared.config.find_output(id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("output id {} not found", id)))?;
        let st = shared.state.output_state(id)
            .cloned()
            .unwrap_or_default();
        Ok(Service::build_output_info(cfg, &st))
    }

    async fn add_output(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        name: &str,
        color: &str,
        source_output_id: u32,
    ) -> zbus::fdo::Result<u32> {
        if parse_hex_color(color).is_none() {
            return Err(zbus::fdo::Error::InvalidArgs(
                format!("invalid hex color '{}'", color),
            ));
        }
        let mut shared = self.inner.lock().await;
        let id = shared.config.next_unused_id();
        shared.config.outputs.push(crate::config::ChannelConfig {
            id: Some(id),
            name: name.to_string(),
            color: color.to_string(),
        });
        shared.state.ensure_output(id);
        let input_ids: Vec<u32> = shared.config.inputs.iter().map(|i| i.id()).collect();
        shared.state.copy_routes_for_output(id, source_output_id, &input_ids);
        shared.config_dirty = true;
        shared.state_dirty = true;
        drop(shared);
        Self::outputs_config_changed(&emitter).await.ok();
        Ok(id)
    }

    async fn remove_output(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        id: u32,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        let idx = shared.config.outputs.iter().position(|c| c.id() == id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("output id {} not found", id)))?;
        shared.config.outputs.remove(idx);
        shared.state.remove_output(id);
        shared.state.remove_routes_for_output(id);
        shared.config_dirty = true;
        shared.state_dirty = true;
        drop(shared);
        Self::outputs_config_changed(&emitter).await.ok();
        Ok(())
    }

    async fn move_output(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        id: u32,
        position: u32,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        let from = shared.config.outputs.iter().position(|c| c.id() == id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("output id {} not found", id)))?;
        let len = shared.config.outputs.len();
        let to = position as usize;
        if to >= len {
            return Err(zbus::fdo::Error::InvalidArgs(
                format!("position {} out of range (0..{})", position, len - 1),
            ));
        }
        let entry = shared.config.outputs.remove(from);
        shared.config.outputs.insert(to, entry);
        shared.config_dirty = true;
        drop(shared);
        Self::outputs_config_changed(&emitter).await.ok();
        Ok(())
    }

    async fn set_output_name(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        id: u32,
        name: &str,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        let cfg = shared.config.find_output_mut(id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("output id {} not found", id)))?;
        cfg.name = name.to_string();
        shared.config_dirty = true;
        drop(shared);
        Self::outputs_config_changed(&emitter).await.ok();
        Ok(())
    }

    async fn set_output_color(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        id: u32,
        color: &str,
    ) -> zbus::fdo::Result<()> {
        if parse_hex_color(color).is_none() {
            return Err(zbus::fdo::Error::InvalidArgs(
                format!("invalid hex color '{}'", color),
            ));
        }
        let mut shared = self.inner.lock().await;
        let cfg = shared.config.find_output_mut(id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("output id {} not found", id)))?;
        cfg.color = color.to_string();
        shared.config_dirty = true;
        drop(shared);
        Self::outputs_config_changed(&emitter).await.ok();
        Ok(())
    }

    async fn set_output_volume(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        id: u32,
        volume: u8,
    ) -> zbus::fdo::Result<()> {
        if volume > 100 {
            return Err(zbus::fdo::Error::InvalidArgs(
                format!("volume {} exceeds maximum of 100", volume),
            ));
        }
        let mut shared = self.inner.lock().await;
        if shared.config.find_output(id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("output id {} not found", id)));
        }
        shared.state.set_output_volume(id, volume);
        shared.state_dirty = true;
        drop(shared);
        Self::output_state_changed(&emitter, id).await.ok();
        Ok(())
    }

    async fn set_output_mute(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        id: u32,
        muted: bool,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        if shared.config.find_output(id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("output id {} not found", id)));
        }
        shared.state.set_output_muted(id, muted);
        shared.state_dirty = true;
        drop(shared);
        Self::output_state_changed(&emitter, id).await.ok();
        Ok(())
    }

    // -- Routing --

    async fn get_route(&self, input_id: u32, output_id: u32) -> zbus::fdo::Result<RouteInfo> {
        let shared = self.inner.lock().await;
        if shared.config.find_input(input_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("input id {} not found", input_id)));
        }
        if shared.config.find_output(output_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("output id {} not found", output_id)));
        }
        let st = shared.state.route_state(input_id, output_id)
            .cloned()
            .unwrap_or_default();
        Ok(Service::build_route_info(input_id, output_id, &st))
    }

    async fn list_routes_for_output(&self, output_id: u32) -> zbus::fdo::Result<Vec<RouteInfo>> {
        let shared = self.inner.lock().await;
        if shared.config.find_output(output_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("output id {} not found", output_id)));
        }
        let mut out = Vec::new();
        for inp in &shared.config.inputs {
            let input_id = inp.id();
            let st = shared.state.route_state(input_id, output_id)
                .cloned()
                .unwrap_or_default();
            out.push(Service::build_route_info(input_id, output_id, &st));
        }
        Ok(out)
    }

    async fn set_route_volume(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        input_id: u32,
        output_id: u32,
        volume: u8,
    ) -> zbus::fdo::Result<()> {
        if volume > 100 {
            return Err(zbus::fdo::Error::InvalidArgs(
                format!("volume {} exceeds maximum of 100", volume),
            ));
        }
        let mut shared = self.inner.lock().await;
        if shared.config.find_input(input_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("input id {} not found", input_id)));
        }
        if shared.config.find_output(output_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("output id {} not found", output_id)));
        }
        shared.state.set_route_volume(input_id, output_id, volume);
        shared.state_dirty = true;
        drop(shared);
        Self::route_changed(&emitter, input_id, output_id).await.ok();
        Ok(())
    }

    async fn set_route_mute(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        input_id: u32,
        output_id: u32,
        muted: bool,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        if shared.config.find_input(input_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("input id {} not found", input_id)));
        }
        if shared.config.find_output(output_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("output id {} not found", output_id)));
        }
        shared.state.set_route_muted(input_id, output_id, muted);
        shared.state_dirty = true;
        drop(shared);
        Self::route_changed(&emitter, input_id, output_id).await.ok();
        Ok(())
    }

    // -- Page --

    async fn get_current_page(&self) -> zbus::fdo::Result<u32> {
        let shared = self.inner.lock().await;
        Ok(shared.state.current_page)
    }

    async fn set_current_page(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        page: u32,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        let max = shared.config.max_page();
        if page > max {
            return Err(zbus::fdo::Error::InvalidArgs(
                format!("page {} out of range (max {})", page, max),
            ));
        }
        shared.state.current_page = page;
        shared.state_dirty = true;
        drop(shared);
        Self::page_changed(&emitter, page).await.ok();
        Ok(())
    }

    // -- Signals --

    #[zbus(signal)]
    async fn inputs_config_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn outputs_config_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn output_state_changed(emitter: &SignalEmitter<'_>, id: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn route_changed(emitter: &SignalEmitter<'_>, input_id: u32, output_id: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn page_changed(emitter: &SignalEmitter<'_>, page: u32) -> zbus::Result<()>;
}
