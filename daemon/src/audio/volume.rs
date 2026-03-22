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
#[allow(dead_code)]
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

    #[test]
    fn combine_pw_volume_both_muted_is_zero() {
        assert_eq!(combine_pw_volume(80, true, 90, true), 0.0);
    }

    #[test]
    fn combine_pw_volume_route_muted_is_zero() {
        assert_eq!(combine_pw_volume(80, true, 90, false), 0.0);
    }

    #[test]
    fn combine_pw_volume_output_muted_is_zero() {
        assert_eq!(combine_pw_volume(80, false, 90, true), 0.0);
    }

    #[test]
    fn combine_pw_volume_neither_muted_is_product() {
        let result = combine_pw_volume(100, false, 100, false);
        let expected = u8_to_pw_volume(100) * u8_to_pw_volume(100);
        assert_eq!(result, expected);
        assert_eq!(result, 1.0);

        let result2 = combine_pw_volume(50, false, 80, false);
        let expected2 = u8_to_pw_volume(50) * u8_to_pw_volume(80);
        assert!((result2 - expected2).abs() < f32::EPSILON);
    }

    #[test]
    fn combine_pw_volume_zero_volume_not_muted() {
        // volume 0 but not muted -> should be 0.0 (from the cubic scaling)
        assert_eq!(combine_pw_volume(0, false, 100, false), 0.0);
        assert_eq!(combine_pw_volume(100, false, 0, false), 0.0);
    }
}
