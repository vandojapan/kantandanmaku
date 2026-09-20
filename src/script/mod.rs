//! Rhai-backed danmaku script runtime.

mod api;
mod engine;

pub use engine::{
    CompiledScript, ScriptContext, ScriptEngine, ScriptError, ScriptEvaluator, SpawnValues,
    MAX_SCRIPT_SOURCE_BYTES,
};

/// The four acceptance-test patterns are ordinary scripts selected by the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    AimedOdd,
    QuantizedAimed,
    SpeedRing,
    Hybrid,
}

impl Preset {
    pub fn source(self) -> &'static str {
        match self {
            Self::AimedOdd => {
                r#"
spawn {
    count = 5;
    angle = aim() + (i - (count - 1) / 2) * deg(8);
    speed = 420;
    life = 6;
}

motion {
    pos = polar(angle, speed * age);
    x = origin_x + pos.x;
    y = origin_y + pos.y;
}
"#
            }
            Self::QuantizedAimed => {
                r#"
spawn {
    count = 5;
    base_angle = quantize(aim(), deg(5.625));
    angle = base_angle + (i - (count - 1) / 2) * deg(7);
    speed = 620;
    life = 5;
}

motion {
    pos = polar(angle, speed * age);
    x = origin_x + pos.x;
    y = origin_y + pos.y;
}
"#
            }
            Self::SpeedRing => {
                r#"
spawn {
    ring_count = 24;
    layers = 3;
    count = ring_count * layers;
    layer = floor(i / ring_count);
    slot = i % ring_count;
    angle = ring(slot, ring_count);
    speed = 280 + layer * 140;
    life = 6;
}

motion {
    pos = polar(angle, speed * age);
    x = origin_x + pos.x;
    y = origin_y + pos.y;
}
"#
            }
            Self::Hybrid => {
                r#"
spawn {
    ways = 5;
    layers = 3;
    count = ways * layers;
    layer = floor(i / ways);
    slot = i % ways;
    base_angle = quantize(aim(), deg(5.625));
    phase = sin(wave * deg(30)) * deg(6);
    angle = base_angle + phase + (slot - (ways - 1) / 2) * deg(8);
    speed = 320 + layer * 140;
    life = 6;
}

motion {
    pos = polar(angle, speed * age);
    x = origin_x + pos.x;
    y = origin_y + pos.y;
}
"#
            }
        }
    }
}

pub fn preset_from_value(value: i32) -> Preset {
    match value {
        1 => Preset::QuantizedAimed,
        2 => Preset::SpeedRing,
        3 => Preset::Hybrid,
        _ => Preset::AimedOdd,
    }
}
