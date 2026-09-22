use super::api::{build_engine, AimContext};
use crate::danmaku::{BulletState, Vec2};
use rhai::{Dynamic, Engine, Scope, AST};
use std::fmt;
use std::sync::{Arc, Mutex};

pub const MAX_SCRIPT_SOURCE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptError {
    ParseError(String),
    RuntimeError(String),
    InvalidValue(String),
}

impl fmt::Display for ScriptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ParseError(message) => write!(formatter, "Parse Error（構文エラー）: {message}"),
            Self::RuntimeError(message) => {
                write!(formatter, "Runtime Error（実行時エラー）: {message}")
            }
            Self::InvalidValue(message) => write!(formatter, "Invalid Value（不正値）: {message}"),
        }
    }
}

impl std::error::Error for ScriptError {}

#[derive(Debug, Clone, Copy)]
pub struct ScriptContext {
    pub i: u32,
    pub count: u32,
    pub wave: i64,
    pub t: f64,
    pub age: f64,
    pub origin_x: f64,
    pub origin_y: f64,
    pub target_x: f64,
    pub target_y: f64,
    pub seed: i64,
    pub spread_angle: f64,
    pub density: f64,
    pub angle: f64,
    pub speed: f64,
    pub life: f64,
}

impl ScriptContext {
    pub fn spawn(
        i: u32,
        count: u32,
        wave: i64,
        t: f64,
        input: crate::danmaku::RuntimeInput,
        options: crate::danmaku::RuntimeOptions,
    ) -> Self {
        Self {
            i,
            count,
            wave,
            t,
            age: 0.0,
            origin_x: input.origin_x,
            origin_y: input.origin_y,
            target_x: input.target_x,
            target_y: input.target_y,
            seed: input.seed,
            spread_angle: options.spread_angle_degrees.to_radians(),
            density: options.density,
            angle: 0.0,
            speed: 0.0,
            life: 0.0,
        }
    }

