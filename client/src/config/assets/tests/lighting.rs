use crate::test_fixtures;

#[test]
fn invalid_celestial_directions_are_rejected_even_when_hidden() {
    let mut sky = test_fixtures::asset_set()
        .skybox("test")
        .expect("fixture sky missing")
        .clone();
    sky.celestial_disc.show = false;
    for direction in [[0.0; 3], [f32::NAN, 1.0, 0.0], [0.0, f32::INFINITY, 0.0]] {
        sky.celestial_disc.direction = direction;
        let error = sky.validate("skyboxes.test").expect_err("invalid direction accepted");
        assert!(error.to_string().contains("skyboxes.test.celestial_disc.direction"));
    }
    for direction in [[0.0, 1.0, 0.0], [0.0, -1.0, 0.0], [2.0, 3.0, 4.0]] {
        sky.celestial_disc.direction = direction;
        sky.validate("skyboxes.test").expect("valid direction rejected");
    }
}

#[test]
fn invalid_disc_looks_identify_the_asset_field() {
    let mut sky = test_fixtures::asset_set()
        .skybox("test")
        .expect("fixture sky missing")
        .clone();
    sky.celestial_disc.dim.phase_percent = 101.0;
    let error = sky.validate("skyboxes.test").expect_err("invalid phase accepted");
    assert!(error.to_string().contains("celestial_disc.dim.phase_percent"));
    sky.celestial_disc.dim.phase_percent = 50.0;
    sky.celestial_disc.dark.luminance = -1.0;
    let error = sky.validate("skyboxes.test").expect_err("negative luminance accepted");
    assert!(error.to_string().contains("celestial_disc.dark.luminance"));
}
