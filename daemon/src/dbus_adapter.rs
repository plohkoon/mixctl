use mixctl_core::{
    AppRuleInfo, CaptureDeviceInfo, InputInfo, OutputInfo, RouteInfo, StreamInfo, parse_hex_color,
};
use zbus::interface;
use zbus::object_server::SignalEmitter;

use mixctl_core::config_sections::{AppletConfig, BeacnConfig, CliConfig, UiConfig};

use crate::audio::PwCommand;
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
        if parse_hex_color(color).is_none() {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "invalid hex color '{}'",
                color
            )));
        }
        let mut shared = self.inner.lock().await;
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
        if parse_hex_color(color).is_none() {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "invalid hex color '{}'",
                color
            )));
        }
        let mut shared = self.inner.lock().await;
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
        if volume > 100 {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "volume {} exceeds maximum of 100",
                volume
            )));
        }
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
        if volume > 100 {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "volume {} exceeds maximum of 100",
                volume
            )));
        }
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
            drop(shared);
            Self::app_rules_changed(&emitter).await.ok();
        } else {
            shared.signal_tx.send(ServiceSignal::StreamsChanged).ok();
            drop(shared);
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
        if parse_hex_color(color).is_none() {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "invalid hex color '{}'",
                color
            )));
        }

        let mut shared = self.inner.lock().await;
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
        Ok(())
    }

    // -- Capture volume/mute --

    async fn set_capture_volume(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        id: u32,
        volume: u8,
    ) -> zbus::fdo::Result<()> {
        if volume > 100 {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "volume {} exceeds maximum of 100",
                volume
            )));
        }
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
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "page {} out of range (max {})",
                page, max
            )));
        }
        shared.state.current_page = page;
        shared.state_dirty = true;

        drop(shared);
        Self::page_changed(&emitter, page).await.ok();
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
}

// Public wrappers for signal emission from outside the #[interface] block.
impl Service {
    pub async fn emit_audio_status_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()> {
        Self::audio_status_changed(emitter).await
    }

    pub async fn emit_capture_devices_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()> {
        Self::capture_devices_changed(emitter).await
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
}
