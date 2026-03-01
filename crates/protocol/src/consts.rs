/// Beacn USB Vendor ID (shared across all Beacn devices)
pub const VENDOR_ID: u16 = 0x33ae;

/// Beacn Mix USB Product ID
pub const PRODUCT_ID_MIX: u16 = 0x0004;

/// Beacn Mix Create USB Product ID
pub const PRODUCT_ID_MIX_CREATE: u16 = 0x0007;

/// All supported product IDs
pub const PRODUCT_IDS: &[u16] = &[PRODUCT_ID_MIX, PRODUCT_ID_MIX_CREATE];

/// Interrupt OUT endpoint (1024 bytes max)
pub const EP_OUT: u8 = 0x03;

/// Interrupt IN endpoint (64 bytes max)
pub const EP_IN: u8 = 0x83;

/// USB interface number
pub const INTERFACE: u8 = 0;

/// USB alternate setting
pub const ALT_SETTING: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceType {
    Mix,
    MixCreate,
}

impl DeviceType {
    pub fn from_pid(pid: u16) -> Option<Self> {
        match pid {
            PRODUCT_ID_MIX => Some(DeviceType::Mix),
            PRODUCT_ID_MIX_CREATE => Some(DeviceType::MixCreate),
            _ => None,
        }
    }

    pub fn pid(self) -> u16 {
        match self {
            DeviceType::Mix => PRODUCT_ID_MIX,
            DeviceType::MixCreate => PRODUCT_ID_MIX_CREATE,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            DeviceType::Mix => "Beacn Mix",
            DeviceType::MixCreate => "Beacn Mix Create",
        }
    }
}

impl core::fmt::Display for DeviceType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.name())
    }
}
