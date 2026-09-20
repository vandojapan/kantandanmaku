use crate::danmaku::{aim, clamp, deg, deterministic_rand, lerp, polar, quantize, Vec2, TAU};
use rhai::{Dynamic, Engine};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct AimContext {
    pub origin_x: f64,
    pub origin_y: f64,
    pub target_x: f64,
    pub target_y: f64,
}

pub(crate) fn build_engine(max_operations: u64, aim_context: Arc<Mutex<AimContext>>) -> Engine {
    let mut engine = Engine::new();
    engine
        .set_max_operations(max_operations)
        .set_max_call_levels(32)
        .set_max_expr_depths(64, 32);

    engine.register_type_with_name::<Vec2>("Vec2");
    engine.register_get("x", |value: &mut Vec2| value.x);
    engine.register_get("y", |value: &mut Vec2| value.y);

    engine.register_fn("sin", f64::sin);
    engine.register_fn("cos", f64::cos);
    engine.register_fn("sin", |value: i64| (value as f64).sin());
    engine.register_fn("cos", |value: i64| (value as f64).cos());
    engine.register_fn("atan2", |y: f64, x: f64| y.atan2(x));
    engine.register_fn("floor", f64::floor);
    engine.register_fn("ceil", f64::ceil);
    engine.register_fn("abs", f64::abs);
    engine.register_fn("sqrt", f64::sqrt);

    engine.register_fn("deg", deg);
    engine.register_fn("deg", |degrees: i64| deg(degrees as f64));
    engine.register_fn("quantize", quantize);
    engine.register_fn("polar", polar);
    engine.register_fn("lerp", lerp);
    engine.register_fn("clamp", clamp);
    engine.register_fn("ring", ring_value as fn(f64, f64) -> f64);
    engine.register_fn("ring", |slot: i64, count: i64| {
        ring_value(slot as f64, count as f64)
    });
    engine.register_fn("ring", |slot: f64, count: i64| {
        ring_value(slot, count as f64)
    });
    engine.register_fn("ring", |slot: i64, count: f64| {
        ring_value(slot as f64, count)
    });

    let aim_context_for_function = Arc::clone(&aim_context);
    engine.register_fn("aim", move || {
        let context = aim_context_for_function
            .lock()
            .map(|guard| *guard)
            .unwrap_or_default();
        aim(
            context.origin_x,
            context.origin_y,
            context.target_x,
            context.target_y,
        )
    });

    engine.register_fn(
        "rand",
        |seed: Dynamic, wave: Dynamic, i: Dynamic, salt: Dynamic| {
            deterministic_rand(
                dynamic_number(seed).round() as i64,
                dynamic_number(wave).round() as i64,
                dynamic_number(i).round() as i64,
                dynamic_number(salt).round() as i64,
            )
        },
    );

    engine
}

fn ring_value(slot: f64, count: f64) -> f64 {
    if count <= 0.0 {
        f64::NAN
    } else {
        TAU * slot / count
    }
}

fn dynamic_number(value: Dynamic) -> f64 {
    if value.is_float() {
        value.as_float().unwrap_or(f64::NAN)
    } else if value.is_int() {
        value
            .as_int()
            .map(|integer| integer as f64)
            .unwrap_or(f64::NAN)
    } else {
        f64::NAN
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_four_functions_are_callable_from_rhai() {
        let aim_context = Arc::new(Mutex::new(AimContext {
            origin_x: 10.0,
            origin_y: -2.0,
            target_x: 13.0,
            target_y: 2.0,
        }));
        let engine = build_engine(1_000, aim_context);

        let aimed = engine.eval::<f64>("aim()").unwrap();
        assert!((aimed - 4.0_f64.atan2(3.0)).abs() < 1e-12);

        let radians = engine.eval::<f64>("deg(180)").unwrap();
        assert!((radians - std::f64::consts::PI).abs() < 1e-12);

        let quantized = engine
            .eval::<f64>("quantize(deg(43.2), deg(5.625))")
            .unwrap();
        assert!((quantized - deg(45.0)).abs() < 1e-12);

        let position = engine.eval::<Vec2>("polar(deg(90), 10.0)").unwrap();
        assert!(position.x.abs() < 1e-12);
        assert!((position.y - 10.0).abs() < 1e-12);
    }
}
