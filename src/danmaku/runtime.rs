use super::{active_wave_count, wave_at, Bullet, BulletState};
use crate::script::{CompiledScript, ScriptContext, ScriptError, SpawnValues};

pub const MAX_BULLETS: usize = 4096;
pub const MAX_WAVES: usize = 1024;
pub const MAX_LIFE_SECONDS: f64 = 30.0;
pub const MAX_SCRIPT_EVALUATIONS: usize = 65_536;
pub const DEFAULT_HEADING_SAMPLE_SECONDS: f64 = 1.0 / 120.0;

#[derive(Debug, Clone, Copy)]
pub struct RuntimeLimits {
    pub max_bullets: usize,
    pub max_waves: usize,
    pub max_life_seconds: f64,
    pub max_script_evaluations: usize,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            max_bullets: MAX_BULLETS,
            max_waves: MAX_WAVES,
            max_life_seconds: MAX_LIFE_SECONDS,
            max_script_evaluations: MAX_SCRIPT_EVALUATIONS,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RuntimeOptions {
    pub calculate_heading: bool,
    pub heading_sample_seconds: f64,
    /// Total angular coverage in degrees; 0 lets each script use its own default.
    pub spread_angle_degrees: f64,
    /// Multiplies the wave frequency; 1.0 preserves the script's timing.
    pub density: f64,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            calculate_heading: false,
            heading_sample_seconds: DEFAULT_HEADING_SAMPLE_SECONDS,
            spread_angle_degrees: 0.0,
            density: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RuntimeInput {
    pub time: f64,
    pub interval: f64,
    pub seed: i64,
    pub origin_x: f64,
    pub origin_y: f64,
    pub target_x: f64,
    pub target_y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedBullet {
    pub bullet: Bullet,
    pub state: BulletState,
    pub heading: Option<f64>,
}

pub type RuntimeResult = Result<Vec<ResolvedBullet>, ScriptError>;

pub fn evaluate(
    script: &CompiledScript,
    input: RuntimeInput,
    limits: RuntimeLimits,
) -> RuntimeResult {
    evaluate_with_options(script, input, limits, RuntimeOptions::default())
}

pub fn evaluate_with_options(
    script: &CompiledScript,
    input: RuntimeInput,
    limits: RuntimeLimits,
    options: RuntimeOptions,
) -> RuntimeResult {
    validate_input(input, limits)?;
    validate_options(options)?;
    let interval = input.interval / options.density;
    let current = wave_at(input.time, interval)
        .map_err(|message| ScriptError::InvalidValue(message.to_string()))?;

    // Life is intentionally bounded.  This lets us enumerate only a bounded
    // recent history rather than replaying every wave since t=0.
    let wave_count = active_wave_count(limits.max_life_seconds, interval, limits.max_waves)
        .map_err(|message| ScriptError::InvalidValue(message.to_string()))?;
    if (limits.max_life_seconds / interval).ceil() as usize + 1 > limits.max_waves {
        return Err(ScriptError::InvalidValue(format!(
            "interval is too small for the wave limit (max {} concurrent waves)",
            limits.max_waves
        )));
    }

    let mut evaluator = script.evaluator(limits.max_script_evaluations as u64);
    let mut evaluations = 0usize;
    let mut output = Vec::new();

    for offset in 0..wave_count {
        let wave = current.wave - offset as i64;
        if wave < 0 {
            break;
        }

        let spawn_time = wave as f64 * interval;
        let age = input.time.max(0.0) - spawn_time;
        if age < 0.0 {
            continue;
        }

        let probe = eval_spawn(
            script,
            &mut evaluator,
            &mut evaluations,
            limits.max_script_evaluations,
            ScriptContext::spawn(0, 0, wave, spawn_time, input, options),
        )?;
        validate_life(probe.life, limits.max_life_seconds)?;
        let count = checked_count(probe.count, limits.max_bullets)?;
        if count == 0 {
            continue;
        }

        for i in 0..count {
            let spawn = if i == 0 {
                probe
            } else {
                eval_spawn(
                    script,
                    &mut evaluator,
                    &mut evaluations,
                    limits.max_script_evaluations,
                    ScriptContext::spawn(i as u32, count as u32, wave, spawn_time, input, options),
                )?
            };
            validate_life(spawn.life, limits.max_life_seconds)?;
            let bullet_count = checked_count(spawn.count, limits.max_bullets)?;
            if bullet_count != count {
                return Err(ScriptError::InvalidValue(format!(
                    "count must be constant within a wave (wave {wave}, bullet {i})"
                )));
            }

            if age >= spawn.life {
                continue;
            }

            let context = ScriptContext::motion(
                i as u32,
                count as u32,
                wave,
                input.time.max(0.0),
                age,
                input,
                options,
                spawn,
            );
            if evaluations >= limits.max_script_evaluations {
                return Err(ScriptError::InvalidValue(
                    "script evaluation limit exceeded".to_string(),
                ));
            }
            let state = script
                .eval_motion(&mut evaluator, context)
                .map_err(|error| {
                    if matches!(error, ScriptError::RuntimeError(_)) {
                        ScriptError::RuntimeError(format!("wave {wave}, bullet {i}: {error}"))
                    } else {
                        error
                    }
                })?;
            evaluations = evaluations.saturating_add(1);
            if evaluations > limits.max_script_evaluations {
                return Err(ScriptError::InvalidValue(
                    "script evaluation limit exceeded".to_string(),
                ));
            }

            let bullet = Bullet {
                i: i as u32,
                wave,
                age,
                origin_x: input.origin_x,
                origin_y: input.origin_y,
                angle: spawn.angle,
                speed: spawn.speed,
                life: spawn.life,
            };
            let heading = if options.calculate_heading {
                evaluate_heading(
                    script,
                    &mut evaluator,
                    &mut evaluations,
                    limits.max_script_evaluations,
                    context,
                    spawn,
                    state,
                    options.heading_sample_seconds,
                )?
            } else {
                None
            };
            if output.len() >= limits.max_bullets {
                return Err(ScriptError::InvalidValue(format!(
                    "total live bullet count exceeds the maximum of {}",
                    limits.max_bullets
                )));
            }
            output.push(ResolvedBullet {
                bullet,
                state,
                heading,
            });
        }
    }

    Ok(output)
}

#[allow(clippy::too_many_arguments)]
fn evaluate_heading(
    script: &CompiledScript,
    evaluator: &mut crate::script::ScriptEvaluator,
    evaluations: &mut usize,
    max_evaluations: usize,
    context: ScriptContext,
    spawn: SpawnValues,
    current: BulletState,
    sample_seconds: f64,
) -> Result<Option<f64>, ScriptError> {
    let (sample_age, sample_t, forward) = if context.age > 0.0 {
        let delta = sample_seconds.min(context.age);
        (context.age - delta, context.t - delta, false)
    } else {
        let delta = sample_seconds.min(spawn.life * 0.5);
        (context.age + delta, context.t + delta, true)
    };
    if sample_age == context.age {
        return Ok(None);
    }
    if *evaluations >= max_evaluations {
        return Err(ScriptError::InvalidValue(
            "script evaluation limit exceeded".to_string(),
        ));
    }

    let sample_context = ScriptContext {
        t: sample_t,
        age: sample_age,
        ..context
    };
    let sample = script.eval_motion(evaluator, sample_context)?;
    *evaluations = evaluations.saturating_add(1);

    let (dx, dy) = if forward {
        (sample.x - current.x, sample.y - current.y)
    } else {
        (current.x - sample.x, current.y - sample.y)
    };
    if dx.abs() <= f64::EPSILON && dy.abs() <= f64::EPSILON {
        Ok(None)
    } else {
        Ok(Some(dy.atan2(dx)))
    }
}

fn eval_spawn(
    script: &CompiledScript,
    evaluator: &mut crate::script::ScriptEvaluator,
    evaluations: &mut usize,
    max_evaluations: usize,
    context: ScriptContext,
) -> Result<SpawnValues, ScriptError> {
    if *evaluations >= max_evaluations {
        return Err(ScriptError::InvalidValue(
            "script evaluation limit exceeded".to_string(),
        ));
    }
    *evaluations = evaluations.saturating_add(1);
    script.eval_spawn(evaluator, context)
}

fn validate_life(life: f64, max_life: f64) -> Result<(), ScriptError> {
    if !life.is_finite() || life <= 0.0 || life > max_life {
        return Err(ScriptError::InvalidValue(format!(
            "life must be in (0, {max_life}] seconds"
        )));
    }
    Ok(())
}

fn checked_count(count: f64, max: usize) -> Result<usize, ScriptError> {
    if !count.is_finite() || count < 0.0 || count.fract() != 0.0 {
        return Err(ScriptError::InvalidValue(
            "count must be a finite non-negative integer".to_string(),
        ));
    }
    let count = count as usize;
    if count > max {
        return Err(ScriptError::InvalidValue(format!(
            "count {count} exceeds the maximum of {max}"
        )));
    }
    Ok(count)
}

fn validate_input(input: RuntimeInput, limits: RuntimeLimits) -> Result<(), ScriptError> {
    if !input.time.is_finite()
        || !input.interval.is_finite()
        || input.interval <= 0.0
        || !input.origin_x.is_finite()
        || !input.origin_y.is_finite()
        || !input.target_x.is_finite()
        || !input.target_y.is_finite()
        || limits.max_bullets == 0
        || limits.max_waves == 0
        || !limits.max_life_seconds.is_finite()
        || limits.max_life_seconds <= 0.0
    {
        return Err(ScriptError::InvalidValue(
            "runtime input contains an invalid value".to_string(),
        ));
    }
    Ok(())
}

fn validate_options(options: RuntimeOptions) -> Result<(), ScriptError> {
    if !options.heading_sample_seconds.is_finite()
        || options.heading_sample_seconds <= 0.0
        || !options.spread_angle_degrees.is_finite()
        || !(0.0..=360.0).contains(&options.spread_angle_degrees)
        || !options.density.is_finite()
        || options.density <= 0.0
    {
        return Err(ScriptError::InvalidValue(
            "runtime options require spread in 0..=360 degrees and positive heading interval / density"
                .to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::{Preset, ScriptEngine};

    fn input(time: f64) -> RuntimeInput {
        RuntimeInput {
            time,
            interval: 0.2,
            seed: 1234,
            origin_x: 10.0,
            origin_y: -20.0,
            target_x: 320.0,
            target_y: 180.0,
        }
    }

    #[test]
    fn direct_seek_matches_sequential_and_reverse_evaluation() {
        let script = ScriptEngine::compile(Preset::Hybrid.source()).unwrap();
        let limits = RuntimeLimits::default();
        let target_time = 3.47;
        let direct = evaluate(&script, input(target_time), limits).unwrap();

        // 0秒付近から順に評価した後でも、同じ時刻は同じ結果になる。
        for time in [0.0, 0.1, 0.7, 1.8, 2.9, target_time] {
            evaluate(&script, input(time), limits).unwrap();
        }
        let after_forward_playback = evaluate(&script, input(target_time), limits).unwrap();
        assert_eq!(direct, after_forward_playback);

        // 後方の時刻から逆向きにシークしても、評価順には依存しない。
        for time in [8.0, 6.0, 4.0, target_time] {
            evaluate(&script, input(time), limits).unwrap();
        }
        let after_reverse_seek = evaluate(&script, input(target_time), limits).unwrap();
        assert_eq!(direct, after_reverse_seek);
    }

    #[test]
    fn bullet_age_is_derived_from_current_time_and_spawn_time() {
        let script = ScriptEngine::compile(Preset::AimedOdd.source()).unwrap();
        let bullets = evaluate(&script, input(0.45), RuntimeLimits::default()).unwrap();

        let current = bullets
            .iter()
            .find(|resolved| resolved.bullet.wave == 2 && resolved.bullet.i == 0)
            .unwrap();
        let previous = bullets
            .iter()
            .find(|resolved| resolved.bullet.wave == 1 && resolved.bullet.i == 0)
            .unwrap();

        assert!((current.bullet.age - 0.05).abs() < 1e-12);
        assert!((previous.bullet.age - 0.25).abs() < 1e-12);
    }

    #[test]
    fn heading_is_calculated_from_the_scripted_motion_without_frame_state() {
        let script = ScriptEngine::compile(
            r#"
spawn { count = 1; angle = 0; speed = 100; life = 2; }
motion {
    x = origin_x;
    y = origin_y + speed * age;
}
"#,
        )
        .unwrap();
        let options = RuntimeOptions {
            calculate_heading: true,
            ..RuntimeOptions::default()
        };
        let first =
            evaluate_with_options(&script, input(0.45), RuntimeLimits::default(), options).unwrap();
        let second =
            evaluate_with_options(&script, input(0.45), RuntimeLimits::default(), options).unwrap();

        assert_eq!(first, second);
        assert!(first.iter().all(|resolved| {
            (resolved.heading.unwrap() - std::f64::consts::FRAC_PI_2).abs() < 1e-12
        }));
    }

    #[test]
    fn spread_angle_controls_full_fan_width_without_rotating_its_center() {
        let script = ScriptEngine::compile(
            r#"
spawn {
    count = 5;
    angle = aim() + (i - (count - 1) / 2) * spread_angle / (count - 1);
    speed = 100;
    life = 2;
}
motion {
    pos = polar(angle, speed * age);
    x = origin_x + pos.x;
    y = origin_y + pos.y;
}
"#,
        )
        .unwrap();
        let mut aimed_right = input(0.1);
        aimed_right.target_x = aimed_right.origin_x + 100.0;
        aimed_right.target_y = aimed_right.origin_y;
        let options = RuntimeOptions {
            calculate_heading: true,
            spread_angle_degrees: 90.0,
            ..RuntimeOptions::default()
        };
        let bullets =
            evaluate_with_options(&script, aimed_right, RuntimeLimits::default(), options).unwrap();
        let wave_zero: Vec<_> = bullets
            .iter()
            .filter(|item| item.bullet.wave == 0)
            .collect();
        assert_eq!(wave_zero.len(), 5);
        assert!((wave_zero[0].bullet.angle + std::f64::consts::FRAC_PI_4).abs() < 1e-10);
        assert!(wave_zero[2].bullet.angle.abs() < 1e-10);
        assert!((wave_zero[4].bullet.angle - std::f64::consts::FRAC_PI_4).abs() < 1e-10);
        assert!(
            (wave_zero[4].bullet.angle - wave_zero[0].bullet.angle - std::f64::consts::FRAC_PI_2)
                .abs()
                < 1e-10
        );
        assert!(wave_zero[2].heading.unwrap().abs() < 1e-10);
        assert!((wave_zero[2].state.x - 20.0).abs() < 1e-10);
        assert!((wave_zero[2].state.y - (-20.0)).abs() < 1e-10);
    }

    #[test]
    fn density_is_exposed_to_spawn_and_motion() {
        let script = ScriptEngine::compile(
            r#"
spawn { count = 1; angle = 0; speed = 100 * density; life = 2; }
motion { x = origin_x + speed * age; y = origin_y + density; }
"#,
        )
        .unwrap();
        let options = RuntimeOptions {
            density: 2.0,
            ..RuntimeOptions::default()
        };
        let bullets =
            evaluate_with_options(&script, input(0.05), RuntimeLimits::default(), options).unwrap();
        assert_eq!(bullets[0].bullet.speed, 200.0);
        assert!((bullets[0].state.x - 20.0).abs() < 1e-12);
        assert!((bullets[0].state.y - (-18.0)).abs() < 1e-12);
    }

    #[test]
    fn density_changes_wave_rate_but_not_bullets_per_wave() {
        let script = ScriptEngine::compile(Preset::AimedOdd.source()).unwrap();
        let options = RuntimeOptions {
            density: 2.0,
            ..RuntimeOptions::default()
        };
        let bullets =
            evaluate_with_options(&script, input(0.45), RuntimeLimits::default(), options).unwrap();
        let wave_four: Vec<_> = bullets
            .iter()
            .filter(|bullet| bullet.bullet.wave == 4)
            .collect();
        assert_eq!(wave_four.len(), 5);
        assert!((wave_four[0].bullet.age - 0.05).abs() < 1e-12);

        let direct =
            evaluate_with_options(&script, input(1.47), RuntimeLimits::default(), options).unwrap();
        evaluate_with_options(&script, input(2.0), RuntimeLimits::default(), options).unwrap();
        assert_eq!(
            direct,
            evaluate_with_options(&script, input(1.47), RuntimeLimits::default(), options).unwrap()
        );
    }

    #[test]
    fn invalid_density_is_rejected() {
        let script = ScriptEngine::compile(Preset::AimedOdd.source()).unwrap();
        for density in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let options = RuntimeOptions {
                density,
                ..RuntimeOptions::default()
            };
            assert!(matches!(
                evaluate_with_options(&script, input(0.1), RuntimeLimits::default(), options),
                Err(ScriptError::InvalidValue(_))
            ));
        }
    }

    #[test]
    fn total_live_bullet_limit_applies_across_all_waves() {
        let script = ScriptEngine::compile(
            r#"
spawn { count = 100; angle = 0; speed = 1; life = 2; }
motion { x = origin_x + speed * age; y = origin_y; }
"#,
        )
        .unwrap();
        let limits = RuntimeLimits {
            max_bullets: 150,
            ..RuntimeLimits::default()
        };
        let error = evaluate(&script, input(0.45), limits).unwrap_err();

        assert!(matches!(error, ScriptError::InvalidValue(_)));
        assert!(error.to_string().contains("total live bullet count"));
    }

    #[test]
    fn excessive_concurrent_wave_count_is_rejected() {
        let script = ScriptEngine::compile(Preset::AimedOdd.source()).unwrap();
        let error = evaluate(
            &script,
            RuntimeInput {
                interval: 0.001,
                ..input(1.0)
            },
            RuntimeLimits::default(),
        )
        .unwrap_err();

        assert!(matches!(error, ScriptError::InvalidValue(_)));
        assert!(error.to_string().contains("wave limit"));
    }
}
