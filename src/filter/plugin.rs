use crate::danmaku::{evaluate_with_options, RuntimeInput, RuntimeLimits, RuntimeOptions};
use crate::render::{resolve_rotation, RotationOptions};
use crate::script::{Preset, ScriptEngine};
use aviutl2::{
    filter::{
        DrawImageParam, FilterConfigItem, FilterConfigItemSliceExt, FilterConfigItems,
        FilterPlugin, FilterPluginFlags, FilterPluginTable, FilterProcVideo, ImageResource,
    },
    AnyResult, AviUtl2Info,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, aviutl2::filter::FilterConfigSelectItems)]
enum ScriptMode {
    #[item(name = "プリセット")]
    Preset,
    #[item(name = "カスタム")]
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, aviutl2::filter::FilterConfigSelectItems)]
enum PresetChoice {
    #[item(name = "自機狙い奇数5方向")]
    AimedOdd,
    #[item(name = "角度量子化自機狙い")]
    QuantizedAimed,
    #[item(name = "速度差リング")]
    SpeedRing,
    #[item(name = "混成弾幕")]
    Hybrid,
}

impl PresetChoice {
    fn script(self) -> &'static str {
        match self {
            Self::AimedOdd => Preset::AimedOdd.source(),
            Self::QuantizedAimed => Preset::QuantizedAimed.source(),
            Self::SpeedRing => Preset::SpeedRing.source(),
            Self::Hybrid => Preset::Hybrid.source(),
        }
    }
}

#[aviutl2::filter::filter_config_items]
#[derive(Debug, Clone, PartialEq)]
struct FilterConfig {
    #[select(
        name = "スクリプトモード",
        items = ScriptMode,
        default = ScriptMode::Preset
    )]
    script_mode: ScriptMode,
    #[select(
        name = "プリセット",
        items = PresetChoice,
        default = PresetChoice::Hybrid
    )]
    preset: PresetChoice,
    #[check(name = "レイヤーを参照", default = false)]
    reference_layer: bool,
    #[string(name = "自機レイヤー", default = "2")]
    target_layer: String,
    #[track(name = "ターゲット X", range = -10000.0..=10000.0, step = 1.0, default = 320.0)]
    target_x: f64,
    #[track(name = "ターゲット Y", range = -10000.0..=10000.0, step = 1.0, default = 0.0)]
    target_y: f64,
    #[track(name = "発射間隔", range = 0.01..=10.0, step = 0.01, default = 0.2)]
    interval: f64,
    #[track(
        name = "シード",
        range = -2147483648.0..=2147483647.0,
        step = 1.0,
        default = 1.0
    )]
    seed: f64,
    #[check(name = "進行方向を向く", default = false)]
    follow_path: bool,
    #[track(name = "回転（移動量）", range = -10.0..=10.0, step = 0.01, default = 0.0)]
    rotation_per_pixel_degrees: f64,
    #[text(name = "カスタムスクリプト", default = "")]
    custom_script: String,
}

#[aviutl2::plugin(FilterPlugin)]
pub struct DanmakuFilter;

impl FilterPlugin for DanmakuFilter {
    type Userdata = ();

    fn new(_info: AviUtl2Info) -> AnyResult<Self> {
        Ok(Self)
    }

    fn plugin_info(&self) -> FilterPluginTable {
        FilterPluginTable {
            name: "弾幕生成フィルタ".to_string(),
            label: Some("弾幕生成".to_string()),
            information: format!(
                "AviUtl2向け決定論的弾幕スクリプトフィルタ・プロトタイプ v{}",
                env!("CARGO_PKG_VERSION")
            ),
            flags: aviutl2::bitflag!(FilterPluginFlags {
                video: true,
                filter: true,
            }),
            config_items: FilterConfig::to_config_items(),
        }
    }

