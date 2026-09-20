use std::f64::consts::PI;

pub const TAU: f64 = PI * 2.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };
}

pub fn deg(degrees: f64) -> f64 {
    degrees.to_radians()
}

pub fn aim(origin_x: f64, origin_y: f64, target_x: f64, target_y: f64) -> f64 {
    (target_y - origin_y).atan2(target_x - origin_x)
}

pub fn quantize(angle: f64, step: f64) -> f64 {
    if !angle.is_finite() || !step.is_finite() || step <= 0.0 {
        return f64::NAN;
    }
    (angle / step).round() * step
}

pub fn polar(angle: f64, radius: f64) -> Vec2 {
    Vec2 {
        x: angle.cos() * radius,
        y: angle.sin() * radius,
    }
}

pub fn lerp(a: f64, b: f64, amount: f64) -> f64 {
    a + (b - a) * amount
}

pub fn clamp(value: f64, min: f64, max: f64) -> f64 {
    if !value.is_finite() || !min.is_finite() || !max.is_finite() {
        return f64::NAN;
    }
    value.clamp(min.min(max), min.max(max))
}

/// A small integer hash used as a stateless random source.
///
/// Every input tuple maps to the same value in `[0, 1)`.  No mutable RNG state
/// is used, so seeking or rendering a frame in isolation cannot alter a later
/// result.
pub fn deterministic_rand(seed: i64, wave: i64, index: i64, salt: i64) -> f64 {
    let mut value = (seed as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    value ^= (wave as u64)
        .rotate_left(17)
        .wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= (index as u64)
        .rotate_left(31)
        .wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= (salt as u64)
        .rotate_left(47)
        .wrapping_mul(0xD6E8_FEB8_6659_FD93);

    // SplitMix64 finalizer.
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;

    (value as f64) / ((u64::MAX as f64) + 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aim_points_at_target() {
        assert!((aim(0.0, 0.0, 1.0, 1.0) - PI / 4.0).abs() < 1e-12);
        assert!((aim(10.0, -2.0, 13.0, 2.0) - 4.0_f64.atan2(3.0)).abs() < 1e-12);
    }

    #[test]
    fn degrees_are_converted_to_radians() {
        assert!((deg(180.0) - PI).abs() < 1e-12);
        assert!((deg(90.0) - PI / 2.0).abs() < 1e-12);
    }

    #[test]
    fn quantize_rounds_to_nearest_step() {
        let result = quantize(deg(43.2), deg(5.625));
        assert!((result - deg(45.0)).abs() < 1e-12);
    }

    #[test]
    fn quantize_is_stable_until_crossing_a_boundary() {
        let step = deg(5.625);
        let below_boundary = quantize(deg(42.18), step);
        let above_boundary = quantize(deg(42.19), step);

        assert!((below_boundary - deg(39.375)).abs() < 1e-12);
        assert!((above_boundary - deg(45.0)).abs() < 1e-12);
        assert!(quantize(0.0, 0.0).is_nan());
    }

    #[test]
    fn polar_matches_unit_circle() {
        let result = polar(PI / 2.0, 10.0);
        assert!(result.x.abs() < 1e-12);
        assert!((result.y - 10.0).abs() < 1e-12);
    }

    #[test]
    fn random_is_repeatable_and_bounded() {
        let a = deterministic_rand(42, 3, 7, 9);
        assert_eq!(a, deterministic_rand(42, 3, 7, 9));
        assert_ne!(a, deterministic_rand(42, 3, 7, 10));
        assert!((0.0..1.0).contains(&a));
    }
}
