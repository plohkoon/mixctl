use mixctl_core::{
    AppRuleInfo, CaptureDeviceInfo, ComponentInfo, CustomInputInfo, InputInfo, OutputInfo,
    PlaybackDeviceInfo, RouteInfo, StreamInfo, parse_hex_color,
};
use zbus::interface;
use zbus::object_server::SignalEmitter;

use mixctl_core::config_sections::{AppletConfig, BeacnConfig, CliConfig, TuiConfig, UiConfig};

use tracing::{info, warn};

use crate::audio::PwCommand;
use crate::audio::mixer::MAX_SLOTS;
use crate::audio::volume::u8_to_pw_volume;
use crate::service::{Service, ServiceSignal};

#[interface(name = "dev.greghuber.MixCtl1")]
impl Service {
    async fn ping(&self) -> zbus::fdo::Result<String> {
        Ok("pong".to_string())
    }

    // -- Audio status --

    async fn get_audio_status(&self) -> zbus::fdo::Result<String> {
        let shared = self.inner.lock().await;
        if shared.audio_connected {
            Ok("connected".to_string())
        } else {
            Ok("disconnected".to_string())
        }
    }

    // -- Default input --

    async fn get_default_input(&self) -> zbus::fdo::Result<u32> {
        let shared = self.inner.lock().await;
        Ok(shared.config.default_input.unwrap_or(0))
    }