    fn proc_video(
        &self,
        config: &[FilterConfigItem],
        video: &mut FilterProcVideo<Self::Userdata>,
    ) -> AnyResult<()> {
        let config: FilterConfig = config.to_struct();
        let source = if config.script_mode == ScriptMode::Custom
            && !config.custom_script.trim().is_empty()
        {
            config.custom_script.as_str()
        } else {
            config.preset.script()
        };
        let (target_x, target_y) = resolve_target(&config, video);

        let compiled = ScriptEngine::compile(source)
            .map_err(|error| aviutl2::anyhow::anyhow!(error.to_string()))?;
        let bullets = evaluate_with_options(
            &compiled,
            RuntimeInput {
                time: video.object.time,
                interval: config.interval,
                seed: config.seed.round() as i64,
                // AviUtl2 draw_image coordinates are relative to the current
                // object's anchor.  The prototype treats that anchor as the
                // virtual emitter origin (0, 0).
                origin_x: 0.0,
                origin_y: 0.0,
                target_x,
                target_y,
            },
            RuntimeLimits::default(),
            RuntimeOptions {
                calculate_heading: config.follow_path,
                ..RuntimeOptions::default()
            },
        )
        .map_err(|error| aviutl2::anyhow::anyhow!(error.to_string()))?;

        let object = ImageResource::Object;
        let rotation_options = RotationOptions {
            follow_path: config.follow_path,
            rotation_per_pixel_degrees: config.rotation_per_pixel_degrees,
        };
        for resolved in bullets {
            let mut param = DrawImageParam::default();
            param.x = resolved.state.x as f32;
            param.y = resolved.state.y as f32;
            param.rz = resolve_rotation(&resolved, rotation_options).to_degrees() as f32;
            param.sx = resolved.state.scale_x as f32;
            param.sy = resolved.state.scale_y as f32;
            param.alpha = resolved.state.alpha as f32;
            video.draw_image(&object, param)?;
        }

        // draw_image() owns the output in this filter, so do not let AviUtl2
        // apply the normal post-filter object draw a second time.
        video.set_default_anchor(0, 0);
        Ok(())
    }
}

fn resolve_target(config: &FilterConfig, video: &mut FilterProcVideo<()>) -> (f64, f64) {
    if config.reference_layer {
        if let Some(target) = resolve_layer_target(&config.target_layer, video) {
            return target;
        }
    }
    (config.target_x, config.target_y)
}

fn resolve_layer_target(user_layer: &str, video: &mut FilterProcVideo<()>) -> Option<(f64, f64)> {
    let api_layer = parse_target_layer(user_layer)?;
    // The active object on the filter's own layer is the source object itself.
    // Avoid asking AviUtl2 to recursively resolve its output parameters.
    if api_layer == video.object.layer {
        return None;
    }

    let target_object = video.get_image_object(api_layer, 0.0)?;
    let target = video
        .get_output_image_param(Some(target_object), 0.0)
        .ok()?;
    let emitter = video.get_output_image_param(None, 0.0).ok()?;
    local_target_coordinates(target.x, target.y, emitter.x, emitter.y)
}

fn parse_target_layer(user_layer: &str) -> Option<u32> {
    user_layer
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|layer| (1..=100).contains(layer))
        .map(|layer| layer - 1)
}

fn local_target_coordinates(
    target_x: f32,
    target_y: f32,
    emitter_x: f32,
    emitter_y: f32,
) -> Option<(f64, f64)> {
    let x = f64::from(target_x - emitter_x);
    let y = f64::from(target_y - emitter_y);
    (x.is_finite() && y.is_finite()).then_some((x, y))
}

aviutl2::register_filter_plugin!(DanmakuFilter);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_uses_checkbox_layer_field_and_movement_rotation_track() {
        let items = FilterConfig::to_config_items();
        let layer_reference = items
            .iter()
            .find(|item| item.name() == "レイヤーを参照")
            .expect("layer reference checkbox is missing");
        let target_layer = items
            .iter()
            .find(|item| item.name() == "自機レイヤー")
            .expect("target layer field is missing");
        let movement_rotation = items
            .iter()
            .find(|item| item.name() == "回転（移動量）")
            .expect("movement rotation track is missing");

        assert!(matches!(layer_reference, FilterConfigItem::Check(_)));
        assert!(matches!(target_layer, FilterConfigItem::String(_)));
        assert!(matches!(movement_rotation, FilterConfigItem::Track(_)));
        assert!(!items.iter().any(|item| item.name() == "自機座標モード"));
    }

    #[test]
    fn layer_target_is_converted_from_scene_to_emitter_local_coordinates() {
        assert_eq!(
            local_target_coordinates(500.0, 100.0, 120.0, -20.0),
            Some((380.0, 120.0))
        );
        assert_eq!(local_target_coordinates(f32::NAN, 0.0, 0.0, 0.0), None);
    }

    #[test]
    fn target_layer_field_accepts_only_layers_one_through_one_hundred() {
        assert_eq!(parse_target_layer("1"), Some(0));
        assert_eq!(parse_target_layer(" 25 "), Some(24));
        assert_eq!(parse_target_layer("100"), Some(99));
        assert_eq!(parse_target_layer("0"), None);
        assert_eq!(parse_target_layer("101"), None);
        assert_eq!(parse_target_layer("2.5"), None);
        assert_eq!(parse_target_layer("abc"), None);
    }
}
