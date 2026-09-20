use crate::danmaku::ResolvedBullet;

#[derive(Debug, Clone, Copy)]
pub struct RotationOptions {
    pub follow_path: bool,
}

impl Default for RotationOptions {
    fn default() -> Self {
        Self { follow_path: false }
    }
}

pub fn resolve_rotation(resolved: &ResolvedBullet, options: RotationOptions) -> f64 {
    if options.follow_path {
        if let Some(heading) = resolved.heading {
            return path_heading_to_object_rotation(heading);
        }
    }
    resolved.state.rotation
}

/// 画面座標上の進行方向を、上向きを正面とする入力画像の回転角へ変換する。
///
/// `heading` は +X を 0 とする画面座標の角度だが、AviUtl2 の標準三角形は
/// ローカルの上方向が頂点である。固定角を足すのではなく、単位速度ベクトルを
/// AviUtl2 のオブジェクト回転座標へ写像している。
fn path_heading_to_object_rotation(heading: f64) -> f64 {
    let velocity_x = heading.cos();
    let velocity_y = heading.sin();
    velocity_x.atan2(-velocity_y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::danmaku::{Bullet, BulletState};

    fn resolved(age: f64, heading: Option<f64>, script_rotation: f64) -> ResolvedBullet {
        ResolvedBullet {
            bullet: Bullet {
                i: 0,
                wave: 0,
                age,
                origin_x: 0.0,
                origin_y: 0.0,
                angle: 0.0,
                speed: 1.0,
                life: 2.0,
            },
            state: BulletState {
                rotation: script_rotation,
                ..BulletState::default()
            },
            heading,
        }
    }

    #[test]
    fn enabled_option_maps_screen_velocity_to_the_up_facing_object_axis() {
        let right = resolved(0.0, Some(0.0), 0.0);
        let down = resolved(0.0, Some(std::f64::consts::FRAC_PI_2), 0.0);
        let left = resolved(0.0, Some(std::f64::consts::PI), 0.0);
        let up = resolved(0.0, Some(-std::f64::consts::FRAC_PI_2), 0.0);
        let options = RotationOptions {
            follow_path: true,
            ..RotationOptions::default()
        };

        assert!((resolve_rotation(&right, options) - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert!((resolve_rotation(&down, options) - std::f64::consts::PI).abs() < 1e-12);
        assert!((resolve_rotation(&left, options) + std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert!(resolve_rotation(&up, options).abs() < 1e-12);
    }

    #[test]
    fn disabled_options_preserve_script_rotation() {
        let bullet = resolved(1.0, Some(1.5), 0.25);
        assert_eq!(resolve_rotation(&bullet, RotationOptions::default()), 0.25);
    }
}
