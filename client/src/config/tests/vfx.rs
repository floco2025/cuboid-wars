use super::*;
use serde_json::json;

#[test]
fn invalid_visual_controls_report_their_config_paths() {
    for (path, invalid) in [
        ("pickups.emissive_brightness", -1.0),
        ("barriers.emissive_brightness", -1.0),
        ("barriers.opacity", 1.1),
        ("barriers.pulse.min_opacity", -0.1),
        ("barriers.pulse.min_opacity", 0.6),
        ("barriers.pulse.frequency_hz", -1.0),
        ("erasers.emissive_brightness", -1.0),
        ("erasers.opacity", -0.1),
        ("erasers.opacity", 1.1),
        ("light_bridges.emissive_brightness", -1.0),
        ("light_bridges.opacity", 1.1),
        ("light_bridges.unpowered_opacity", -0.1),
        ("light_bridges.unpowered_opacity", 0.9),
        ("light_bridges.fade_secs", 0.0),
    ] {
        let mut value = json!({
            "pickups": { "emissive_brightness": 1.0 },
            "barriers": { "emissive_brightness": 2.0, "opacity": 0.5, "pulse": { "min_opacity": 0.1, "frequency_hz": 0.5 } },
            "erasers": { "emissive_brightness": 3.0, "opacity": 0.2 },
            "light_bridges": { "emissive_brightness": 1.0, "opacity": 0.5, "unpowered_opacity": 0.1, "fade_secs": 0.25 }
        });
        let mut field = &mut value;
        for key in path.split('.') {
            field = &mut field[key];
        }
        *field = json!(invalid);
        let config: VfxConfig = serde_json::from_value(value).expect("invalid-value VFX fixture failed to deserialize");
        let error = config.validate().expect_err("invalid VFX setting accepted");
        assert!(
            error.to_string().contains(&format!("vfx.{path}")),
            "wrong path in {error}"
        );
    }
}

#[test]
fn emission_and_opacity_can_be_zero() {
    let config: VfxConfig = serde_json::from_value(json!({
        "pickups": { "emissive_brightness": 0.0 },
        "barriers": {
            "emissive_brightness": 0.0,
            "opacity": 0.0,
            "pulse": { "min_opacity": 0.0, "frequency_hz": 0.0 }
        },
        "erasers": { "emissive_brightness": 0.0, "opacity": 0.0 },
        "light_bridges": { "emissive_brightness": 0.0, "opacity": 0.0, "unpowered_opacity": 0.0, "fade_secs": 0.25 }
    }))
    .expect("zero-value VFX config failed to deserialize");
    config.validate().expect("zero emission or opacity rejected");
}
