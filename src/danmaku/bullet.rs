use super::math::{polar, Vec2};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bullet {
    pub i: u32,
    pub wave: i64,
    pub age: f64,
    pub origin_x: f64,
    pub origin_y: f64,
    pub angle: f64,
    pub speed: f64,
    pub life: f64,
}

impl Bullet {
    pub fn is_alive(&self) -> bool {
        self.age >= 0.0 && self.age < self.life
    }

    pub fn position(&self) -> Vec2 {
        let offset = polar(self.angle, self.speed * self.age);
        Vec2 {
            x: self.origin_x + offset.x,
            y: self.origin_y + offset.y,
        }
    }
}

/// The renderer-facing state.  Rotation, scale and alpha are present even
/// though the first prototype only requires scripts to set x/y.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BulletState {
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub alpha: f64,
}

impl Default for BulletState {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            alpha: 1.0,
        }
    }
}
