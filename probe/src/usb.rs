use std::time::Duration;

use anyhow::{bail, Context, Result};
use mixctl_protocol::consts::*;
use mixctl_protocol::init::INIT_PAYLOAD;
use mixctl_protocol::DeviceType;
use rusb::{DeviceHandle, GlobalContext};
use tracing::{debug, info};

pub struct Device {
    handle: DeviceHandle<GlobalContext>,
    has_kernel_driver: bool,
    pub device_type: DeviceType,
}

impl Device {
    pub fn open() -> Result<Self> {
        // Try each supported PID until we find a device
        let (handle, device_type) = PRODUCT_IDS
            .iter()
            .find_map(|&pid| {
                let handle = rusb::open_device_with_vid_pid(VENDOR_ID, pid)?;
                let dt = DeviceType::from_pid(pid)?;
                Some((handle, dt))
            })
            .context("no Beacn Mix or Mix Create found (VID=0x33ae)")?;

        info!("found {}", device_type);

        let has_kernel_driver = handle
            .kernel_driver_active(INTERFACE)
            .unwrap_or(false);

        if has_kernel_driver {
            debug!("detaching kernel driver from interface {}", INTERFACE);
            handle
                .detach_kernel_driver(INTERFACE)
                .context("failed to detach kernel driver")?;
        }

        handle
            .claim_interface(INTERFACE)
            .context("failed to claim interface")?;
        info!("claimed interface {}", INTERFACE);

        handle
            .set_alternate_setting(INTERFACE, ALT_SETTING)
            .context("failed to set alternate setting")?;
        debug!("set alt setting {}", ALT_SETTING);

        handle
            .clear_halt(EP_IN)
            .context("failed to clear halt on IN endpoint")?;
        debug!("cleared halt on EP_IN 0x{:02x}", EP_IN);

        // Send init payload
        let written = handle
            .write_interrupt(EP_OUT, &INIT_PAYLOAD, Duration::from_secs(1))
            .context("failed to send init payload")?;
        debug!("sent init payload ({} bytes)", written);

        // Read version response
        let mut buf = [0u8; 64];
        match handle.read_interrupt(EP_IN, &mut buf, Duration::from_secs(1)) {
            Ok(n) => {
                info!("init response: {} bytes", n);
                if let Some(ver) = mixctl_protocol::init::parse_version_response(&buf) {
                    info!("version info:\n{}", ver);
                }
            }
            Err(e) => {
                debug!("no init response (may be normal): {}", e);
            }
        }

        Ok(Self {
            handle,
            has_kernel_driver,
            device_type,
        })
    }

    pub fn write_command(&self, data: &[u8]) -> Result<usize> {
        let n = self
            .handle
            .write_interrupt(EP_OUT, data, Duration::from_secs(1))
            .context("write_command failed")?;
        Ok(n)
    }

    pub fn write_raw(&self, data: &[u8]) -> Result<usize> {
        let n = self
            .handle
            .write_interrupt(EP_OUT, data, Duration::from_secs(1))
            .context("write_raw failed")?;
        Ok(n)
    }

    pub fn write_raw_timeout(&self, data: &[u8], timeout: Duration) -> Result<usize> {
        let n = self
            .handle
            .write_interrupt(EP_OUT, data, timeout)
            .context("write_raw failed")?;
        Ok(n)
    }

    pub fn read(&self, timeout: Duration) -> Result<Vec<u8>> {
        let mut buf = [0u8; 64];
        let n = self
            .handle
            .read_interrupt(EP_IN, &mut buf, timeout)
            .context("read failed")?;
        Ok(buf[..n].to_vec())
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        let _ = self.handle.release_interface(INTERFACE);
        if self.has_kernel_driver {
            let _ = self.handle.attach_kernel_driver(INTERFACE);
        }
    }
}

#[derive(Debug)]
pub struct DiscoveredDevice {
    pub device_type: DeviceType,
    pub bus: u8,
    pub address: u8,
}

pub fn discover() -> Result<Vec<DiscoveredDevice>> {
    let mut results = Vec::new();
    for device in rusb::devices()?.iter() {
        let desc = device.device_descriptor()?;
        if desc.vendor_id() == VENDOR_ID {
            if let Some(device_type) = DeviceType::from_pid(desc.product_id()) {
                results.push(DiscoveredDevice {
                    device_type,
                    bus: device.bus_number(),
                    address: device.address(),
                });
            }
        }
    }
    if results.is_empty() {
        bail!("no Beacn devices found");
    }
    Ok(results)
}
