#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveTime {
    pub wave: i64,
    pub spawn_time: f64,
    pub age: f64,
}

pub fn wave_at(time: f64, interval: f64) -> Result<WaveTime, &'static str> {
    if !time.is_finite() || !interval.is_finite() || interval <= 0.0 {
        return Err("time and interval must be finite, and interval must be positive");
    }

    let time = time.max(0.0);
    let wave = (time / interval).floor() as i64;
    let spawn_time = wave as f64 * interval;
    Ok(WaveTime {
        wave,
        spawn_time,
        age: (time - spawn_time).max(0.0),
    })
}

pub fn active_wave_count(
    life: f64,
    interval: f64,
    max_waves: usize,
) -> Result<usize, &'static str> {
    if !life.is_finite() || !interval.is_finite() || life < 0.0 || interval <= 0.0 {
        return Err("life must be finite and non-negative, and interval must be positive");
    }
    let needed = (life / interval).ceil() as usize + 1;
    Ok(needed.min(max_waves))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wave_and_age_are_directly_calculated() {
        let current = wave_at(1.05, 0.2).unwrap();
        assert_eq!(current.wave, 5);
        assert!((current.spawn_time - 1.0).abs() < 1e-12);
        assert!((current.age - 0.05).abs() < 1e-12);
    }

    #[test]
    fn active_wave_count_includes_current_wave() {
        assert_eq!(active_wave_count(4.0, 0.2, 256).unwrap(), 21);
    }

    #[test]
    fn invalid_interval_is_rejected() {
        assert!(wave_at(1.0, 0.0).is_err());
    }
}