    pub fn motion(
        i: u32,
        count: u32,
        wave: i64,
        t: f64,
        age: f64,
        input: crate::danmaku::RuntimeInput,
        options: crate::danmaku::RuntimeOptions,
        spawn: SpawnValues,
    ) -> Self {
        Self {
            i,
            count,
            wave,
            t,
            age,
            origin_x: input.origin_x,
            origin_y: input.origin_y,
            target_x: input.target_x,
            target_y: input.target_y,
            seed: input.seed,
            spread_angle: options.spread_angle_degrees.to_radians(),
            density: options.density,
            angle: spawn.angle,
            speed: spawn.speed,
            life: spawn.life,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SpawnValues {
    pub count: f64,
    pub angle: f64,
    pub speed: f64,
    pub life: f64,
}

pub struct CompiledScript {
    spawn: AST,
    motion: AST,
}

pub struct ScriptEngine;

impl ScriptEngine {
    pub fn compile(source: &str) -> Result<CompiledScript, ScriptError> {
        if source.len() > MAX_SCRIPT_SOURCE_BYTES {
            return Err(ScriptError::InvalidValue(format!(
                "script source exceeds the maximum of {MAX_SCRIPT_SOURCE_BYTES} bytes"
            )));
        }
        let spawn_source = extract_block(source, "spawn")?;
        let motion_source = extract_block(source, "motion")?;
        // Parsing is done with the same feature set and grammar as runtime
        // evaluation, so malformed custom scripts fail before any rendering.
        let mut parser = Engine::new();
        parser.set_max_expr_depths(64, 32);
        let spawn = parser
            .compile(&spawn_source)
            .map_err(|error| ScriptError::ParseError(error.to_string()))?;
        let motion = parser
            .compile(&motion_source)
            .map_err(|error| ScriptError::ParseError(error.to_string()))?;
        Ok(CompiledScript { spawn, motion })
    }
}

pub struct ScriptEvaluator {
    engine: Engine,
    aim_context: Arc<Mutex<AimContext>>,
}

impl CompiledScript {
    pub fn evaluator(&self, max_operations: u64) -> ScriptEvaluator {
        let aim_context = Arc::new(Mutex::new(AimContext::default()));
        ScriptEvaluator {
            engine: build_engine(max_operations, Arc::clone(&aim_context)),
            aim_context,
        }
    }

    pub fn eval_spawn(
        &self,
        evaluator: &mut ScriptEvaluator,
        context: ScriptContext,
    ) -> Result<SpawnValues, ScriptError> {
        let mut scope = make_scope(context);
        evaluator.set_aim_context(context);
        let _ = evaluator
            .engine
            .eval_ast_with_scope::<Dynamic>(&mut scope, &self.spawn)
            .map_err(|error| ScriptError::RuntimeError(error.to_string()))?;

        let result = SpawnValues {
            count: read_number(&scope, "count")?,
            angle: read_number(&scope, "angle")?,
            speed: read_number(&scope, "speed")?,
            life: read_number(&scope, "life")?,
        };
        validate_spawn(result)
    }

    pub fn eval_motion(
        &self,
        evaluator: &mut ScriptEvaluator,
        context: ScriptContext,
    ) -> Result<BulletState, ScriptError> {
        let mut scope = make_scope(context);
        evaluator.set_aim_context(context);
        let _ = evaluator
            .engine
            .eval_ast_with_scope::<Dynamic>(&mut scope, &self.motion)
            .map_err(|error| ScriptError::RuntimeError(error.to_string()))?;

        let state = BulletState {
            x: read_number(&scope, "x")?,
            y: read_number(&scope, "y")?,
            rotation: read_number(&scope, "rotation")?,
            scale_x: read_number(&scope, "scale_x")?,
            scale_y: read_number(&scope, "scale_y")?,
            alpha: read_number(&scope, "alpha")?,
        };
        if [
            state.x,
            state.y,
            state.rotation,
            state.scale_x,
            state.scale_y,
            state.alpha,
        ]
        .iter()
        .all(|value| value.is_finite())
            && state.scale_x >= 0.0
            && state.scale_y >= 0.0
            && (0.0..=1.0).contains(&state.alpha)
        {
            Ok(state)
        } else {
            Err(ScriptError::InvalidValue(
                "motion returned NaN, Infinity, negative scale, or alpha outside 0..=1".to_string(),
            ))
        }
    }
}

impl ScriptEvaluator {
    fn set_aim_context(&mut self, context: ScriptContext) {
        if let Ok(mut aim_context) = self.aim_context.lock() {
            aim_context.origin_x = context.origin_x;
            aim_context.origin_y = context.origin_y;
            aim_context.target_x = context.target_x;
            aim_context.target_y = context.target_y;
        }
    }
}

fn make_scope(context: ScriptContext) -> Scope<'static> {
    let mut scope = Scope::new();
    scope.push("i", context.i as f64);
    scope.push("count", context.count as f64);
    scope.push("wave", context.wave as f64);
    scope.push("t", context.t);
    scope.push("age", context.age);
    scope.push("origin_x", context.origin_x);
    scope.push("origin_y", context.origin_y);
    scope.push("target_x", context.target_x);
    scope.push("target_y", context.target_y);
    scope.push("seed", context.seed as f64);
    scope.push("spread_angle", context.spread_angle);
    scope.push("density", context.density);
    scope.push("angle", context.angle);
    scope.push("speed", context.speed);
    scope.push("life", context.life);
    scope.push("x", context.origin_x);
    scope.push("y", context.origin_y);
    scope.push("rotation", 0.0);
    scope.push("scale_x", 1.0);
    scope.push("scale_y", 1.0);
    scope.push("alpha", 1.0);
    scope.push("pos", Vec2::ZERO);

    // The sample syntax intentionally uses assignments (`count = 5`) for
    // intermediate values.  Seeding the common names keeps that concise form
    // valid in Rhai while custom scripts can still use `let` for new names.
    for name in [
        "ring_count",
        "layers",
        "ways",
        "layer",
        "slot",
        "base_angle",
        "phase",
    ] {
        scope.push(name, 0.0);
    }
    scope
}

fn validate_spawn(values: SpawnValues) -> Result<SpawnValues, ScriptError> {
    if [values.count, values.angle, values.speed, values.life]
        .iter()
        .all(|value| value.is_finite())
        && values.count >= 0.0
        && values.speed >= 0.0
        && values.life > 0.0
    {
        Ok(values)
    } else {
        Err(ScriptError::InvalidValue(
            "spawn returned NaN, Infinity, negative speed, or non-positive life".to_string(),
        ))
    }
}

fn read_number(scope: &Scope<'_>, name: &str) -> Result<f64, ScriptError> {
    let value = scope
        .get_value::<Dynamic>(name)
        .ok_or_else(|| ScriptError::InvalidValue(format!("script did not assign {name}")))?;
    if value.is_float() {
        value
            .as_float()
            .map_err(|_| ScriptError::InvalidValue(format!("{name} is not a number")))
    } else if value.is_int() {
        value
            .as_int()
            .map(|integer| integer as f64)
            .map_err(|_| ScriptError::InvalidValue(format!("{name} is not a number")))
    } else {
        Err(ScriptError::InvalidValue(format!("{name} is not a number")))
    }
}

fn extract_block(source: &str, name: &str) -> Result<String, ScriptError> {
    let bytes = source.as_bytes();
    let mut search_from = 0usize;
    while let Some(relative) = source[search_from..].find(name) {
        let start = search_from + relative;
        let before_ok = start == 0 || !is_identifier_byte(bytes[start - 1]);
        let after_name = start + name.len();
        let after_ok = after_name >= bytes.len() || !is_identifier_byte(bytes[after_name]);
        if before_ok && after_ok {
            let mut open = after_name;
            while open < bytes.len() && bytes[open].is_ascii_whitespace() {
                open += 1;
            }
            if open < bytes.len() && bytes[open] == b'{' {
                let close = matching_brace(source, open)
                    .ok_or_else(|| ScriptError::ParseError(format!("unterminated {name} block")))?;
                return Ok(source[open + 1..close].to_string());
            }
        }
        search_from = after_name;
    }
    Err(ScriptError::ParseError(format!("missing {name} block")))
}

fn matching_brace(source: &str, open: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut index = open;
    let mut quote = None;
    let mut line_comment = false;
    let mut block_comment = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if line_comment {
            if byte == b'\n' {
                line_comment = false;
            }
            index += 1;
            continue;
        }
        if block_comment {
            if byte == b'*' && bytes.get(index + 1) == Some(&b'/') {
                block_comment = false;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        if let Some(expected) = quote {
            if byte == b'\\' {
                index += 2;
                continue;
            }
            if byte == expected {
                quote = None;
            }
            index += 1;
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'/') {
            line_comment = true;
            index += 2;
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
            block_comment = true;
            index += 2;
            continue;
        }
        if byte == b'\'' || byte == b'"' {
            quote = Some(byte);
        } else if byte == b'{' {
            depth += 1;
        } else if byte == b'}' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(index);
            }
        }
        index += 1;
    }
    None
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::Preset;

    #[test]
    fn preset_compiles_and_evaluates() {
        let script = ScriptEngine::compile(Preset::AimedOdd.source()).unwrap();
        let input = crate::danmaku::RuntimeInput {
            time: 1.0,
            interval: 0.2,
            seed: 1,
            origin_x: 0.0,
            origin_y: 0.0,
            target_x: 100.0,
            target_y: 0.0,
        };
        let bullets = crate::danmaku::evaluate(&script, input, Default::default()).unwrap();
        // At t=1.0 with a 0.2s interval, waves 0..=5 have spawned; all six
        // are still within the six-second life.
        assert_eq!(bullets.len(), 6 * 5);
        assert!(bullets.iter().any(|bullet| bullet.bullet.i == 2));
    }

    #[test]
    fn bundled_title_samples_compile_and_evaluate() {
        let samples = [
            (
                "touhou_rotating_ring",
                include_str!("../../samples/touhou_rotating_ring.rhai"),
            ),
            (
                "dodonpachi_daioujou_aimed_fan",
                include_str!("../../samples/dodonpachi_daioujou_aimed_fan.rhai"),
            ),
            (
                "mushihimesama_spiral_layers",
                include_str!("../../samples/mushihimesama_spiral_layers.rhai"),
            ),
        ];
        let input = crate::danmaku::RuntimeInput {
            time: 0.25,
            interval: 0.2,
            seed: 1,
            origin_x: 0.0,
            origin_y: 0.0,
            target_x: 500.0,
            target_y: 100.0,
        };

        for (name, source) in samples {
            let script = ScriptEngine::compile(source)
                .unwrap_or_else(|error| panic!("{name} failed to compile: {error}"));
            let bullets = crate::danmaku::evaluate(&script, input, Default::default())
                .unwrap_or_else(|error| panic!("{name} failed to evaluate: {error}"));
            assert!(!bullets.is_empty(), "{name} produced no bullets");
        }
    }

    #[test]
    fn bundled_samples_use_spread_angle_and_density_variables() {
        let sources = [
            include_str!("../../samples/touhou_rotating_ring.rhai"),
            include_str!("../../samples/dodonpachi_daioujou_aimed_fan.rhai"),
            include_str!("../../samples/mushihimesama_spiral_layers.rhai"),
        ];
        let target_angle = 43.2_f64.to_radians();
        let input = crate::danmaku::RuntimeInput {
            time: 0.4,
            interval: 0.2,
            seed: 1,
            origin_x: 0.0,
            origin_y: 0.0,
            target_x: 100.0 * target_angle.cos(),
            target_y: 100.0 * target_angle.sin(),
        };

        for source in sources {
            let script = ScriptEngine::compile(source).unwrap();
            let spawn_angle = |options| {
                let mut evaluator = script.evaluator(1_000);
                script
                    .eval_spawn(
                        &mut evaluator,
                        ScriptContext::spawn(1, 0, 2, 0.4, input, options),
                    )
                    .unwrap()
                    .angle
            };
            let original = spawn_angle(Default::default());
            let narrowed = spawn_angle(crate::danmaku::RuntimeOptions {
                spread_angle_degrees: 180.0,
                ..Default::default()
            });
            let denser = spawn_angle(crate::danmaku::RuntimeOptions {
                density: 2.0,
                ..Default::default()
            });
            assert!((narrowed - original).abs() > 1e-6);
            assert!((denser - original).abs() > 1e-6);
        }
    }

    fn preset_input(time: f64, interval: f64, target_degrees: f64) -> crate::danmaku::RuntimeInput {
        let target_angle = target_degrees.to_radians();
        crate::danmaku::RuntimeInput {
            time,
            interval,
            seed: 7,
            origin_x: 0.0,
            origin_y: 0.0,
            target_x: target_angle.cos() * 100.0,
            target_y: target_angle.sin() * 100.0,
        }
    }

    fn evaluate_preset(
        preset: Preset,
        input: crate::danmaku::RuntimeInput,
    ) -> crate::danmaku::RuntimeResult {
        let script = ScriptEngine::compile(preset.source())?;
        crate::danmaku::evaluate(&script, input, Default::default())
    }

    #[test]
    fn aimed_odd_five_way_has_a_center_bullet_aimed_at_the_target() {
        let input = crate::danmaku::RuntimeInput {
            origin_x: 10.0,
            origin_y: -20.0,
            target_x: 110.0,
            target_y: -20.0,
            ..preset_input(0.1, 1.0, 0.0)
        };
        let bullets = evaluate_preset(Preset::AimedOdd, input).unwrap();

        assert_eq!(bullets.len(), 5);
        let center = bullets
            .iter()
            .find(|resolved| resolved.bullet.i == 2)
            .unwrap();
        assert!(center.bullet.angle.abs() < 1e-12);
        assert!((center.state.x - 52.0).abs() < 1e-12);
        assert!((center.state.y + 20.0).abs() < 1e-12);
        assert!((bullets[0].bullet.angle - crate::danmaku::deg(-16.0)).abs() < 1e-12);
        assert!((bullets[4].bullet.angle - crate::danmaku::deg(16.0)).abs() < 1e-12);
    }

    #[test]
    fn spread_angle_sets_the_full_width_for_fans_and_partial_rings() {
        let options = crate::danmaku::RuntimeOptions {
            spread_angle_degrees: 90.0,
            ..Default::default()
        };
        let input = preset_input(0.1, 1.0, 0.0);
        for preset in [Preset::AimedOdd, Preset::QuantizedAimed, Preset::Hybrid] {
            let script = ScriptEngine::compile(preset.source()).unwrap();
            let bullets =
                crate::danmaku::evaluate_with_options(&script, input, Default::default(), options)
                    .unwrap();
            let first = bullets.iter().find(|item| item.bullet.i == 0).unwrap();
            let center = bullets.iter().find(|item| item.bullet.i == 2).unwrap();
            let last = bullets.iter().find(|item| item.bullet.i == 4).unwrap();
            assert!((first.bullet.angle - crate::danmaku::deg(-45.0)).abs() < 1e-10);
            assert!(center.bullet.angle.abs() < 1e-10);
            assert!((last.bullet.angle - crate::danmaku::deg(45.0)).abs() < 1e-10);
        }

        let script = ScriptEngine::compile(Preset::SpeedRing.source()).unwrap();
        let bullets =
            crate::danmaku::evaluate_with_options(&script, input, Default::default(), options)
                .unwrap();
        assert!(bullets[0].bullet.angle.abs() < 1e-10);
        assert!((bullets[23].bullet.angle - crate::danmaku::deg(90.0)).abs() < 1e-10);
    }

    #[test]
    fn quantized_aim_changes_only_after_crossing_the_boundary() {
        let center_angle = |target_degrees| {
            evaluate_preset(
                Preset::QuantizedAimed,
                preset_input(0.1, 1.0, target_degrees),
            )
            .unwrap()
            .into_iter()
            .find(|resolved| resolved.bullet.i == 2)
            .unwrap()
            .bullet
            .angle
        };

        let before = center_angle(42.0);
        let just_before = center_angle(42.18);
        let just_after = center_angle(42.19);
        assert!((before - crate::danmaku::deg(39.375)).abs() < 1e-12);
        assert_eq!(before, just_before);
        assert!((just_after - crate::danmaku::deg(45.0)).abs() < 1e-12);
        assert_ne!(just_before, just_after);
    }

    #[test]
    fn speed_difference_ring_separates_three_layers_at_each_angle() {
        let input = preset_input(0.5, 1.0, 0.0);
        let bullets = evaluate_preset(Preset::SpeedRing, input).unwrap();

        assert_eq!(bullets.len(), 24 * 3);
        for slot in 0..24_u32 {
            let layers: Vec<_> = [slot, slot + 24, slot + 48]
                .into_iter()
                .map(|index| {
                    bullets
                        .iter()
                        .find(|resolved| resolved.bullet.i == index)
                        .unwrap()
                })
                .collect();
            assert_eq!(layers[0].bullet.angle, layers[1].bullet.angle);
            assert_eq!(layers[1].bullet.angle, layers[2].bullet.angle);
            assert_eq!(layers[0].bullet.speed, 280.0);
            assert_eq!(layers[1].bullet.speed, 420.0);
            assert_eq!(layers[2].bullet.speed, 560.0);

            let radii: Vec<_> = layers
                .iter()
                .map(|resolved| resolved.state.x.hypot(resolved.state.y))
                .collect();
            assert!((radii[0] - 140.0).abs() < 1e-9);
            assert!((radii[1] - 210.0).abs() < 1e-9);
            assert!((radii[2] - 280.0).abs() < 1e-9);
        }
    }

    #[test]
    fn hybrid_combines_five_way_quantization_speed_layers_and_wave_phase() {
        let bullets = evaluate_preset(Preset::Hybrid, preset_input(0.21, 0.2, 43.2)).unwrap();
        let wave_zero: Vec<_> = bullets
            .iter()
            .filter(|resolved| resolved.bullet.wave == 0)
            .collect();
        let wave_one: Vec<_> = bullets
            .iter()
            .filter(|resolved| resolved.bullet.wave == 1)
            .collect();

        assert_eq!(wave_zero.len(), 15);
        assert_eq!(wave_one.len(), 15);
        for (index, expected_speed) in [(2, 320.0), (7, 460.0), (12, 600.0)] {
            let bullet = wave_one
                .iter()
                .find(|resolved| resolved.bullet.i == index)
                .unwrap();
            assert_eq!(bullet.bullet.speed, expected_speed);
            assert!((bullet.bullet.angle - crate::danmaku::deg(48.0)).abs() < 1e-12);
        }

        let wave_zero_center = wave_zero
            .iter()
            .find(|resolved| resolved.bullet.i == 2)
            .unwrap();
        let wave_one_center = wave_one
            .iter()
            .find(|resolved| resolved.bullet.i == 2)
            .unwrap();
        assert!((wave_zero_center.bullet.angle - crate::danmaku::deg(45.0)).abs() < 1e-12);
        assert!(
            (wave_one_center.bullet.angle
                - wave_zero_center.bullet.angle
                - crate::danmaku::deg(3.0))
            .abs()
                < 1e-12
        );
    }

    #[test]
    fn custom_script_reports_parse_errors() {
        let error = match ScriptEngine::compile("spawn { count = ; } motion { }") {
            Ok(_) => panic!("invalid script unexpectedly compiled"),
            Err(error) => error,
        };
        assert!(matches!(error, ScriptError::ParseError(_)));
    }

    #[test]
    fn custom_script_uses_deterministic_rand_and_reports_other_errors() {
        let source = r#"
spawn {
    count = 1;
    angle = rand(seed, wave, i, 0);
    speed = 10;
    life = 1;
}
motion {
    pos = polar(angle, speed * age);
    x = pos.x;
    y = pos.y;
}
"#;
        let script = ScriptEngine::compile(source).unwrap();
        let input = crate::danmaku::RuntimeInput {
            time: 0.25,
            interval: 0.2,
            seed: 123,
            origin_x: 0.0,
            origin_y: 0.0,
            target_x: 1.0,
            target_y: 0.0,
        };
        let first = crate::danmaku::evaluate(&script, input, Default::default()).unwrap();
        let second = crate::danmaku::evaluate(&script, input, Default::default()).unwrap();
        assert_eq!(first, second);

        let runtime_script = ScriptEngine::compile(
            "spawn { count = 1; angle = missing(); speed = 1; life = 1; } motion { }",
        )
        .unwrap();
        let runtime_error = crate::danmaku::evaluate(&runtime_script, input, Default::default())
            .expect_err("missing function should be a runtime error");
        assert!(matches!(runtime_error, ScriptError::RuntimeError(_)));

        let invalid_script = ScriptEngine::compile(
            "spawn { count = 4097; angle = 0; speed = 1; life = 1; } motion { }",
        )
        .unwrap();
        let invalid_error = crate::danmaku::evaluate(&invalid_script, input, Default::default())
            .expect_err("too many bullets should be rejected");
        assert!(matches!(invalid_error, ScriptError::InvalidValue(_)));
    }

    #[test]
    fn custom_spawn_receives_context_and_returns_bullet_parameters() {
        let source = r#"
spawn {
    count = 3;
    angle = aim() + wave * deg(10) + t;
    speed = seed + target_x - origin_y;
    life = 2.5;
}
motion {
    x = origin_x + speed * age;
    y = origin_y + angle;
}
"#;
        let script = ScriptEngine::compile(source).unwrap();
        let input = crate::danmaku::RuntimeInput {
            time: 0.0,
            interval: 0.2,
            seed: 11,
            origin_x: 4.0,
            origin_y: -3.0,
            target_x: 8.0,
            target_y: 1.0,
        };
        let context = ScriptContext::spawn(0, 0, 2, 0.5, input, Default::default());
        let mut evaluator = script.evaluator(1_000);
        let spawn = script.eval_spawn(&mut evaluator, context).unwrap();

        assert_eq!(spawn.count, 3.0);
        assert!(
            (spawn.angle - ((4.0_f64).atan2(4.0) + crate::danmaku::deg(20.0) + 0.5)).abs() < 1e-12
        );
        assert_eq!(spawn.speed, 22.0);
        assert_eq!(spawn.life, 2.5);
    }

    #[test]
    fn script_operation_limit_reports_runtime_error() {
        let script = ScriptEngine::compile(
            r#"
spawn {
    count = 1;
    while true {}
    angle = 0;
    speed = 1;
    life = 1;
}
motion { x = 0; y = 0; }
"#,
        )
        .unwrap();
        let input = crate::danmaku::RuntimeInput {
            time: 0.0,
            interval: 0.2,
            seed: 1,
            origin_x: 0.0,
            origin_y: 0.0,
            target_x: 1.0,
            target_y: 0.0,
        };
        let mut evaluator = script.evaluator(64);
        let error = script
            .eval_spawn(
                &mut evaluator,
                ScriptContext::spawn(0, 0, 0, 0.0, input, Default::default()),
            )
            .expect_err("unbounded script must stop at the operation limit");
        assert!(matches!(error, ScriptError::RuntimeError(_)));
    }

    #[test]
    fn invalid_numeric_values_and_large_sources_are_rejected() {
        let input = crate::danmaku::RuntimeInput {
            time: 0.1,
            interval: 1.0,
            seed: 1,
            origin_x: 0.0,
            origin_y: 0.0,
            target_x: 1.0,
            target_y: 0.0,
        };
        for source in [
            "spawn { count = 1; angle = sqrt(-1.0); speed = 1; life = 1; } motion { x = 0; y = 0; }",
            "spawn { count = 1; angle = 0; speed = -1; life = 1; } motion { x = 0; y = 0; }",
            "spawn { count = 1; angle = 0; speed = 1; life = -1; } motion { x = 0; y = 0; }",
            "spawn { count = 1.5; angle = 0; speed = 1; life = 1; } motion { x = 0; y = 0; }",
        ] {
            let script = ScriptEngine::compile(source).unwrap();
            let error = crate::danmaku::evaluate(&script, input, Default::default()).unwrap_err();
            assert!(matches!(error, ScriptError::InvalidValue(_)));
        }

        let oversized = " ".repeat(MAX_SCRIPT_SOURCE_BYTES + 1);
        let error = match ScriptEngine::compile(&oversized) {
            Ok(_) => panic!("oversized script unexpectedly compiled"),
            Err(error) => error,
        };
        assert!(matches!(error, ScriptError::InvalidValue(_)));
    }
}
