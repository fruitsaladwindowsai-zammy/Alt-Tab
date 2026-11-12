//! Depth and fog calculations matching Fathom's temporal model.
//!
//! A window `d` fathoms down lost focus `30 * (2^d - 1)` seconds ago:
//! `depth = log2(1 + seconds_since_focus / 30)`, clamped to `[0, 8]`.

pub const TIME_UNIT_SECONDS: f64 = 30.0;
pub const MAX_DEPTH: f64 = 8.0;
pub const FOG_PER_DEPTH: f64 = 0.08;

/// Clamps depth into `[0.0, MAX_DEPTH]`. NaN maps to `MAX_DEPTH`.
pub fn clamp_depth(depth: f64) -> f64 {
    if depth.is_nan() {
        return MAX_DEPTH;
    }
    depth.clamp(0.0, MAX_DEPTH)
}

/// Converts elapsed seconds since last focus into fathoms (`[0.0, 8.0]`).
pub fn depth_for_seconds(seconds: f64) -> f64 {
    if seconds.is_nan() || !seconds.is_finite() {
        return MAX_DEPTH;
    }
    if seconds <= 0.0 {
        return 0.0;
    }
    clamp_depth((1.0 + seconds / TIME_UNIT_SECONDS).log2())
}

/// Fog opacity contribution for a given depth (`0.0` at surface to `0.64` at 2h+).
pub fn fog_for_depth(depth: f64) -> f64 {
    clamp_depth(depth) * FOG_PER_DEPTH
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_curve_matches_spec() {
        assert_eq!(depth_for_seconds(0.0), 0.0);
        assert_eq!(depth_for_seconds(-5.0), 0.0);
        assert!((depth_for_seconds(30.0) - 1.0).abs() < 1e-6);
        assert!((depth_for_seconds(90.0) - 2.0).abs() < 1e-6);
        assert!((depth_for_seconds(45.0) - 1.321928).abs() < 1e-4);
        assert_eq!(depth_for_seconds(f64::INFINITY), MAX_DEPTH);
        assert_eq!(depth_for_seconds(f64::NAN), MAX_DEPTH);
        assert_eq!(depth_for_seconds(100_000.0), MAX_DEPTH);
    }

    #[test]
    fn fog_curve_matches_spec() {
        assert_eq!(fog_for_depth(0.0), 0.0);
        assert!((fog_for_depth(8.0) - 0.64).abs() < 1e-6);
    }
}
