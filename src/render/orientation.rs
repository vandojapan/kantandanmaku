use crate::danmaku::ResolvedBullet;

#[derive(Debug, Clone, Copy)]
pub struct RotationOptions {
    pub follow_path: bool,
    /// 発生原点からの移動距離1pxあたりに加える回転角（度）。
    pub rotation_per_pixel_degrees: f64,
}

impl Default for RotationOptions {
    fn default() -> Self {
        Self {
            follow_path: false,
            rotation_per_pixel_degrees: 0.0,
        }
    }
}

pub fn resolve_rotation(resolved: &ResolvedBullet, options: RotationOptions) -> f64 {
    let base_rotation = if options.follow_path {
        if let Some(heading) = resolved.heading {
            path_heading_to_object_rotation(heading)
        } else {
            resolved.state.rotation
        }
    } else {
        resolved.state.rotation
    };
    let distance = (resolved.state.x - resolved.bullet.origin_x)
        .hypot(resolved.state.y - resolved.bullet.origin_y);
    base_rotation + (distance * options.rotation_per_pixel_degrees).to_radians()
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

    #[test]
    fn movement_rotation_is_derived_from_current_displacement() {
        let mut bullet = resolved(1.0, None, 0.25);
        bullet.state.x = 3.0;
        bullet.state.y = 4.0;
        let options = RotationOptions {
            rotation_per_pixel_degrees: 10.0,
            ..RotationOptions::default()
        };

        let expected = 0.25 + 50.0_f64.to_radians();
        assert!((resolve_rotation(&bullet, options) - expected).abs() < 1e-12);
    }

    #[test]
    fn movement_rotation_is_added_after_path_alignment() {
        let mut bullet = resolved(1.0, Some(0.0), 0.75);
        bullet.state.x = 100.0;
        let options = RotationOptions {
            follow_path: true,
            rotation_per_pixel_degrees: -1.0,
        };

        let expected = std::f64::consts::FRAC_PI_2 - 100.0_f64.to_radians();
        assert!((resolve_rotation(&bullet, options) - expected).abs() < 1e-12);
    }
}
