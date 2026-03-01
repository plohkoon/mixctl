use core::fmt;

pub const INIT_PAYLOAD: [u8; 4] = [0x00, 0x00, 0x00, 0x01];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitStep {
    ClaimInterface,
    ClearHaltIn,
    SendInit,
    ReadVersion,
}

impl fmt::Display for InitStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InitStep::ClaimInterface => write!(f, "claim interface"),
            InitStep::ClearHaltIn => write!(f, "clear halt on IN endpoint"),
            InitStep::SendInit => write!(f, "send init payload"),
            InitStep::ReadVersion => write!(f, "read version response"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionInfo {
    pub raw: [u8; 64],
}

impl VersionInfo {
    pub fn new(raw: [u8; 64]) -> Self {
        Self { raw }
    }
}

impl fmt::Display for VersionInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, chunk) in self.raw.chunks(16).enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{:04x}:  ", i * 16)?;
            for (j, byte) in chunk.iter().enumerate() {
                if j > 0 {
                    write!(f, " ")?;
                }
                write!(f, "{:02x}", byte)?;
            }
        }
        Ok(())
    }
}

pub fn parse_version_response(buf: &[u8; 64]) -> Option<VersionInfo> {
    // A valid version response has a non-zero first byte
    if buf.iter().all(|&b| b == 0) {
        return None;
    }
    Some(VersionInfo::new(*buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_payload_value() {
        assert_eq!(INIT_PAYLOAD, [0x00, 0x00, 0x00, 0x01]);
    }

    #[test]
    fn parse_version_all_zeros_returns_none() {
        let buf = [0u8; 64];
        assert_eq!(parse_version_response(&buf), None);
    }

    #[test]
    fn parse_version_valid_response() {
        let mut buf = [0u8; 64];
        buf[0] = 0x01;
        buf[1] = 0x02;
        buf[2] = 0x03;
        let info = parse_version_response(&buf).unwrap();
        assert_eq!(info.raw[0], 0x01);
        assert_eq!(info.raw[1], 0x02);
        assert_eq!(info.raw[2], 0x03);
    }

    #[test]
    fn version_info_display() {
        let mut buf = [0u8; 64];
        buf[0] = 0xAB;
        buf[15] = 0xCD;
        let info = VersionInfo::new(buf);
        let display = format!("{}", info);
        assert!(display.contains("ab"));
        assert!(display.contains("cd"));
    }
}