    async fn set_default_input(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        id: u32,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        if id > 0 && shared.config.find_input(id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!(
                "input id {} not found",
                id
            )));
        }
        shared.config.default_input = if id == 0 { None } else { Some(id) };
        shared.config_dirty = true;
        Service::send_pw_cmd(&shared, PwCommand::SetDefaultInput { input_id: id });
        drop(shared);
        Self::inputs_config_changed(&emitter).await.ok();
        Ok(())
    }

    async fn get_default_output(&self) -> zbus::fdo::Result<u32> {
        let shared = self.inner.lock().await;
        Ok(shared.config.default_output.unwrap_or(0))
    }

    async fn set_default_output(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        id: u32,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        if id > 0 {
            if !shared.config.outputs.iter().any(|o| o.id() == id) {
                return Err(zbus::fdo::Error::Failed(format!("output id {} not found", id)));
            }
        }
        shared.config.default_output = if id == 0 { None } else { Some(id) };
        shared.config_dirty = true;
        Service::send_pw_cmd(&shared, PwCommand::SetDefaultOutput { output_id: id });
        drop(shared);
        Self::outputs_config_changed(&emitter).await.ok();
        Ok(())
    }

    // -- Inputs (config-only) --

    async fn list_inputs(&self) -> zbus::fdo::Result<Vec<InputInfo>> {
        let shared = self.inner.lock().await;
        let out: Vec<InputInfo> = shared
            .config
            .inputs
            .iter()
            .map(|cfg| Service::build_input_info(cfg))
            .collect();
        Ok(out)
    }

    async fn get_input(&self, id: u32) -> zbus::fdo::Result<InputInfo> {
        let shared = self.inner.lock().await;
        let cfg = shared
            .config
            .find_input(id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("input id {} not found", id)))?;
        Ok(Service::build_input_info(cfg))
    }

    async fn add_input(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        name: &str,
        color: &str,
    ) -> zbus::fdo::Result<u32> {
        if name.is_empty() || name.len() > 50 {
            return Err(zbus::fdo::Error::InvalidArgs(
                "name must be 1-50 characters".into(),
            ));
        }
        if parse_hex_color(color).is_none() {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "invalid hex color '{}'",
                color
            )));
        }
        let mut shared = self.inner.lock().await;
        if shared.config.inputs.len() >= MAX_SLOTS {
            return Err(zbus::fdo::Error::InvalidArgs(
                format!("maximum number of inputs ({}) reached", MAX_SLOTS),
            ));
        }
        let id = shared.config.next_unused_id();
        shared.config.inputs.push(crate::config::ChannelConfig {
            id: Some(id),
            name: name.to_string(),
            color: color.to_string(),
            target_device: None,
            capture_device: None,
        });
        let output_ids: Vec<u32> = shared.config.outputs.iter().map(|o| o.id()).collect();
        shared.state.ensure_routes_for_input(id, &output_ids);
        shared.config_dirty = true;
        shared.state_dirty = true;

        // Create PipeWire sink
        Service::send_pw_cmd(
            &shared,
            PwCommand::CreateInputSink {
                input_id: id,
                description: name.to_string(),
            },
        );

        // Create route loopbacks to all outputs (with combined volume)
        for &output_id in &output_ids {
            shared.send_route_link(id, output_id);
        }


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
        let idx = shared
            .config
            .inputs
            .iter()
            .position(|c| c.id() == id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("input id {} not found", id)))?;

        // DestroyInputSink now handles route loopback + capture loopback cleanup
        Service::send_pw_cmd(&shared, PwCommand::DestroyInputSink { input_id: id });

        shared.config.inputs.remove(idx);
        shared.state.remove_routes_for_input(id);
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
        let from = shared
            .config
            .inputs
            .iter()
            .position(|c| c.id() == id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("input id {} not found", id)))?;
        let len = shared.config.inputs.len();
        let to = position as usize;
        if to >= len {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "position {} out of range (0..{})",
                position,
                len - 1
            )));
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
        if name.is_empty() || name.len() > 50 {
            return Err(zbus::fdo::Error::InvalidArgs(
                "name must be 1-50 characters".into(),
            ));
        }
        let mut shared = self.inner.lock().await;
        let cfg = shared
            .config
            .find_input_mut(id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("input id {} not found", id)))?;
        cfg.name = name.to_string();
        shared.config_dirty = true;

        Service::send_pw_cmd(
            &shared,
            PwCommand::RenameInputSink {
                input_id: id,
                description: name.to_string(),
            },
        );


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
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "invalid hex color '{}'",
                color
            )));
        }
        let mut shared = self.inner.lock().await;
        let cfg = shared
            .config
            .find_input_mut(id)
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
            let st = shared
                .state
                .output_state(cfg.id())
                .cloned()
                .unwrap_or_default();
            out.push(Service::build_output_info(cfg, &st));
        }
        Ok(out)
    }

    async fn get_output(&self, id: u32) -> zbus::fdo::Result<OutputInfo> {
        let shared = self.inner.lock().await;
        let cfg = shared
            .config
            .find_output(id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("output id {} not found", id)))?;
        let st = shared
            .state
            .output_state(id)
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
        if name.is_empty() || name.len() > 50 {
            return Err(zbus::fdo::Error::InvalidArgs(
                "name must be 1-50 characters".into(),
            ));
        }
        if parse_hex_color(color).is_none() {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "invalid hex color '{}'",
                color
            )));
        }
        let mut shared = self.inner.lock().await;
        if shared.config.outputs.len() >= MAX_SLOTS {
            return Err(zbus::fdo::Error::InvalidArgs(
                format!("maximum number of outputs ({}) reached", MAX_SLOTS),
            ));
        }
        let id = shared.config.next_unused_id();
        shared.config.outputs.push(crate::config::ChannelConfig {
            id: Some(id),
            name: name.to_string(),
            color: color.to_string(),
            target_device: None,
            capture_device: None,
        });
        shared.state.ensure_output(id);
        let input_ids: Vec<u32> = shared.config.inputs.iter().map(|i| i.id()).collect();
        shared
            .state
            .copy_routes_for_output(id, source_output_id, &input_ids);
        shared.config_dirty = true;
        shared.state_dirty = true;

        // Create PipeWire source
        Service::send_pw_cmd(
            &shared,
            PwCommand::CreateOutputSource {
                output_id: id,
                description: name.to_string(),
            },
        );

        // Create route loopbacks from all inputs (with combined volume)
        for &input_id in &input_ids {
            shared.send_route_link(input_id, id);
        }


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
        let idx = shared
            .config
            .outputs
            .iter()
            .position(|c| c.id() == id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("output id {} not found", id)))?;

        // Destroy PipeWire output source (also destroys related route loopbacks)
        Service::send_pw_cmd(&shared, PwCommand::DestroyOutputSource { output_id: id });

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
        let from = shared
            .config
            .outputs
            .iter()
            .position(|c| c.id() == id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("output id {} not found", id)))?;
        let len = shared.config.outputs.len();
        let to = position as usize;
        if to >= len {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "position {} out of range (0..{})",
                position,
                len - 1
            )));
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
        if name.is_empty() || name.len() > 50 {
            return Err(zbus::fdo::Error::InvalidArgs(
                "name must be 1-50 characters".into(),
            ));
        }
        let mut shared = self.inner.lock().await;
        let cfg = shared
            .config
            .find_output_mut(id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("output id {} not found", id)))?;
        cfg.name = name.to_string();
        shared.config_dirty = true;

        Service::send_pw_cmd(
            &shared,
            PwCommand::RenameOutputSource {
                output_id: id,
                description: name.to_string(),
            },
        );


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
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "invalid hex color '{}'",
                color
            )));
        }
        let mut shared = self.inner.lock().await;
        let cfg = shared
            .config
            .find_output_mut(id)
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
        let volume = volume.min(100);
        let mut shared = self.inner.lock().await;
        if shared.config.find_output(id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!(
                "output id {} not found",
                id
            )));
        }
        shared.state.set_output_volume(id, volume);
        shared.state_dirty = true;

        // Update all route loopbacks for this output with combined volume
        let input_ids: Vec<u32> = shared.config.inputs.iter().map(|i| i.id()).collect();
        for input_id in input_ids {
            shared.send_route_link(input_id, id);
        }


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
            return Err(zbus::fdo::Error::Failed(format!(
                "output id {} not found",
                id
            )));
        }
        shared.state.set_output_muted(id, muted);
        shared.state_dirty = true;

        // Update all route loopbacks for this output with combined volume
        let input_ids: Vec<u32> = shared.config.inputs.iter().map(|i| i.id()).collect();
        for input_id in input_ids {
            shared.send_route_link(input_id, id);
        }


        drop(shared);
        Self::output_state_changed(&emitter, id).await.ok();
        Ok(())
    }

    async fn set_output_target(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        id: u32,
        device_name: &str,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        let cfg = shared
            .config
            .find_output_mut(id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("output id {} not found", id)))?;

        let device = if device_name.is_empty() {
            None
        } else {
            Some(device_name.to_string())
        };
        cfg.target_device = device.clone();
        shared.config_dirty = true;

        Service::send_pw_cmd(
            &shared,
            PwCommand::SetOutputTarget {
                output_id: id,
                device_name: device,
            },
        );

        drop(shared);
        Self::outputs_config_changed(&emitter).await.ok();
        Ok(())
    }

    // -- Routing --

    async fn get_route(&self, input_id: u32, output_id: u32) -> zbus::fdo::Result<RouteInfo> {
        let shared = self.inner.lock().await;
        if shared.config.find_input(input_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!(
                "input id {} not found",
                input_id
            )));
        }
        if shared.config.find_output(output_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!(
                "output id {} not found",
                output_id
            )));
        }
        let st = shared
            .state
            .route_state(input_id, output_id)
            .cloned()
            .unwrap_or_default();
        Ok(Service::build_route_info(input_id, output_id, &st))
    }

    async fn list_routes_for_output(
        &self,
        output_id: u32,
    ) -> zbus::fdo::Result<Vec<RouteInfo>> {
        let shared = self.inner.lock().await;
        if shared.config.find_output(output_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!(
                "output id {} not found",
                output_id
            )));
        }
        let mut out = Vec::new();
        for inp in &shared.config.inputs {
            let input_id = inp.id();
            let st = shared
                .state
                .route_state(input_id, output_id)
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
        let volume = volume.min(100);
        let mut shared = self.inner.lock().await;
        if shared.config.find_input(input_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!(
                "input id {} not found",
                input_id
            )));
        }
        if shared.config.find_output(output_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!(
                "output id {} not found",
                output_id
            )));
        }
        shared.state.set_route_volume(input_id, output_id, volume);
        shared.state_dirty = true;

        shared.send_route_link(input_id, output_id);


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
            return Err(zbus::fdo::Error::Failed(format!(
                "input id {} not found",
                input_id
            )));
        }
        if shared.config.find_output(output_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!(
                "output id {} not found",
                output_id
            )));
        }
        shared.state.set_route_muted(input_id, output_id, muted);
        shared.state_dirty = true;

        shared.send_route_link(input_id, output_id);


        drop(shared);
        Self::route_changed(&emitter, input_id, output_id).await.ok();
        Ok(())
    }

    // -- Streams (Phase 3) --

    async fn list_streams(&self) -> zbus::fdo::Result<Vec<StreamInfo>> {
        let shared = self.inner.lock().await;
        let streams: Vec<StreamInfo> = shared
            .active_streams
            .iter()
            .map(|(&pw_node_id, state)| StreamInfo {
                pw_node_id,
                app_name: state.app_name.clone(),
                media_name: state.media_name.clone(),
                input_id: state.input_id,
            })
            .collect();
        Ok(streams)
    }

    async fn assign_stream(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        pw_node_id: u32,
        input_id: u32,
        remember: bool,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        if shared.config.find_input(input_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!(
                "input id {} not found",
                input_id
            )));
        }

        let stream = shared
            .active_streams
            .get_mut(&pw_node_id)
            .ok_or_else(|| {
                zbus::fdo::Error::Failed(format!("stream {} not found", pw_node_id))
            })?;

        let app_name = stream.app_name.clone();
        stream.input_id = input_id;

        Service::send_pw_cmd(
            &shared,
            PwCommand::MoveStream {
                pw_node_id,
                input_id,
            },
        );

        if remember {
            // Add or update app rule
            if let Some(rule) = shared
                .config
                .app_rules
                .iter_mut()
                .find(|r| r.app_name == app_name)
            {
                rule.input_id = input_id;
            } else {
                shared.config.app_rules.push(crate::config::AppRule {
                    app_name,
                    input_id,
                });
            }
            shared.config_dirty = true;
        }
        shared.signal_tx.send(ServiceSignal::StreamsChanged).ok();
        drop(shared);
        if remember {
            Self::app_rules_changed(&emitter).await.ok();
        }
        Ok(())
    }

    // -- App Rules (Phase 3) --

    async fn list_app_rules(&self) -> zbus::fdo::Result<Vec<AppRuleInfo>> {
        let shared = self.inner.lock().await;
        let rules: Vec<AppRuleInfo> = shared
            .config
            .app_rules
            .iter()
            .map(|r| AppRuleInfo {
                app_name: r.app_name.clone(),
                input_id: r.input_id,
            })
            .collect();
        Ok(rules)
    }

    async fn set_app_rule(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        app_name: &str,
        input_id: u32,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        if shared.config.find_input(input_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!(
                "input id {} not found",
                input_id
            )));
        }

        if let Some(rule) = shared
            .config
            .app_rules
            .iter_mut()
            .find(|r| r.app_name == app_name)
        {
            rule.input_id = input_id;
        } else {
            shared.config.app_rules.push(crate::config::AppRule {
                app_name: app_name.to_string(),
                input_id,
            });
        }
        shared.config_dirty = true;
        drop(shared);
        Self::app_rules_changed(&emitter).await.ok();
        Ok(())
    }

    async fn remove_app_rule(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        app_name: &str,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        let before = shared.config.app_rules.len();
        shared
            .config
            .app_rules
            .retain(|r| r.app_name != app_name);
        if shared.config.app_rules.len() == before {
            return Err(zbus::fdo::Error::Failed(format!(
                "app rule '{}' not found",
                app_name
            )));
        }
        shared.config_dirty = true;
        drop(shared);
        Self::app_rules_changed(&emitter).await.ok();
        Ok(())
    }

    // -- Capture Devices (Phase 4) --

    async fn list_capture_devices(&self) -> zbus::fdo::Result<Vec<CaptureDeviceInfo>> {
        let shared = self.inner.lock().await;
        let devices: Vec<CaptureDeviceInfo> = shared
            .capture_devices
            .iter()
            .map(|(&pw_node_id, state)| {
                // Check if this device is already added as an input
                let added_input = shared
                    .config
                    .inputs
                    .iter()
                    .find(|i| i.capture_device.as_deref() == Some(&state.device_name));
                CaptureDeviceInfo {
                    pw_node_id,
                    name: state.name.clone(),
                    device_name: state.device_name.clone(),
                    is_added: added_input.is_some(),
                    input_id: added_input.map(|i| i.id()).unwrap_or(0),
                }
            })
            .collect();
        Ok(devices)
    }

    async fn add_capture_input(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        pw_node_id: u32,
        name: &str,
        color: &str,
    ) -> zbus::fdo::Result<u32> {
        if name.is_empty() || name.len() > 50 {
            return Err(zbus::fdo::Error::InvalidArgs(
                "name must be 1-50 characters".into(),
            ));
        }
        if parse_hex_color(color).is_none() {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "invalid hex color '{}'",
                color
            )));
        }

        let mut shared = self.inner.lock().await;
        if shared.config.inputs.len() >= MAX_SLOTS {
            return Err(zbus::fdo::Error::InvalidArgs(
                format!("maximum number of inputs ({}) reached", MAX_SLOTS),
            ));
        }
        let device_state = shared
            .capture_devices
            .get(&pw_node_id)
            .ok_or_else(|| {
                zbus::fdo::Error::Failed(format!("capture device {} not found", pw_node_id))
            })?
            .clone();

        let id = shared.config.next_unused_id();
        shared.config.inputs.push(crate::config::ChannelConfig {
            id: Some(id),
            name: name.to_string(),
            color: color.to_string(),
            target_device: None,
            capture_device: Some(device_state.device_name.clone()),
        });
        let output_ids: Vec<u32> = shared.config.outputs.iter().map(|o| o.id()).collect();
        shared.state.ensure_routes_for_input(id, &output_ids);
        shared.state.ensure_capture_volume(id);
        shared.config_dirty = true;
        shared.state_dirty = true;

        // Create PipeWire capture input (sink + capture loopback)
        Service::send_pw_cmd(
            &shared,
            PwCommand::CreateCaptureInput {
                input_id: id,
                description: name.to_string(),
                capture_device_name: device_state.device_name,
            },
        );

        // Create route loopbacks to all outputs (with combined volume)
        for &output_id in &output_ids {
            shared.send_route_link(id, output_id);
        }

        drop(shared);
        Self::inputs_config_changed(&emitter).await.ok();
        Ok(id)
    }

    // -- Playback Devices --

    async fn list_playback_devices(&self) -> zbus::fdo::Result<Vec<PlaybackDeviceInfo>> {
        let shared = self.inner.lock().await;
        let devices: Vec<PlaybackDeviceInfo> = shared
            .playback_devices
            .iter()
            .map(|(&pw_node_id, state)| PlaybackDeviceInfo {
                pw_node_id,
                name: state.name.clone(),
                device_name: state.device_name.clone(),
            })
            .collect();
        Ok(devices)
    }

    async fn remove_capture_input(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        id: u32,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        let cfg = shared
            .config
            .find_input_mut(id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("input id {} not found", id)))?;
        if cfg.capture_device.is_none() {
            return Err(zbus::fdo::Error::Failed(
                "input has no capture device".into(),
            ));
        }
        cfg.capture_device = None;
        shared.config_dirty = true;
        Service::send_pw_cmd(
            &shared,
            PwCommand::DestroyCaptureLoopback { input_id: id },
        );
        drop(shared);
        Self::inputs_config_changed(&emitter).await.ok();
        Self::capture_devices_changed(&emitter).await.ok();
        Ok(())
    }

    async fn bind_capture_to_input(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        input_id: u32,
        device_name: &str,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        // Verify target input exists
        if shared.config.find_input(input_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("input id {} not found", input_id)));
        }
        // Unbind this capture device from ANY input that currently has it
        let old_input_id = shared.config.inputs.iter()
            .find(|i| i.capture_device.as_deref() == Some(device_name))
            .map(|i| i.id());
        if let Some(old_id) = old_input_id {
            let cfg_mut = shared.config.find_input_mut(old_id).unwrap();
            cfg_mut.capture_device = None;
            Service::send_pw_cmd(
                &shared,
                PwCommand::DestroyCaptureLoopback { input_id: old_id },
            );
            tracing::info!("unbound capture device '{device_name}' from input {old_id}");
        }
        // Also unbind any existing capture device from the target input
        let target_has_capture = shared.config.find_input(input_id)
            .and_then(|c| c.capture_device.clone());
        if let Some(old_device) = target_has_capture {
            let cfg_mut = shared.config.find_input_mut(input_id).unwrap();
            cfg_mut.capture_device = None;
            Service::send_pw_cmd(
                &shared,
                PwCommand::DestroyCaptureLoopback { input_id },
            );
            tracing::info!("unbound previous capture device '{old_device}' from target input {input_id}");
        }
        // Feedback loop detection: check if any output targeted at the same
        // hardware device as this capture would create a mic→speaker→mic loop.
        for output in &shared.config.outputs {
            if let Some(ref target) = output.target_device {
                // Simple heuristic: if the capture device name and target device
                // share the same USB device path (e.g., both contain "Yeti"),
                // warn about potential feedback.
                let capture_base = device_name.split('.').nth(1).unwrap_or("");
                let target_base = target.split('.').nth(1).unwrap_or("");
                if !capture_base.is_empty() && capture_base == target_base {
                    // Check if this input routes to that output with non-zero volume
                    let route_key = format!("{input_id}:{}", output.id());
                    let is_routed = shared.state.routes.get(&route_key)
                        .map(|r| !r.muted && r.volume > 0)
                        .unwrap_or(true); // default route is unmuted
                    if is_routed {
                        warn!("potential feedback loop: capture device '{device_name}' routes to output '{}' which targets '{target}' (same hardware)", output.name);
                        // Don't block — just warn. User may have headphones on the same device.
                    }
                }
            }
        }

        let cfg = shared
            .config
            .find_input_mut(input_id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("input id {} not found", input_id)))?;
        cfg.capture_device = Some(device_name.to_string());
        shared.state.ensure_capture_volume(input_id);
        shared.config_dirty = true;
        shared.state_dirty = true;

        // Create only the capture loopback (input sink already exists)
        Service::send_pw_cmd(
            &shared,
            PwCommand::BindCaptureToInput {
                input_id,
                capture_device_name: device_name.to_string(),
            },
        );

        drop(shared);
        Self::inputs_config_changed(&emitter).await.ok();
        Self::capture_devices_changed(&emitter).await.ok();
        Ok(())
    }

    // -- Capture volume/mute --

    async fn set_capture_volume(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        id: u32,
        volume: u8,
    ) -> zbus::fdo::Result<()> {
        let volume = volume.min(100);
        let mut shared = self.inner.lock().await;
        let cfg = shared
            .config
            .find_input(id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("input id {} not found", id)))?;
        if cfg.capture_device.is_none() {
            return Err(zbus::fdo::Error::Failed(
                "input has no capture device".into(),
            ));
        }
        shared.state.set_capture_volume(id, volume);
        shared.state_dirty = true;

        let muted = shared
            .state
            .capture_volume_state(id)
            .map(|s| s.muted)
            .unwrap_or(false);
        let pw_volume = if muted { 0.0 } else { u8_to_pw_volume(volume) };
        Service::send_pw_cmd(
            &shared,
            PwCommand::SetCaptureVolume {
                input_id: id,
                pw_volume,
            },
        );

        drop(shared);
        Self::inputs_config_changed(&emitter).await.ok();
        Ok(())
    }

    async fn set_capture_mute(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        id: u32,
        muted: bool,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        let cfg = shared
            .config
            .find_input(id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("input id {} not found", id)))?;
        if cfg.capture_device.is_none() {
            return Err(zbus::fdo::Error::Failed(
                "input has no capture device".into(),
            ));
        }
        shared.state.set_capture_muted(id, muted);
        shared.state_dirty = true;

        let volume = shared
            .state
            .capture_volume_state(id)
            .map(|s| s.volume)
            .unwrap_or(100);
        let pw_volume = if muted { 0.0 } else { u8_to_pw_volume(volume) };
        Service::send_pw_cmd(
            &shared,
            PwCommand::SetCaptureVolume {
                input_id: id,
                pw_volume,
            },
        );

        drop(shared);
        Self::inputs_config_changed(&emitter).await.ok();
        Ok(())
    }



    // -- Config sections --

    async fn get_config_section(&self, section: &str) -> zbus::fdo::Result<String> {
        let shared = self.inner.lock().await;
        let json = match section {
            "beacn" => serde_json::to_string(&shared.config.beacn),
            "ui" => serde_json::to_string(&shared.config.ui),
            "applet" => serde_json::to_string(&shared.config.applet),
            "cli" => serde_json::to_string(&shared.config.cli),
            "tui" => serde_json::to_string(&shared.config.tui),
            _ => {
                return Err(zbus::fdo::Error::InvalidArgs(format!(
                    "unknown config section '{section}'"
                )));
            }
        }
        .map_err(|e| zbus::fdo::Error::Failed(format!("serialize failed: {e}")))?;
        Ok(json)
    }

    async fn set_config_section(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        section: &str,
        json: &str,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        match section {
            "beacn" => {
                shared.config.beacn = serde_json::from_str::<BeacnConfig>(json)
                    .map_err(|e| zbus::fdo::Error::InvalidArgs(format!("invalid JSON: {e}")))?;
            }
            "ui" => {
                shared.config.ui = serde_json::from_str::<UiConfig>(json)
                    .map_err(|e| zbus::fdo::Error::InvalidArgs(format!("invalid JSON: {e}")))?;
            }
            "applet" => {
                shared.config.applet = serde_json::from_str::<AppletConfig>(json)
                    .map_err(|e| zbus::fdo::Error::InvalidArgs(format!("invalid JSON: {e}")))?;
            }
            "cli" => {
                shared.config.cli = serde_json::from_str::<CliConfig>(json)
                    .map_err(|e| zbus::fdo::Error::InvalidArgs(format!("invalid JSON: {e}")))?;
            }
            "tui" => {
                shared.config.tui = serde_json::from_str::<TuiConfig>(json)
                    .map_err(|e| zbus::fdo::Error::InvalidArgs(format!("invalid JSON: {e}")))?;
            }
            _ => {
                return Err(zbus::fdo::Error::InvalidArgs(format!(
                    "unknown config section '{section}'"
                )));
            }
        }
        shared.config_dirty = true;
        let section_name = section.to_string();
        shared
            .signal_tx
            .send(ServiceSignal::ConfigSectionChanged {
                section: section_name.clone(),
            })
            .ok();
        drop(shared);
        Self::config_section_changed(&emitter, section_name).await.ok();
        Ok(())
    }

    // -- Level monitoring --

    async fn get_broadcast_levels(&self) -> zbus::fdo::Result<bool> {
        let shared = self.inner.lock().await;
        Ok(shared.config.broadcast_levels.unwrap_or(false))
    }

    async fn set_broadcast_levels(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        enabled: bool,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        let was_enabled = shared.config.broadcast_levels.unwrap_or(false);
        if enabled == was_enabled {
            return Ok(());
        }
        shared.config.broadcast_levels = Some(enabled);
        shared.config_dirty = true;

        if enabled {
            Service::send_pw_cmd(&shared, PwCommand::EnableLevelMonitoring);
        } else {
            Service::send_pw_cmd(&shared, PwCommand::DisableLevelMonitoring);
            shared.input_levels.clear();
        }
        shared
            .signal_tx
            .send(ServiceSignal::BroadcastLevelsChanged { enabled })
            .ok();

        drop(shared);
        Self::broadcast_levels_changed(&emitter, enabled).await.ok();
        Ok(())
    }

    async fn get_input_levels(&self) -> zbus::fdo::Result<Vec<(u32, f64)>> {
        let shared = self.inner.lock().await;
        let levels: Vec<(u32, f64)> = shared
            .input_levels
            .iter()
            .map(|(&id, &lvl)| (id, lvl as f64))
            .collect();
        Ok(levels)
    }

    // -- Component tracking --

    async fn register_component(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        component_type: &str,
    ) -> zbus::fdo::Result<()> {
        let sender = header
            .sender()
            .ok_or_else(|| zbus::fdo::Error::Failed("no sender".into()))?
            .to_string();
        let mut shared = self.inner.lock().await;
        shared.components.insert(sender.clone(), component_type.to_string());
        info!("component registered: {component_type} ({sender})");
        shared.signal_tx.send(ServiceSignal::ComponentChanged).ok();
        drop(shared);
        Self::component_changed(&emitter).await.ok();
        Ok(())
    }

    async fn list_components(&self) -> zbus::fdo::Result<Vec<ComponentInfo>> {
        let shared = self.inner.lock().await;
        let components: Vec<ComponentInfo> = shared
            .components
            .iter()
            .map(|(bus_name, component_type)| ComponentInfo {
                bus_name: bus_name.clone(),
                component_type: component_type.clone(),
            })
            .collect();
        Ok(components)
    }

    // -- DSP: EQ (per input, 8 bands) --

    async fn set_input_eq_enabled(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        input_id: u32,
        enabled: bool,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        if shared.config.find_input(input_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("input id {} not found", input_id)));
        }
        shared.state.ensure_input_eq(input_id).enabled = enabled;
        shared.state_dirty = true;
        Service::send_pw_cmd(&shared, PwCommand::SetInputEqEnabled { input_id, enabled });
        drop(shared);
        Self::input_dsp_changed(&emitter, input_id).await.ok();
        Ok(())
    }

    async fn get_input_eq_enabled(&self, input_id: u32) -> zbus::fdo::Result<bool> {
        let shared = self.inner.lock().await;
        if shared.config.find_input(input_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("input id {} not found", input_id)));
        }
        Ok(shared.state.input_eq_state(input_id).map(|s| s.enabled).unwrap_or(false))
    }

    async fn set_input_eq_band(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        input_id: u32,
        band: u8,
        band_type: &str,
        freq: f64,
        gain_db: f64,
        q: f64,
    ) -> zbus::fdo::Result<()> {
        if band >= 8 {
            return Err(zbus::fdo::Error::InvalidArgs("band must be 0-7".into()));
        }
        let valid_types = ["peaking", "low_shelf", "high_shelf", "bypass"];
        if !valid_types.contains(&band_type) {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "invalid band_type '{}', expected one of: peaking, low_shelf, high_shelf, bypass",
                band_type
            )));
        }
        let mut shared = self.inner.lock().await;
        if shared.config.find_input(input_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("input id {} not found", input_id)));
        }
        let eq = shared.state.ensure_input_eq(input_id);
        while eq.bands.len() <= band as usize {
            eq.bands.push(crate::state::EqBandDspState::default());
        }
        eq.bands[band as usize] = crate::state::EqBandDspState {
            band_type: band_type.to_string(),
            frequency: freq,
            gain_db,
            q,
        };
        shared.state_dirty = true;
        Service::send_pw_cmd(&shared, PwCommand::SetInputEqBand {
            input_id,
            band,
            band_type: band_type.to_string(),
            freq,
            gain_db,
            q,
        });
        drop(shared);
        Self::input_dsp_changed(&emitter, input_id).await.ok();
        Ok(())
    }

    async fn get_input_eq(
        &self,
        input_id: u32,
    ) -> zbus::fdo::Result<Vec<mixctl_core::EqBandInfo>> {
        let shared = self.inner.lock().await;
        if shared.config.find_input(input_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("input id {} not found", input_id)));
        }
        let eq = shared.state.input_eq_state(input_id);
        match eq {
            Some(eq) => Ok(eq.bands.iter().map(|b| mixctl_core::EqBandInfo {
                band_type: b.band_type.clone(),
                frequency: b.frequency,
                gain_db: b.gain_db,
                q: b.q,
            }).collect()),
            None => {
                let default = crate::state::InputEqDspState::default();
                Ok(default.bands.iter().map(|b| mixctl_core::EqBandInfo {
                    band_type: b.band_type.clone(),
                    frequency: b.frequency,
                    gain_db: b.gain_db,
                    q: b.q,
                }).collect())
            }
        }
    }

    async fn reset_input_eq(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        input_id: u32,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        if shared.config.find_input(input_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("input id {} not found", input_id)));
        }
        *shared.state.ensure_input_eq(input_id) = crate::state::InputEqDspState::default();
        shared.state_dirty = true;
        Service::send_pw_cmd(&shared, PwCommand::ResetInputEq { input_id });
        drop(shared);
        Self::input_dsp_changed(&emitter, input_id).await.ok();
        Ok(())
    }

    // -- DSP: Gate (per input) --

    async fn set_input_gate_enabled(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        input_id: u32,
        enabled: bool,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        if shared.config.find_input(input_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("input id {} not found", input_id)));
        }
        shared.state.ensure_input_gate(input_id).enabled = enabled;
        shared.state_dirty = true;
        Service::send_pw_cmd(&shared, PwCommand::SetInputGateEnabled { input_id, enabled });
        drop(shared);
        Self::input_dsp_changed(&emitter, input_id).await.ok();
        Ok(())
    }

    async fn set_input_gate(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        input_id: u32,
        threshold_db: f64,
        attack_ms: f64,
        release_ms: f64,
        hold_ms: f64,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        if shared.config.find_input(input_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("input id {} not found", input_id)));
        }
        let gate = shared.state.ensure_input_gate(input_id);
        gate.threshold_db = threshold_db;
        gate.attack_ms = attack_ms;
        gate.release_ms = release_ms;
        gate.hold_ms = hold_ms;
        shared.state_dirty = true;
        Service::send_pw_cmd(&shared, PwCommand::SetInputGate {
            input_id, threshold_db, attack_ms, release_ms, hold_ms,
        });
        drop(shared);
        Self::input_dsp_changed(&emitter, input_id).await.ok();
        Ok(())
    }

    async fn get_input_gate(&self, input_id: u32) -> zbus::fdo::Result<mixctl_core::GateInfo> {
        let shared = self.inner.lock().await;
        if shared.config.find_input(input_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("input id {} not found", input_id)));
        }
        let gate = shared.state.input_gate_state(input_id)
            .cloned()
            .unwrap_or_default();
        Ok(mixctl_core::GateInfo {
            enabled: gate.enabled,
            threshold_db: gate.threshold_db,
            attack_ms: gate.attack_ms,
            release_ms: gate.release_ms,
            hold_ms: gate.hold_ms,
        })
    }

    // -- DSP: De-esser (per input) --

    async fn set_input_deesser_enabled(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        input_id: u32,
        enabled: bool,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        if shared.config.find_input(input_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("input id {} not found", input_id)));
        }
        shared.state.ensure_input_deesser(input_id).enabled = enabled;
        shared.state_dirty = true;
        Service::send_pw_cmd(&shared, PwCommand::SetInputDeesserEnabled { input_id, enabled });
        drop(shared);
        Self::input_dsp_changed(&emitter, input_id).await.ok();
        Ok(())
    }

    async fn set_input_deesser(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        input_id: u32,
        frequency: f64,
        threshold_db: f64,
        ratio: f64,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        if shared.config.find_input(input_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("input id {} not found", input_id)));
        }
        let ds = shared.state.ensure_input_deesser(input_id);
        ds.frequency = frequency;
        ds.threshold_db = threshold_db;
        ds.ratio = ratio;
        shared.state_dirty = true;
        Service::send_pw_cmd(&shared, PwCommand::SetInputDeesser {
            input_id, frequency, threshold_db, ratio,
        });
        drop(shared);
        Self::input_dsp_changed(&emitter, input_id).await.ok();
        Ok(())
    }

    async fn get_input_deesser(&self, input_id: u32) -> zbus::fdo::Result<mixctl_core::DeesserInfo> {
        let shared = self.inner.lock().await;
        if shared.config.find_input(input_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("input id {} not found", input_id)));
        }
        let ds = shared.state.input_deesser_state(input_id)
            .cloned()
            .unwrap_or_default();
        Ok(mixctl_core::DeesserInfo {
            enabled: ds.enabled,
            frequency: ds.frequency,
            threshold_db: ds.threshold_db,
            ratio: ds.ratio,
        })
    }

    // -- DSP: Compressor (per output) --

    async fn set_output_compressor_enabled(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        output_id: u32,
        enabled: bool,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        if shared.config.find_output(output_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("output id {} not found", output_id)));
        }
        shared.state.ensure_output_compressor(output_id).enabled = enabled;
        shared.state_dirty = true;
        Service::send_pw_cmd(&shared, PwCommand::SetOutputCompressorEnabled { output_id, enabled });
        drop(shared);
        Self::output_dsp_changed(&emitter, output_id).await.ok();
        Ok(())
    }

    async fn set_output_compressor(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        output_id: u32,
        threshold_db: f64,
        ratio: f64,
        attack_ms: f64,
        release_ms: f64,
        makeup_gain_db: f64,
        knee_db: f64,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        if shared.config.find_output(output_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("output id {} not found", output_id)));
        }
        let comp = shared.state.ensure_output_compressor(output_id);
        comp.threshold_db = threshold_db;
        comp.ratio = ratio;
        comp.attack_ms = attack_ms;
        comp.release_ms = release_ms;
        comp.makeup_gain_db = makeup_gain_db;
        comp.knee_db = knee_db;
        shared.state_dirty = true;
        Service::send_pw_cmd(&shared, PwCommand::SetOutputCompressor {
            output_id, threshold_db, ratio, attack_ms, release_ms, makeup_gain_db, knee_db,
        });
        drop(shared);
        Self::output_dsp_changed(&emitter, output_id).await.ok();
        Ok(())
    }

    async fn get_output_compressor(&self, output_id: u32) -> zbus::fdo::Result<mixctl_core::CompressorInfo> {
        let shared = self.inner.lock().await;
        if shared.config.find_output(output_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("output id {} not found", output_id)));
        }
        let comp = shared.state.output_compressor_state(output_id)
            .cloned()
            .unwrap_or_default();
        Ok(mixctl_core::CompressorInfo {
            enabled: comp.enabled,
            threshold_db: comp.threshold_db,
            ratio: comp.ratio,
            attack_ms: comp.attack_ms,
            release_ms: comp.release_ms,
            makeup_gain_db: comp.makeup_gain_db,
            knee_db: comp.knee_db,
        })
    }

    // -- DSP: Limiter (per output) --

    async fn set_output_limiter_enabled(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        output_id: u32,
        enabled: bool,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        if shared.config.find_output(output_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("output id {} not found", output_id)));
        }
        shared.state.ensure_output_limiter(output_id).enabled = enabled;
        shared.state_dirty = true;
        Service::send_pw_cmd(&shared, PwCommand::SetOutputLimiterEnabled { output_id, enabled });
        drop(shared);
        Self::output_dsp_changed(&emitter, output_id).await.ok();
        Ok(())
    }

    async fn set_output_limiter(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        output_id: u32,
        ceiling_db: f64,
        release_ms: f64,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        if shared.config.find_output(output_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("output id {} not found", output_id)));
        }
        let lim = shared.state.ensure_output_limiter(output_id);
        lim.ceiling_db = ceiling_db;
        lim.release_ms = release_ms;
        shared.state_dirty = true;
        Service::send_pw_cmd(&shared, PwCommand::SetOutputLimiter {
            output_id, ceiling_db, release_ms,
        });
        drop(shared);
        Self::output_dsp_changed(&emitter, output_id).await.ok();
        Ok(())
    }

    async fn get_output_limiter(&self, output_id: u32) -> zbus::fdo::Result<mixctl_core::LimiterInfo> {
        let shared = self.inner.lock().await;
        if shared.config.find_output(output_id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("output id {} not found", output_id)));
        }
        let lim = shared.state.output_limiter_state(output_id)
            .cloned()
            .unwrap_or_default();
        Ok(mixctl_core::LimiterInfo {
            enabled: lim.enabled,
            ceiling_db: lim.ceiling_db,
            release_ms: lim.release_ms,
        })
    }

    // -- DSP: Noise suppression (stub) --

    async fn set_capture_noise_suppression(
        &self,
        _input_id: u32,
        _enabled: bool,
    ) -> zbus::fdo::Result<()> {
        Err(zbus::fdo::Error::NotSupported("noise suppression not yet implemented".into()))
    }

    async fn get_capture_noise_suppression(&self, _input_id: u32) -> zbus::fdo::Result<bool> {
        Ok(false)
    }

    // -- Profiles --

    async fn list_profiles(&self) -> zbus::fdo::Result<Vec<String>> {
        let profile_dir = profile_dir();
        let mut names = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&profile_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.path().file_stem() {
                    if entry.path().extension().map(|e| e == "toml").unwrap_or(false) {
                        names.push(name.to_string_lossy().into_owned());
                    }
                }
            }
        }
        names.sort();
        Ok(names)
    }

    async fn save_profile(&self, name: &str) -> zbus::fdo::Result<()> {
        let shared = self.inner.lock().await;
        let profile_dir = profile_dir();
        std::fs::create_dir_all(&profile_dir)
            .map_err(|e| zbus::fdo::Error::Failed(format!("create dir failed: {e}")))?;
        let path = profile_dir.join(format!("{name}.toml"));
        let toml = toml::to_string_pretty(&shared.state)
            .map_err(|e| zbus::fdo::Error::Failed(format!("serialize failed: {e}")))?;
        std::fs::write(&path, toml)
            .map_err(|e| zbus::fdo::Error::Failed(format!("write failed: {e}")))?;
        Ok(())
    }

    async fn load_profile(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        name: &str,
    ) -> zbus::fdo::Result<()> {
        let profile_dir = profile_dir();
        let path = profile_dir.join(format!("{name}.toml"));
        let toml_str = std::fs::read_to_string(&path)
            .map_err(|e| zbus::fdo::Error::Failed(format!("read failed: {e}")))?;
        let loaded_state: crate::state::StateFile = toml::from_str(&toml_str)
            .map_err(|e| zbus::fdo::Error::Failed(format!("parse failed: {e}")))?;

        let mut shared = self.inner.lock().await;
        // Merge loaded state: overwrite volumes, mutes, DSP but keep structural state
        shared.state.outputs = loaded_state.outputs;
        shared.state.routes = loaded_state.routes;
        shared.state.capture_volumes = loaded_state.capture_volumes;
        shared.state.input_eq = loaded_state.input_eq;
        shared.state.input_gate = loaded_state.input_gate;
        shared.state.input_deesser = loaded_state.input_deesser;
        shared.state.output_compressor = loaded_state.output_compressor;
        shared.state.output_limiter = loaded_state.output_limiter;

        // Reconcile to ensure state matches current config
        let config = shared.config.clone();
        shared.state.reconcile(&config);
        shared.state_dirty = true;

        drop(shared);
        Self::profile_changed(&emitter, name.to_string()).await.ok();
        // Also emit full refresh signals so all apps update
        Self::inputs_config_changed(&emitter).await.ok();
        Self::outputs_config_changed(&emitter).await.ok();
        Ok(())
    }

    async fn delete_profile(&self, name: &str) -> zbus::fdo::Result<()> {
        let profile_dir = profile_dir();
        let path = profile_dir.join(format!("{name}.toml"));
        std::fs::remove_file(&path)
            .map_err(|e| zbus::fdo::Error::Failed(format!("delete failed: {e}")))?;
        Ok(())
    }

    // -- Custom inputs --

    async fn list_custom_inputs(&self) -> zbus::fdo::Result<Vec<CustomInputInfo>> {
        let shared = self.inner.lock().await;
        let out: Vec<CustomInputInfo> = shared
            .config
            .custom_inputs
            .iter()
            .map(|ci| CustomInputInfo {
                id: ci.id(),
                name: ci.name.clone(),
                color: ci.color.clone(),
                custom_type: ci.custom_type.clone(),
                value: shared.state.custom_input_value(ci.id()),
            })
            .collect();
        Ok(out)
    }

    async fn add_custom_input(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        name: &str,
        color: &str,
        custom_type: &str,
        params_json: &str,
    ) -> zbus::fdo::Result<u32> {
        if name.is_empty() || name.len() > 50 {
            return Err(zbus::fdo::Error::InvalidArgs(
                "name must be 1-50 characters".into(),
            ));
        }
        if parse_hex_color(color).is_none() {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "invalid hex color '{}'",
                color
            )));
        }

        // Parse params from JSON
        let params: std::collections::HashMap<String, toml::Value> = if params_json.is_empty() {
            std::collections::HashMap::new()
        } else {
            let json_val: serde_json::Value = serde_json::from_str(params_json)
                .map_err(|e| zbus::fdo::Error::InvalidArgs(format!("invalid params JSON: {e}")))?;
            json_to_toml_map(&json_val)
                .map_err(|e| zbus::fdo::Error::InvalidArgs(format!("params conversion failed: {e}")))?
        };

        // Try creating the handler to validate the type and params
        let handler = crate::custom_inputs::create_handler(custom_type, &params)
            .map_err(|e| zbus::fdo::Error::Failed(format!("failed to create handler: {e}")))?;

        let mut shared = self.inner.lock().await;
        let id = shared.config.next_unused_id();

        // Read original value if handler supports it
        let original = if handler.supports_read() {
            match handler.read_current() {
                Ok(v) => v,
                Err(e) => {
                    warn!("custom input {}: failed to read original value: {e}", id);
                    50
                }
            }
        } else {
            50
        };

        shared.config.custom_inputs.push(crate::config::CustomInputConfig {
            id: Some(id),
            name: name.to_string(),
            color: color.to_string(),
            custom_type: custom_type.to_string(),
            restore_on_exit: true,
            params,
        });
        shared.state.set_custom_input_value(id, original);
        shared.config_dirty = true;
        shared.state_dirty = true;
        shared.custom_input_originals.insert(id, original);
        shared.custom_input_handlers.insert(id, handler);

        info!("added custom input {} '{}' (type={}, original={})", id, name, custom_type, original);

        drop(shared);
        Self::custom_input_changed(&emitter, id).await.ok();
        Ok(id)
    }

    async fn remove_custom_input(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        id: u32,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        let idx = shared
            .config
            .custom_inputs
            .iter()
            .position(|c| c.id() == id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("custom input id {} not found", id)))?;

        let restore = shared.config.custom_inputs[idx].restore_on_exit;

        // Restore original value if configured
        if restore {
            if let Some(&original) = shared.custom_input_originals.get(&id) {
                if let Some(handler) = shared.custom_input_handlers.get(&id) {
                    if let Err(e) = handler.apply(original) {
                        warn!("custom input {}: failed to restore original value: {e}", id);
                    }
                }
            }
        }

        shared.config.custom_inputs.remove(idx);
        shared.state.custom_input_values.remove(&id.to_string());
        shared.custom_input_handlers.remove(&id);
        shared.custom_input_originals.remove(&id);
        shared.config_dirty = true;
        shared.state_dirty = true;

        info!("removed custom input {}", id);

        drop(shared);
        Self::custom_input_changed(&emitter, id).await.ok();
        Ok(())
    }

    async fn get_custom_input_value(&self, id: u32) -> zbus::fdo::Result<u8> {
        let shared = self.inner.lock().await;
        if !shared.config.is_custom_input(id) {
            return Err(zbus::fdo::Error::Failed(format!(
                "custom input id {} not found",
                id
            )));
        }
        Ok(shared.state.custom_input_value(id))
    }

    async fn set_custom_input_value(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        id: u32,
        value: u8,
    ) -> zbus::fdo::Result<()> {
        let value = value.min(100);
        let mut shared = self.inner.lock().await;
        if !shared.config.is_custom_input(id) {
            return Err(zbus::fdo::Error::Failed(format!(
                "custom input id {} not found",
                id
            )));
        }

        // Apply via handler
        if let Some(handler) = shared.custom_input_handlers.get(&id) {
            // Check if this is a DSP parameter handler (special case)
            if let Some((_channel_id, _param, _min, _max)) = handler.as_dsp_parameter() {
                // DSP parameter: would need to issue PW commands here
                // For now, just log it
                let mapped = _min + (value as f64 / 100.0) * (_max - _min);
                info!(
                    "custom input {}: dsp_parameter channel={} param={} mapped_value={}",
                    id, _channel_id, _param, mapped
                );
            } else if let Err(e) = handler.apply(value) {
                warn!("custom input {}: handler.apply failed: {e}", id);
            }
        }

        shared.state.set_custom_input_value(id, value);
        shared.state_dirty = true;

        shared.signal_tx.send(ServiceSignal::CustomInputChanged { id }).ok();

        drop(shared);
        Self::custom_input_changed(&emitter, id).await.ok();
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
    async fn route_changed(
        emitter: &SignalEmitter<'_>,
        input_id: u32,
        output_id: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn page_changed(emitter: &SignalEmitter<'_>, page: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn audio_status_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn streams_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn app_rules_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn capture_devices_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn playback_devices_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn input_levels_changed(
        emitter: &SignalEmitter<'_>,
        levels: Vec<(u32, f64)>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn broadcast_levels_changed(
        emitter: &SignalEmitter<'_>,
        enabled: bool,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn config_section_changed(
        emitter: &SignalEmitter<'_>,
        section: String,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn component_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn input_dsp_changed(emitter: &SignalEmitter<'_>, input_id: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn output_dsp_changed(emitter: &SignalEmitter<'_>, output_id: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn profile_changed(emitter: &SignalEmitter<'_>, name: String) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn custom_input_changed(emitter: &SignalEmitter<'_>, id: u32) -> zbus::Result<()>;
}

/// Convert a JSON object to a HashMap<String, toml::Value> for custom input params.
fn json_to_toml_map(
    json: &serde_json::Value,
) -> anyhow::Result<std::collections::HashMap<String, toml::Value>> {
    let obj = json
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("expected JSON object"))?;
    let mut map = std::collections::HashMap::new();
    for (k, v) in obj {
        map.insert(k.clone(), json_val_to_toml(v)?);
    }
    Ok(map)
}

fn json_val_to_toml(v: &serde_json::Value) -> anyhow::Result<toml::Value> {
    match v {
        serde_json::Value::Bool(b) => Ok(toml::Value::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(toml::Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(toml::Value::Float(f))
            } else {
                anyhow::bail!("unsupported number type")
            }
        }
        serde_json::Value::String(s) => Ok(toml::Value::String(s.clone())),
        serde_json::Value::Object(obj) => {
            let mut table = toml::map::Map::new();
            for (k, v) in obj {
                table.insert(k.clone(), json_val_to_toml(v)?);
            }
            Ok(toml::Value::Table(table))
        }
        serde_json::Value::Array(arr) => {
            let vals: Result<Vec<_>, _> = arr.iter().map(json_val_to_toml).collect();
            Ok(toml::Value::Array(vals?))
        }
        serde_json::Value::Null => Ok(toml::Value::String(String::new())),
    }
}

fn profile_dir() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("mixctl")
        .join("profiles")
}

// Public wrappers for signal emission from outside the #[interface] block.
impl Service {
    pub async fn emit_audio_status_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()> {
        Self::audio_status_changed(emitter).await
    }

    pub async fn emit_capture_devices_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()> {
        Self::capture_devices_changed(emitter).await
    }

    pub async fn emit_playback_devices_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()> {
        Self::playback_devices_changed(emitter).await
    }

    pub async fn emit_streams_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()> {
        Self::streams_changed(emitter).await
    }

    pub async fn emit_input_levels_changed(
        emitter: &SignalEmitter<'_>,
        levels: Vec<(u32, f64)>,
    ) -> zbus::Result<()> {
        Self::input_levels_changed(emitter, levels).await
    }

    pub async fn emit_broadcast_levels_changed(
        emitter: &SignalEmitter<'_>,
        enabled: bool,
    ) -> zbus::Result<()> {
        Self::broadcast_levels_changed(emitter, enabled).await
    }

    pub async fn emit_config_section_changed(
        emitter: &SignalEmitter<'_>,
        section: String,
    ) -> zbus::Result<()> {
        Self::config_section_changed(emitter, section).await
    }

    pub async fn emit_component_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()> {
        Self::component_changed(emitter).await
    }

    #[allow(dead_code)]
    pub async fn emit_input_dsp_changed(
        emitter: &SignalEmitter<'_>,
        input_id: u32,
    ) -> zbus::Result<()> {
        Self::input_dsp_changed(emitter, input_id).await
    }

    #[allow(dead_code)]
    pub async fn emit_output_dsp_changed(
        emitter: &SignalEmitter<'_>,
        output_id: u32,
    ) -> zbus::Result<()> {
        Self::output_dsp_changed(emitter, output_id).await
    }

    #[allow(dead_code)]
    pub async fn emit_profile_changed(
        emitter: &SignalEmitter<'_>,
        name: String,
    ) -> zbus::Result<()> {
        Self::profile_changed(emitter, name).await
    }

    pub async fn emit_custom_input_changed(
        emitter: &SignalEmitter<'_>,
        id: u32,
    ) -> zbus::Result<()> {
        Self::custom_input_changed(emitter, id).await
    }
}
