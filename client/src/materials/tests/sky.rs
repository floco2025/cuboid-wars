use super::*;
use crate::{
    constants::{SKY_MOON_APPARENT_RADIUS_DEGREES, SKY_SUN_APPARENT_RADIUS_DEGREES},
    test_fixtures,
};

#[test]
fn body_size_scales_multiply_real_apparent_radii() {
    let config = test_fixtures::client_settings().sky;
    let material = ProceduralSkyMaterial::from_config(config);
    assert!((material.sun.x - (SKY_SUN_APPARENT_RADIUS_DEGREES * config.sun.size_scale).to_radians()).abs() < 1e-6);
    assert!((material.moon.x - (SKY_MOON_APPARENT_RADIUS_DEGREES * config.moon.size_scale).to_radians()).abs() < 1e-6);
    assert!(
        config.sun.size_scale > 1.0,
        "shipped sun should be creatively exaggerated"
    );
    assert!(
        config.moon.size_scale > 1.0,
        "shipped moon should be creatively exaggerated"
    );
}

#[test]
fn moon_earthshine_is_only_a_faint_hint() {
    let material = ProceduralSkyMaterial::from_config(test_fixtures::client_settings().sky);
    assert!(material.moon.z <= 0.001);
    assert_eq!(material.moon.w, 0.0);
    let shader = include_str!("../sky.wgsl");
    assert!(!shader.contains("crater"));
    assert!(!shader.contains("value_noise"));
}
