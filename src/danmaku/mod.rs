//! Deterministic, frame-independent danmaku model.

mod bullet;
mod math;
mod runtime;
mod wave;

pub use bullet::{Bullet, BulletState};
pub use math::{aim, clamp, deg, deterministic_rand, lerp, polar, quantize, Vec2, TAU};
pub use runtime::{
    evaluate, evaluate_with_options, ResolvedBullet, RuntimeInput, RuntimeLimits, RuntimeOptions,
    RuntimeResult,
};
pub use wave::{active_wave_count, wave_at, WaveTime};
