/// Convert a u8 volume (0-100) to PipeWire's f32 cubic volume scale.
/// 50% slider maps to approximately -18dB (perceptually "half volume").
pub fn u8_to_pw_volume(v: u8) -> f32 {
    let linear = (v as f32) / 100.0;
    linear * linear * linear
}

/// Compute combined PW volume from route and output volumes/mute states.
/// Output: route_vol * output_vol (both cubic-scaled), or 0.0 if either is muted.
pub fn combine_pw_volume(route_vol: u8, route_muted: bool, output_vol: u8, output_muted: bool) -> f32 {
    if route_muted || output_muted {
        0.0
    } else {
        u8_to_pw_volume(route_vol) * u8_to_pw_volume(output_vol)
    }
}

/// Convert PipeWire's f32 cubic volume back to u8 (0-100).
pub fn pw_volume_to_u8(v: f32) -> u8 {
    (v.cbrt() * 100.0).round().clamp(0.0, 100.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        for v in 0..=100u8 {
            let pw = u8_to_pw_volume(v);
            let back = pw_volume_to_u8(pw);
            assert_eq!(back, v, "roundtrip failed for {v}");
        }
    }

    #[test]
    fn boundary_values() {
        assert_eq!(u8_to_pw_volume(0), 0.0);
        assert_eq!(u8_to_pw_volume(100), 1.0);
        assert_eq!(pw_volume_to_u8(0.0), 0);
        assert_eq!(pw_volume_to_u8(1.0), 100);
    }

    #[test]
    fn midpoint_is_quiet() {
        let mid = u8_to_pw_volume(50);
        assert!(mid < 0.15, "50% should be ~0.125 (cubic), got {mid}");
    }
}
