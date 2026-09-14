use super::*;
use crate::{
    constants::{SKY_MOON_APPARENT_RADIUS_DEGREES, SKY_SUN_APPARENT_RADIUS_DEGREES},
    test_fixtures,
};

#[test]
fn body_size_scales_multiply_real_apparent_radii() {
    let mut config = test_fixtures::client_settings().sky;
    config.sun.size_scale = 3.0;
    config.moon.size_scale = 2.5;
    let material = ProceduralSkyMaterial::from_config(config);
    assert!((material.sun.x - (SKY_SUN_APPARENT_RADIUS_DEGREES * 3.0).to_radians()).abs() < 1e-6);
    assert!((material.moon.x - (SKY_MOON_APPARENT_RADIUS_DEGREES * 2.5).to_radians()).abs() < 1e-6);
}
