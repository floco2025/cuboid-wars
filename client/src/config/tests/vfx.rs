use super::*;
use serde_json::json;

#[test]
fn invalid_visual_controls_report_their_config_paths() {
    for (path, invalid) in [
        ("pickups.emissive_brightness", -1.0),
        ("fields.emissive_brightness", -1.0),
        ("fields.rail_emissive_brightness", -1.0),
        ("fields.opacity", 1.1),
        ("fields.passable_opacity", -0.1),
        ("fields.passable_opacity", 0.9),
        ("fields.fade_secs", 0.0),
        ("erasers.emissive_brightness", -1.0),
        ("erasers.rail_emissive_brightness", -1.0),
        ("erasers.opacity", -0.1),
        ("erasers.opacity", 1.1),
    ] {
        let mut value = json!({
            "pickups": { "emissive_brightness": 1.0 },
            "fields": { "emissive_brightness": 2.0, "rail_emissive_brightness": 1.0, "opacity": 0.5, "passable_opacity": 0.1, "fade_secs": 0.25 },
            "erasers": { "emissive_brightness": 3.0, "rail_emissive_brightness": 1.0, "opacity": 0.2 }
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
        "fields": { "emissive_brightness": 0.0, "rail_emissive_brightness": 0.0, "opacity": 0.0, "passable_opacity": 0.0, "fade_secs": 0.25 },
        "erasers": { "emissive_brightness": 0.0, "rail_emissive_brightness": 0.0, "opacity": 0.0 }
    }))
    .expect("zero-value VFX config failed to deserialize");
    config.validate().expect("zero emission or opacity rejected");
}
