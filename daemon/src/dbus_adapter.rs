use mixctl_core::{ChannelInfo, parse_hex_color};
use zbus::interface;
use zbus::object_server::SignalEmitter;

use crate::service::Service;

#[interface(name = "dev.greghuber.MixCtl1")]
impl Service {
    async fn ping(&self) -> zbus::fdo::Result<String> {
        Ok("pong".to_string())
    }

    async fn list_channels(&self) -> zbus::fdo::Result<Vec<ChannelInfo>> {
        let shared = self.inner.lock().await;
        let mut out = Vec::with_capacity(shared.config.channels.len());
        for cfg in &shared.config.channels {
            let st = shared.state.channel_state(cfg.id())
                .cloned()
                .unwrap_or_default();
            out.push(Service::build_channel_info(cfg, &st));
        }
        Ok(out)
    }

    async fn get_channel(&self, id: u32) -> zbus::fdo::Result<ChannelInfo> {
        let shared = self.inner.lock().await;
        let cfg = shared.config.find_channel(id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("channel id {} not found", id)))?;
        let st = shared.state.channel_state(id)
            .cloned()
            .unwrap_or_default();
        Ok(Service::build_channel_info(cfg, &st))
    }

    async fn add_channel(
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
        shared.config.channels.push(crate::config::ChannelConfig {
            id: Some(id),
            name: name.to_string(),
            color: color.to_string(),
        });
        shared.state.ensure_channel(id);
        shared.config_dirty = true;
        shared.state_dirty = true;
        drop(shared);
        Self::channels_config_changed(&emitter).await.ok();
        Ok(id)
    }

    async fn remove_channel(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        id: u32,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        let idx = shared.config.channels.iter().position(|c| c.id() == id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("channel id {} not found", id)))?;
        shared.config.channels.remove(idx);
        shared.state.remove_channel(id);
        // Clamp page if we reduced pages
        let max = shared.config.max_page();
        if shared.state.current_page > max {
            shared.state.current_page = max;
        }
        shared.config_dirty = true;
        shared.state_dirty = true;
        drop(shared);
        Self::channels_config_changed(&emitter).await.ok();
        Ok(())
    }

    async fn move_channel(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        id: u32,
        position: u32,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        let from = shared.config.channels.iter().position(|c| c.id() == id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("channel id {} not found", id)))?;
        let len = shared.config.channels.len();
        let to = position as usize;
        if to >= len {
            return Err(zbus::fdo::Error::InvalidArgs(
                format!("position {} out of range (0..{})", position, len - 1),
            ));
        }
        let ch = shared.config.channels.remove(from);
        shared.config.channels.insert(to, ch);
        shared.config_dirty = true;
        drop(shared);
        Self::channels_config_changed(&emitter).await.ok();
        Ok(())
    }

    async fn set_channel_name(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        id: u32,
        name: &str,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        let cfg = shared.config.find_channel_mut(id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("channel id {} not found", id)))?;
        cfg.name = name.to_string();
        shared.config_dirty = true;
        drop(shared);
        Self::channels_config_changed(&emitter).await.ok();
        Ok(())
    }

    async fn set_channel_color(
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
        let cfg = shared.config.find_channel_mut(id)
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("channel id {} not found", id)))?;
        cfg.color = color.to_string();
        shared.config_dirty = true;
        drop(shared);
        Self::channels_config_changed(&emitter).await.ok();
        Ok(())
    }

    async fn set_channel_mute(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        id: u32,
        muted: bool,
    ) -> zbus::fdo::Result<()> {
        let mut shared = self.inner.lock().await;
        if shared.config.find_channel(id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("channel id {} not found", id)));
        }
        shared.state.set_muted(id, muted);
        shared.state_dirty = true;
        drop(shared);
        Self::channel_state_changed(&emitter, id).await.ok();
        Ok(())
    }

    async fn set_channel_volume(
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
        if shared.config.find_channel(id).is_none() {
            return Err(zbus::fdo::Error::Failed(format!("channel id {} not found", id)));
        }
        shared.state.set_volume(id, volume);
        shared.state_dirty = true;
        drop(shared);
        Self::channel_state_changed(&emitter, id).await.ok();
        Ok(())
    }

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

    // Signals
    #[zbus(signal)]
    async fn channel_state_changed(emitter: &SignalEmitter<'_>, id: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn channels_config_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn page_changed(emitter: &SignalEmitter<'_>, page: u32) -> zbus::Result<()>;
}
